//! Live sessions (milestone M9, step 2 of `docs/collab-protocol.md`): people edit the same file
//! together and see each other's cursors, selections and viewports, and can follow one another.
//!
//! The engine holds what everyone sees ([`Live`]: status, peers and their presence, who you
//! follow) and the `collab.*` commands. The networking lives in a higher layer
//! (`figcraft-collab-net`), which the app installs as a [`LiveLink`]: the engine never opens a
//! socket, so nothing below the UI layer depends on networking. Without a link (the CLI, tests)
//! the commands say live sessions aren't available.
//!
//! Peers, presence and document changes are the wire types of the collaboration core
//! ([`figcraft_collab::message`]). Each participant runs a core [`Client`]: the host's session
//! document is the one it shared, a guest gets the host's file as a new tab from the welcome.
//! Every command, gesture or group on the session document is synced as one batch and is one step
//! of the user's own undo history ([`Session::record`]); undo and redo there take back only the
//! user's own steps ([`figcraft_collab::Client::undo`]). Batches from the host's server are applied
//! through the client and installed with [`Session::apply_remote`], which never touches the undo
//! history. After a dropped connection the client resumes with its id and catches up; what it
//! made meanwhile goes out then.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;

use figcraft_collab::Client;
use figcraft_collab::message::{ClientMsg, EditingText, ServerMsg};
use figcraft_doc::{Document, NodeId};
use serde::Serialize;
use serde_json::{Value, json};

pub use figcraft_collab::message::{Peer, Presence};

use crate::cmd::CommandSpec;
use crate::collab::EngineRecompute;
use crate::params::{get, num, req_string, string};
use crate::{DocState, EngineError, Result, Session, ViewState, bad, spec};

/// Most selected ids one presence update carries (the rest aren't shown to others).
pub const MAX_SELECTION: usize = 10_000;
/// Longest name shown to others (characters).
pub const MAX_NAME: usize = 64;

/// The colours people get in a session, in the order the host hands them out (`#RRGGBB`, white
/// text reads on each).
pub const PEOPLE: [&str; 8] = ["#0D99FF", "#9747FF", "#14AE5C", "#F24822", "#D9730D", "#E83E8C", "#00A3A3", "#665CE0"];

/// `#RRGGBB` → (r, g, b); `None` for anything else.
pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.is_ascii())?;
    let byte = |i: usize| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// The connection to a live session.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Status {
    /// Not in a session.
    #[default]
    Off,
    /// Joining (connecting to the host, waiting for its welcome).
    Connecting,
    /// In the session.
    Live,
    /// The connection dropped; trying again (`attempt` from 1).
    Reconnecting { attempt: u32 },
    /// Couldn't join, or the connection was lost for good.
    Failed { reason: String },
}

impl Status {
    pub fn id(&self) -> &'static str {
        match self {
            Status::Off => "off",
            Status::Connecting => "connecting",
            Status::Live => "live",
            Status::Reconnecting { .. } => "reconnecting",
            Status::Failed { .. } => "failed",
        }
    }

    /// In a session or on the way into one.
    pub fn active(&self) -> bool {
        matches!(self, Status::Connecting | Status::Live | Status::Reconnecting { .. })
    }

    /// What the Share popover says about it.
    pub fn label(&self) -> String {
        match self {
            Status::Off => "Not in a live session".into(),
            Status::Connecting => "Joining…".into(),
            Status::Live => "Live".into(),
            Status::Reconnecting { attempt } => format!("Connection lost, reconnecting (attempt {attempt})…"),
            Status::Failed { reason } => reason.clone(),
        }
    }
}

/// Your part in the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Host,
    Guest,
}

/// A page of the shared file.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageInfo {
    pub id: u64,
    pub name: String,
}

/// The session you're in.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub role: Role,
    /// Your client id, name and colour as the others see them.
    pub me: u32,
    pub name: String,
    pub color: String,
    /// The shared file's name and pages (the host's).
    pub file_name: String,
    pub pages: Vec<PageInfo>,
    /// The invite link (`figcraft://join?host=…&file=…&token=…`) and the same as a plain
    /// WebSocket URL. Both carry the session's secret token: share them only with people you
    /// invite.
    pub link: String,
    pub url: String,
    /// `ip:port` the session listens on (host) or connects to (guest).
    pub address: String,
    /// Reachable from other machines (the host listens on a network address, not loopback).
    pub network: bool,
}

/// What happened in the session since the last [`LiveLink::poll`].
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Status(Status),
    /// Joined, or joined again after a reconnect: who you are and who else is here.
    Welcome {
        info: SessionInfo,
        peers: Vec<Peer>,
    },
    PeerJoined(Peer),
    PeerLeft(u32),
    Presence(u32, Presence),
    /// For the document sync client: the welcome (with the whole document), or a batch of
    /// changes the server ordered, or the rejection of one of yours.
    Sync(ServerMsg),
    /// The host ended the session.
    Ended {
        reason: String,
    },
}

/// Which network interface a hosted session listens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    /// This machine only (127.0.0.1): the default.
    Local,
    /// The machine's address on the local network (only when the user chooses to share there).
    Lan,
    /// A given interface address (e.g. a VPN's).
    Addr(IpAddr),
}

/// What [`LiveLink::host`] needs to start a session for the open file.
#[derive(Clone, Debug)]
pub struct HostOptions {
    /// `None`: the default port, or any free one when it's taken.
    pub port: Option<u16>,
    pub bind: Bind,
    /// Your name (empty: the host picks one).
    pub name: String,
    /// The shared file (the session's authoritative copy starts from it).
    pub doc: Arc<Document>,
}

/// The networking behind live sessions, installed by the app ([`Live::install`]). Calls return
/// at once; what happens later arrives through [`poll`](Self::poll).
pub trait LiveLink: Send {
    /// Start hosting a session for `opts.doc` (ending any current one).
    fn host(&mut self, opts: HostOptions) -> std::result::Result<SessionInfo, String>;
    /// Start joining the session behind an invite link (ending any current one). Errors are for
    /// links that can't be used; they never echo the link (it holds the token).
    fn join(&mut self, link: &str, name: &str) -> std::result::Result<(), String>;
    /// End the current session (stop hosting or leave).
    fn leave(&mut self);
    /// Your latest presence (the link throttles and coalesces).
    fn presence(&mut self, p: &Presence);
    /// A message for the server (document changes), sent in order.
    fn send(&mut self, msg: ClientMsg);
    /// Events since the last call; never blocks.
    fn poll(&mut self) -> Vec<Event>;
}

/// The view a follow last applied: any other view means the user moved.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FollowedView {
    page: NodeId,
    cx: f64,
    cy: f64,
    zoom: f64,
}

/// Live session state (see the module docs).
#[derive(Default)]
pub struct Live {
    link: Option<Box<dyn LiveLink>>,
    pub status: Status,
    pub info: Option<SessionInfo>,
    /// Everyone else in the session, by client id.
    pub peers: BTreeMap<u32, Peer>,
    /// Their latest presence (only while they're in the session).
    pub presence: BTreeMap<u32, Presence>,
    /// The peer whose viewport you follow.
    pub follow: Option<u32>,
    followed: Option<FollowedView>,
    /// Your pointer over the canvas (page coordinates): the UI sets it, or `collab.cursor`.
    pub cursor: Option<(f64, f64)>,
    /// The presence last handed to the link.
    sent: Option<Presence>,
    /// The document sync client (from the welcome).
    client: Option<Client>,
    /// The open document the session syncs (its `DocState::uid`).
    pub doc_uid: Option<u64>,
    /// The session document as last sent or installed: a different one has local edits to send.
    synced: Option<Arc<Document>>,
    /// Server batches waiting for a gesture or preview on the session document to end.
    deferred: Vec<ServerMsg>,
    /// Nesting of [`Session::grouped`] on the session document: its edits sync once, at the end.
    hold: u32,
}

impl Live {
    /// Install the networking (the app does this once at startup).
    pub fn install(&mut self, link: Box<dyn LiveLink>) {
        self.link = Some(link);
    }

    /// A link is installed: sessions can be hosted and joined.
    pub fn available(&self) -> bool {
        self.link.is_some()
    }

    pub fn role(&self) -> Option<Role> {
        self.info.as_ref().map(|i| i.role)
    }

    /// Back to no session (keeps the link).
    fn reset(&mut self) {
        self.status = Status::Off;
        self.info = None;
        self.peers.clear();
        self.presence.clear();
        self.follow = None;
        self.followed = None;
        self.sent = None;
        self.client = None;
        self.doc_uid = None;
        self.synced = None;
        self.deferred.clear();
        self.hold = 0;
    }

    /// The document sync client (once the welcome arrived): peers, pending batches, the user's
    /// undo history.
    pub fn client(&self) -> Option<&Client> {
        self.client.as_ref()
    }

    /// Document changes are syncing (the welcome arrived).
    pub fn syncing(&self) -> bool {
        self.client.is_some()
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if let Some(l) = self.link.as_mut() {
            l.leave();
        }
    }
}

/// The page a presence is on (its viewport's, else its cursor's).
pub fn presence_page(p: &Presence) -> Option<NodeId> {
    p.viewport.map(|v| v.0).or(p.cursor.map(|c| c.0))
}

/// Presence ready to send: non-finite numbers and empty viewports dropped, the selection capped.
pub fn sanitize(mut p: Presence) -> Presence {
    p.cursor = p.cursor.filter(|c| c.1.is_finite() && c.2.is_finite());
    p.viewport = p.viewport.filter(|v| v.1.is_finite() && v.2.is_finite() && v.3.is_finite() && v.4.is_finite() && v.3 > 0.0 && v.4 > 0.0);
    if let Some(s) = p.selection.as_mut() {
        s.truncate(MAX_SELECTION);
    }
    p
}

/// The name others see: the name from Preferences, or empty for the default ("You"), which the
/// host replaces.
pub fn display_name(prefs: &crate::Prefs) -> String {
    let n = prefs.user_name.trim();
    if n == crate::cmd::comment::DEFAULT_AUTHOR { String::new() } else { n.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect() }
}

/// Initials for an avatar ("Ada Lovelace" → "AL"; "?" for none).
pub fn initials(name: &str) -> String {
    let mut out: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
    if out.is_empty() {
        out.push('?');
    }
    out.to_uppercase()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// A gesture, version preview or live text preview shows a document that isn't the edit to
/// sync yet.
fn busy(ds: &DocState) -> bool {
    ds.interaction.is_some() || ds.preview.is_some() || ds.text_preview.is_some()
}

impl Session {
    /// The active document is the session's synced document: page and layer ids in others'
    /// presence refer to it.
    pub fn live_shared(&self) -> bool {
        self.live.client.is_some() && self.ds().is_ok_and(|ds| Some(ds.uid) == self.live.doc_uid)
    }

    /// Install a document that came from the session (others' edits): the selection, page and view
    /// stay where they can, and the undo history isn't touched.
    pub fn apply_remote(&mut self, uid: u64, doc: Arc<Document>) {
        let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid) else { return };
        ds.doc = doc;
        if !ds.doc.pages.contains(&ds.page) {
            ds.page = ds.doc.first_page().unwrap_or(ds.page);
        }
        if ds.text_edit.as_ref().is_some_and(|t| !ds.doc.contains(t.id)) {
            ds.text_edit = None;
        }
        if ds.vector_edit.as_ref().is_some_and(|v| !ds.doc.contains(v.id)) {
            ds.vector_edit = None;
        }
        ds.clean_selection();
        ds.revision += 1;
    }

    /// Your presence now: the session document's page, selection, viewport and edited text, and
    /// the pointer (nothing while another document is in front).
    pub fn live_presence(&self) -> Presence {
        let Ok(ds) = self.ds() else { return Presence::default() };
        if self.live.doc_uid.is_some_and(|u| u != ds.uid) {
            return Presence::default();
        }
        let (x0, y0) = ds.view.origin();
        let z = ds.view.zoom.max(1e-9);
        sanitize(Presence {
            cursor: self.live.cursor.map(|(x, y)| (ds.page, x, y)),
            selection: Some(ds.selection.clone()),
            viewport: Some((ds.page, x0, y0, ds.view.vw / z, ds.view.vh / z)),
            editing_text: ds.text_edit.as_ref().map(|t| {
                let (a, b) = (t.anchor.min(t.caret), t.anchor.max(t.caret));
                Box::new(EditingText { node: t.id, caret: t.caret, selection: (a != b).then_some((a, b)) })
            }),
        })
    }

    /// Take in what happened in the session (the UI calls it every frame; the network wakes the
    /// UI when something arrives). Returns whether anything changed.
    pub fn live_poll(&mut self) -> bool {
        let Some(link) = self.live.link.as_mut() else { return false };
        let events = link.poll();
        if events.is_empty() {
            return false;
        }
        for e in events {
            self.live_event(e);
        }
        self.live_deferred();
        self.live_follow_apply();
        true
    }

    fn live_event(&mut self, e: Event) {
        let live = &mut self.live;
        match e {
            Event::Status(st) => {
                match &st {
                    Status::Failed { reason } => {
                        self.message = Some(reason.clone());
                        live.reset();
                    }
                    Status::Off => live.reset(),
                    // Who's here comes again with the next welcome; edits wait for it.
                    Status::Reconnecting { .. } => {
                        live.peers.clear();
                        live.presence.clear();
                        live.sent = None;
                        if let Some(c) = live.client.as_mut() {
                            c.disconnect();
                        }
                    }
                    Status::Connecting | Status::Live => {}
                }
                live.status = st;
            }
            Event::Welcome { mut info, peers } => {
                // The shared file's name and pages come with the document (the sync welcome).
                if let Some(old) = live.info.as_ref().filter(|_| info.file_name.is_empty()) {
                    info.file_name = old.file_name.clone();
                    info.pages = old.pages.clone();
                }
                live.status = Status::Live;
                live.info = Some(info);
                live.peers = peers.into_iter().map(|p| (p.client_id, p)).collect();
                live.presence.retain(|id, _| live.peers.contains_key(id));
                if live.follow.is_some_and(|f| !live.peers.contains_key(&f)) {
                    live.follow = None;
                    live.followed = None;
                }
                // Others need your presence again.
                live.sent = None;
            }
            Event::PeerJoined(p) => {
                self.message = Some(format!("{} joined", p.name));
                live.peers.insert(p.client_id, p);
            }
            Event::PeerLeft(id) => {
                live.presence.remove(&id);
                if let Some(p) = live.peers.remove(&id) {
                    self.message = Some(format!("{} left", p.name));
                }
                if live.follow == Some(id) {
                    live.follow = None;
                    live.followed = None;
                }
            }
            Event::Presence(id, p) => {
                if live.peers.contains_key(&id) {
                    live.presence.insert(id, p);
                }
            }
            // Back after a dropped connection: the client catches up in order with the rest.
            Event::Sync(msg @ ServerMsg::Welcome { .. }) if live.client.is_some() => live.deferred.push(msg),
            Event::Sync(ServerMsg::Welcome { .. }) => {
                if let Event::Sync(w) = e {
                    self.live_welcome(w);
                }
            }
            Event::Sync(msg) => {
                live.deferred.push(msg);
            }
            Event::Ended { reason } => {
                self.message = Some(reason);
                live.reset();
            }
        }
    }

    /// The welcome: start the sync client; the host keeps editing the document it shared, a guest
    /// opens the host's file as a new tab (or gets it again in that tab after a reconnect).
    fn live_welcome(&mut self, welcome: ServerMsg) {
        let client = match Client::join(welcome) {
            Ok(c) => c,
            Err(e) => return self.live_fail(format!("Couldn't open the shared file: {e}")),
        };
        let role = self.live.role();
        let existing = self.live.doc_uid.filter(|u| self.docs.iter().any(|d| d.uid == *u));
        match (role, existing) {
            (Some(Role::Host), Some(uid)) => {
                if let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid) {
                    let was_dirty = ds.is_dirty();
                    // Undo is per user from now on: earlier whole-document steps would revert
                    // others' edits.
                    ds.history.undo.clear();
                    ds.history.redo.clear();
                    ds.doc_mut().set_client_id(client.id());
                    if !was_dirty {
                        ds.mark_saved();
                    }
                }
                // Edits made since hosting started go out with the first sync.
                self.live.synced = None;
            }
            (Some(Role::Guest), Some(uid)) => {
                let doc = Arc::new(client.document().clone());
                self.apply_remote(uid, doc.clone());
                self.live.synced = Some(doc);
            }
            (Some(Role::Guest), None) => {
                let ds = DocState::new(client.document().clone(), None);
                self.message = Some(format!("Joined {}", ds.doc.name));
                self.live.doc_uid = Some(ds.uid);
                self.live.synced = Some(ds.doc.clone());
                self.docs.push(ds);
                self.active = self.docs.len() - 1;
            }
            _ => return,
        }
        let doc = client.document();
        if let Some(info) = self.live.info.as_mut() {
            info.file_name = doc.name.clone();
            info.pages = doc.pages.iter().map(|p| PageInfo { id: p.0, name: doc.node(*p).map(|n| n.name.clone()).unwrap_or_default() }).collect();
        }
        self.live.client = Some(client);
        self.live.deferred.clear();
    }

    /// End the session with a message (sync broke).
    fn live_fail(&mut self, reason: String) {
        log::warn!("live session: {reason}");
        if let Some(l) = self.live.link.as_mut() {
            l.leave();
        }
        self.live.reset();
        self.live.status = Status::Failed { reason: reason.clone() };
        self.message = Some(reason);
    }

    /// Send the session document's edits not synced yet (none, unless something changed it
    /// outside a command), unless a gesture or preview is still showing.
    fn live_flush(&mut self) {
        self.live_edit(None);
    }

    /// Whether document `uid` is the live session's synced document: its edits sync and its undo
    /// history is the user's own.
    pub fn live_doc(&self, uid: u64) -> bool {
        self.live.client.is_some() && self.live.doc_uid == Some(uid)
    }

    /// Sync the session document's edits as one batch: one step of the user's undo history called
    /// `label`, or not undoable (`None`). Nothing while a gesture or preview shows, or inside a
    /// group (it syncs when the group ends).
    pub(crate) fn live_edit(&mut self, label: Option<&str>) {
        let Some(uid) = self.live.doc_uid else { return };
        let Some(ds) = self.docs.iter().find(|d| d.uid == uid) else { return };
        if self.live.hold > 0 || busy(ds) || self.live.synced.as_ref().is_some_and(|s| Arc::ptr_eq(s, &ds.doc)) {
            return;
        }
        let doc = ds.doc.clone();
        let Some(client) = self.live.client.as_mut() else { return };
        match client.edit_with(&doc, label) {
            Ok(Some(msg)) => {
                if let Some(l) = self.live.link.as_mut() {
                    l.send(msg);
                }
            }
            Ok(None) => {}
            Err(e) => return self.live_fail(format!("Couldn't sync an edit: {e}")),
        }
        self.live.synced = Some(doc);
    }

    /// A group of commands on document `uid` starts (`on`) or ends: in between its edits don't
    /// sync on their own.
    pub(crate) fn live_hold(&mut self, uid: u64, on: bool) {
        if self.live_doc(uid) {
            self.live.hold = if on { self.live.hold.saturating_add(1) } else { self.live.hold.saturating_sub(1) };
        }
    }

    /// Undo (`back`) or redo the user's own last step on the session document: what it wrote goes
    /// back where nobody else wrote since. Returns whether there was a step to take back.
    pub(crate) fn live_travel(&mut self, back: bool) -> bool {
        let Some(uid) = self.live.doc_uid else { return false };
        // Edits not synced yet go first.
        self.live_flush();
        let Some(client) = self.live.client.as_mut() else { return false };
        let (undo, redo) = client.history_len();
        let msgs = match if back { client.undo(&EngineRecompute) } else { client.redo(&EngineRecompute) } {
            Ok(m) => m,
            Err(e) => {
                log::warn!("live session: {} failed: {e}", if back { "undo" } else { "redo" });
                return false;
            }
        };
        let (u, r) = client.history_len();
        let did = if back { r > redo } else { u > undo };
        let doc = Arc::new(client.document().clone());
        if let Some(l) = self.live.link.as_mut() {
            for m in msgs {
                l.send(m);
            }
        }
        if did {
            self.apply_remote(uid, doc.clone());
            self.live.synced = Some(doc);
        }
        did
    }

    /// Apply the server's batches that waited (none while a gesture or preview shows).
    fn live_deferred(&mut self) {
        if self.live.deferred.is_empty() || self.live.client.is_none() {
            return;
        }
        let Some(uid) = self.live.doc_uid else { return };
        if self.docs.iter().find(|d| d.uid == uid).is_none_or(busy) {
            return;
        }
        // Your own edits first: the client replays them over what others did.
        self.live_flush();
        let msgs = std::mem::take(&mut self.live.deferred);
        let mut out = Vec::new();
        let Some(client) = self.live.client.as_mut() else { return };
        for m in msgs {
            match client.receive(m, &EngineRecompute) {
                Ok(more) => out.extend(more),
                Err(e) => return self.live_fail(format!("The live session got out of step ({e}); join again")),
            }
        }
        let doc = Arc::new(client.document().clone());
        // Edits the server refused were rolled back: say why (the UI's toast).
        let notices: Vec<String> = client.take_notices().iter().map(figcraft_collab::Notice::message).collect();
        if !notices.is_empty() {
            self.message = Some(notices.join("; "));
        }
        if let Some(l) = self.live.link.as_mut() {
            for m in out {
                l.send(m);
            }
        }
        self.apply_remote(uid, doc.clone());
        self.live.synced = Some(doc);
    }

    /// Show the followed peer's viewport (and their page, in a shared document).
    fn live_follow_apply(&mut self) {
        let Some(id) = self.live.follow else { return };
        if !self.live.peers.contains_key(&id) {
            self.live.follow = None;
            self.live.followed = None;
            return;
        }
        let Some((page, x, y, w, h)) = self.live.presence.get(&id).and_then(|p| p.viewport) else { return };
        let shared = self.live_shared();
        let Ok(ds) = self.ds_mut() else { return };
        if shared && page != ds.page && ds.doc.pages.contains(&page) {
            ds.page = page;
            ds.selection.clear();
            ds.text_edit = None;
        }
        if !(w > 0.0 && h > 0.0 && x.is_finite() && y.is_finite()) {
            return;
        }
        ds.view.cx = x + w / 2.0;
        ds.view.cy = y + h / 2.0;
        let zoom = (ds.view.vw / w).min(ds.view.vh / h);
        if zoom.is_finite() {
            ds.view.zoom = zoom.clamp(ViewState::MIN_ZOOM, ViewState::MAX_ZOOM);
        }
        ds.revision += 1;
        self.live.followed = Some(FollowedView { page: ds.page, cx: ds.view.cx, cy: ds.view.cy, zoom: ds.view.zoom });
    }

    /// Send your presence when it changed (the UI calls it every frame it draws, after input;
    /// headless sessions after each call). A view you moved yourself stops following.
    pub fn live_sync(&mut self) {
        if self.live.status != Status::Live {
            return;
        }
        if self.live.doc_uid.is_some_and(|u| !self.docs.iter().any(|d| d.uid == u)) {
            let reason = "The live session's file was closed, so you left the session".to_string();
            if let Some(l) = self.live.link.as_mut() {
                l.leave();
            }
            self.live.reset();
            self.message = Some(reason);
            return;
        }
        self.live_deferred();
        self.live_flush();
        if self.live.status != Status::Live {
            return;
        }
        if let (Some(_), Some(f), Ok(ds)) = (self.live.follow, self.live.followed, self.ds())
            && (ds.page != f.page || !close(ds.view.cx, f.cx) || !close(ds.view.cy, f.cy) || !close(ds.view.zoom, f.zoom))
        {
            self.live.follow = None;
            self.live.followed = None;
        }
        let p = self.live_presence();
        if self.live.sent.as_ref() == Some(&p) {
            return;
        }
        if let Some(link) = self.live.link.as_mut() {
            link.presence(&p);
        }
        self.live.sent = Some(p);
    }

    /// Stop following (Escape, `collab.follow {}`). False when you weren't following anyone.
    pub fn live_unfollow(&mut self) -> bool {
        self.live.followed = None;
        self.live.follow.take().is_some()
    }

    /// Follow `id`'s viewport (shown at once when known).
    pub fn live_follow(&mut self, id: u32) -> Result<()> {
        if !self.live.peers.contains_key(&id) {
            return Err(bad("collab.follow", format!("nobody with client id {id} is in the session")));
        }
        self.live.follow = Some(id);
        self.live.followed = None;
        self.live_follow_apply();
        Ok(())
    }

    /// The session as JSON (`collab.peers`).
    pub fn live_summary(&self) -> Value {
        let l = &self.live;
        let info = l.info.as_ref();
        let peers: Vec<Value> = l
            .peers
            .values()
            .map(|p| json!({"id": p.client_id, "name": p.name, "color": p.color, "presence": l.presence.get(&p.client_id)}))
            .collect();
        json!({
            "available": l.available(),
            "status": l.status.id(),
            "reason": match &l.status { Status::Failed { reason } => Some(reason.as_str()), _ => None },
            "attempt": match l.status { Status::Reconnecting { attempt } => Some(attempt), _ => None },
            "role": info.map(|i| i.role),
            "me": info.map(|i| i.me),
            "name": info.map(|i| i.name.as_str()),
            "color": info.map(|i| i.color.as_str()),
            "fileName": info.map(|i| i.file_name.as_str()),
            "pages": info.map(|i| &i.pages),
            "link": info.map(|i| i.link.as_str()),
            "url": info.map(|i| i.url.as_str()),
            "address": info.map(|i| i.address.as_str()),
            "network": info.map(|i| i.network),
            "follow": l.follow,
            "syncing": l.syncing(),
            "document": l.doc_uid.and_then(|u| self.docs.iter().position(|d| d.uid == u)),
            "peers": peers,
        })
    }
}

fn unavailable(cmd: &str) -> EngineError {
    EngineError::Disabled(cmd.to_string(), "live sessions aren't available here (the desktop app and the MCP server host and join them)".into())
}

fn host(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "collab.host";
    let port = match get(p, "port") {
        None => None,
        Some(v) => Some(v.as_u64().and_then(|n| u16::try_from(n).ok()).ok_or_else(|| bad(CMD, "`port` must be a whole number from 0 to 65535"))?),
    };
    let bind = match string(p, "bind").as_deref().map(str::trim) {
        None | Some("local") => Bind::Local,
        Some("lan") => Bind::Lan,
        Some(other) => other.parse::<IpAddr>().map(Bind::Addr).map_err(|_| bad(CMD, "`bind` must be \"local\", \"lan\" or an IP address"))?,
    };
    let (doc, uid) = s.ds().map(|ds| (ds.doc.clone(), ds.uid))?;
    let opts = HostOptions { port, bind, name: display_name(&s.prefs), doc };
    let link = s.live.link.as_mut().ok_or_else(|| unavailable(CMD))?;
    let info = link.host(opts).map_err(|e| EngineError::Other(format!("couldn't start the live session: {e}")))?;
    s.live.reset();
    s.live.status = Status::Live;
    s.live.info = Some(info);
    s.live.doc_uid = Some(uid);
    Ok(s.live_summary())
}

fn join(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "collab.join";
    let link_text = req_string(CMD, p, "link")?;
    let name = display_name(&s.prefs);
    let link = s.live.link.as_mut().ok_or_else(|| unavailable(CMD))?;
    link.join(link_text.trim(), &name).map_err(|e| bad(CMD, e))?;
    s.live.reset();
    s.live.status = Status::Connecting;
    Ok(s.live_summary())
}

fn leave(s: &mut Session, _: &Value) -> Result<Value> {
    let was = s.live.status.active();
    if let Some(l) = s.live.link.as_mut() {
        l.leave();
    }
    s.live.reset();
    Ok(json!({"ended": was}))
}

fn follow(s: &mut Session, p: &Value) -> Result<Value> {
    match get(p, "client") {
        None => {
            s.live_unfollow();
        }
        Some(v) => {
            let id = v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad("collab.follow", "`client` must be a client id"))?;
            s.live_follow(id)?;
        }
    }
    Ok(json!({"follow": s.live.follow}))
}

fn cursor(s: &mut Session, p: &Value) -> Result<Value> {
    s.live.cursor = match (num(p, "x"), num(p, "y")) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => return Err(bad("collab.cursor", "give both `x` and `y` (page coordinates), or neither to hide the cursor")),
    };
    s.live_sync();
    Ok(json!({"cursor": s.live.cursor.map(|(x, y)| [x, y])}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "collab.host",
            "Start live session",
            "{port? (default 47900, else any free port), bind?: \"local\" (this machine, default) | \"lan\" (the local network) | an IP address} → the session: {link, url (invites: they carry the secret token), address, …}",
            false,
            host
        ),
        spec!("collab.stop", "Stop live session", "{} (also leaves a joined session) → {ended}", false, leave),
        spec!(
            "collab.join",
            "Join live session",
            "{link: figcraft://join?host=…&file=…&token=… or ws://host:port/?file=…&token=…} → status \"connecting\"; `collab.peers` tells when it's live",
            false,
            join
        ),
        spec!("collab.leave", "Leave live session", "{} → {ended}", false, leave),
        spec!(
            "collab.peers",
            "People in this file",
            "{} → {status: off|connecting|live|reconnecting|failed, reason?, role, me, name, color, fileName, pages, link, url, address, network, follow, syncing, document (index of the synced open document), peers: [{id, name, color, presence: {cursor: [page, x, y], selection, viewport: [page, x, y, w, h], editing_text}}]}",
            false,
            |s, _| Ok(s.live_summary())
        ),
        spec!("collab.follow", "Follow", "{client? (a peer's id; omit to stop following)} → {follow}", false, follow),
        spec!(
            "collab.cursor",
            "Show cursor at",
            "{x?, y? (page coordinates on the current page; omit both to hide)} (agents; in the app the pointer drives it)",
            false,
            cursor
        ),
    ]
}
