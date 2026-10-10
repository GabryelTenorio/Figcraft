//! The engine's side of real-time collaboration (`figcraft_collab`): its normalize pass as the
//! recompute hook, `collab.diff` (the change batch an edit syncs as), and documents in a live
//! session ([`LiveDoc`]): every edit is synced, undo and redo are per user, and what others do
//! arrives through [`Session::collab_receive`]. The transport stays outside the engine: it
//! attaches a joined [`Client`] with [`Session::collab_attach`], feeds server messages in and
//! sends what [`Session::collab_outgoing`] returns.

use std::collections::BTreeSet;
use std::sync::Arc;

use figcraft_collab::{Client, ClientMsg, Hello, Mode, Recompute, Replica, ServerMsg, diff, message};
use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::string;
use crate::{DocState, EngineError, Result, Session, bad, normalize, spec};

/// Derived data follows the engine's normalize pass: text boxes, groups, instances following
/// their mains, variable bindings and auto layout, recomputed over the touched nodes.
pub struct EngineRecompute;

impl Recompute for EngineRecompute {
    fn recompute(&self, doc: &mut Document, before: &Document, touched: &BTreeSet<NodeId>) {
        doc.mark_dirty(touched.iter().copied());
        normalize(doc, before);
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
        "collab.diff",
        "Show sync changes",
        "{against? (undo: the last undoable action, the default | saved: since the last save)} → the changes a live session would send, their count and binary size",
        false,
        diff_cmd
    )]
}

fn diff_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let ds = s.ds()?;
    let from: Arc<Document> = match string(p, "against").as_deref().unwrap_or("undo") {
        "undo" if ds.collab.is_some() => {
            return Err(EngineError::Disabled("collab.diff".into(), "in a live session undo is per user: compare against \"saved\"".into()));
        }
        "undo" => {
            ds.history.undo.last().map(|e| e.doc.clone()).ok_or_else(|| EngineError::Disabled("collab.diff".into(), "nothing to undo".into()))?
        }
        "saved" => ds.saved_doc.clone(),
        other => return Err(bad("collab.diff", format!("`against` is \"undo\" or \"saved\", not {other:?}"))),
    };
    let base = Replica::new((*from).clone());
    let d = diff(&base, &ds.doc).map_err(|e| EngineError::Other(e.to_string()))?;
    let bytes = message::encode(&d.changes).map_err(|e| EngineError::Other(e.to_string()))?.len();
    Ok(json!({"changes": d.changes, "count": d.changes.len(), "bytes": bytes}))
}

/// A document in a live session: its sync client, the messages waiting for the transport, and
/// what of the document is synced.
#[derive(Clone, Debug)]
pub struct LiveDoc {
    client: Client,
    outbox: Vec<ClientMsg>,
    /// Nesting of [`Session::grouped`]: the edits inside are synced once, as one undo step, when
    /// the outermost group ends.
    hold: u32,
    /// The document as last synced: a different `DocState::doc` holds edits not synced yet.
    synced: Arc<Document>,
}

impl LiveDoc {
    /// The session client (peers, presence, pending batches, the user's undo history).
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Sync `doc` as an edit: an undo step called `label`, or not undoable (`None`).
    fn record(&mut self, doc: &Arc<Document>, label: Option<&str>) {
        if self.hold > 0 || Arc::ptr_eq(doc, &self.synced) {
            return;
        }
        match self.client.edit_with(doc, label) {
            Ok(msg) => self.outbox.extend(msg),
            // Only a value that can't be encoded gets here; the next edit tries again.
            Err(e) => log::warn!("live session: an edit couldn't be synced: {e}"),
        }
        self.synced = doc.clone();
    }
}

impl DocState {
    /// Record an edit (the document changed from `before`, shown with `selection` on `page`) as
    /// one undo step called `label` (`None`: not undoable). In a live session the edit is synced
    /// and the step is the user's own; otherwise it is a snapshot of `before`.
    pub(crate) fn record(&mut self, label: Option<&str>, before: Arc<Document>, selection: Vec<NodeId>, page: NodeId) {
        if let Some(live) = self.collab.as_mut() {
            // A version shown in place of the document is no edit of it.
            if self.preview.is_none() {
                live.record(&self.doc, label);
            }
            return;
        }
        let Some(label) = label else { return };
        self.history.undo.push(crate::HistoryEntry { label: label.to_string(), doc: before, selection, page });
        if self.history.undo.len() > self.history.limit.max(1) {
            self.history.undo.remove(0);
        }
        self.history.redo.clear();
    }

    /// Edits inside a group are synced once, when it ends (see [`Session::grouped`]).
    pub(crate) fn hold_sync(&mut self, on: bool) {
        if let Some(live) = self.collab.as_mut() {
            live.hold = if on { live.hold.saturating_add(1) } else { live.hold.saturating_sub(1) };
        }
    }

    /// How many undo and redo steps there are (the user's own in a live session).
    pub fn history_len(&self) -> (usize, usize) {
        match &self.collab {
            Some(live) => live.client.history_len(),
            None => (self.history.undo.len(), self.history.redo.len()),
        }
    }

    /// The labels of the last `n` undo steps, the next one first.
    pub fn undo_labels(&self, n: usize) -> Vec<String> {
        match &self.collab {
            Some(live) => live.client.undo_labels().rev().take(n).map(str::to_string).collect(),
            None => self.history.undo.iter().rev().take(n).map(|h| h.label.clone()).collect(),
        }
    }

    /// Show the live session's document (the client's view), keeping what isn't synced (this
    /// file's published library). A gesture in progress carries on on top of it; a version shown
    /// in place of the document keeps showing.
    fn show_live(&mut self, before: &Replica) {
        let Some(live) = self.collab.as_mut() else { return };
        let real = self.preview.as_ref().map_or(&self.doc, |p| &p.doc);
        let mut doc = live.client.document().clone();
        doc.published = real.published.clone();
        let doc = Arc::new(doc);
        live.synced = doc.clone();
        if let Some(it) = self.interaction.as_mut() {
            // The gesture's edits so far, on top of the new state.
            let mut moved = live.client.view().clone();
            let gesture = diff(before, &self.doc).map(|d| d.changes).unwrap_or_default();
            let _ = moved.apply(&gesture, Mode::Lenient);
            let mut shown = moved.document().clone();
            shown.published = doc.published.clone();
            shown.reserve_ids(NodeId(self.doc.next_id().0.saturating_sub(1)));
            it.doc = doc;
            self.doc = Arc::new(shown);
        } else if let Some(p) = self.preview.as_mut() {
            p.doc = doc;
        } else {
            self.doc = doc;
        }
        let real = self.preview.as_ref().map_or(&self.doc, |p| &p.doc).clone();
        if !real.pages.contains(&self.page)
            && let Some(first) = real.first_page()
        {
            self.page = first;
        }
        if self.hover.is_some_and(|h| !self.doc.contains(h)) {
            self.hover = None;
        }
        self.clean_selection();
        self.revision += 1;
    }

    /// Undo (`back`) or redo the user's own last step in a live session. Returns whether it did
    /// anything; `None` outside a live session.
    pub(crate) fn live_travel(&mut self, back: bool) -> Option<bool> {
        let live = self.collab.as_mut()?;
        // Edits not synced yet go first (there are none unless something edited the document
        // outside a command).
        live.record(&self.doc, None);
        let (undo, redo) = live.client.history_len();
        let before = live.client.view().clone();
        let result = if back { live.client.undo(&EngineRecompute) } else { live.client.redo(&EngineRecompute) };
        match result {
            Ok(msgs) => live.outbox.extend(msgs),
            Err(e) => {
                log::warn!("live session: {} failed: {e}", if back { "undo" } else { "redo" });
                return Some(false);
            }
        }
        let after = live.client.history_len();
        let did = if back { after.1 > redo } else { after.0 > undo };
        if did {
            self.show_live(&before);
        }
        Some(did)
    }
}

impl Session {
    fn live(&mut self, uid: u64) -> Result<&mut DocState> {
        self.docs
            .iter_mut()
            .find(|d| d.uid == uid && d.collab.is_some())
            .ok_or_else(|| EngineError::Other("that document isn't in a live session".into()))
    }

    /// Put the active document into a live session through `client` (just joined: its document
    /// replaces the open one). From now on every edit is synced, and undo and redo take back only
    /// the user's own steps (the history starts empty). Returns the document's uid, which the
    /// other `collab_*` calls take.
    pub fn collab_attach(&mut self, mut client: Client) -> Result<u64> {
        crate::textpreview::end(self);
        let ds = self.ds_mut()?;
        ds.end_preview();
        if let Some(it) = ds.interaction.take() {
            ds.doc = it.doc;
        }
        ds.text_edit = None;
        ds.vector_edit = None;
        ds.history.undo.clear();
        ds.history.redo.clear();
        client.set_history_limit(ds.history.limit);
        let before = client.view().clone();
        ds.collab = Some(LiveDoc { synced: ds.doc.clone(), client, outbox: Vec::new(), hold: 0 });
        ds.show_live(&before);
        Ok(ds.uid)
    }

    /// Leave the live session of document `uid`: it keeps the document as it is and goes back to
    /// single-user undo (the history starts empty). Returns the session client.
    pub fn collab_detach(&mut self, uid: u64) -> Option<Client> {
        let ds = self.live(uid).ok()?;
        let live = ds.collab.take()?;
        ds.history.undo.clear();
        ds.history.redo.clear();
        ds.revision += 1;
        Some(live.client)
    }

    /// The live session client of document `uid`.
    pub fn collab_client(&self, uid: u64) -> Option<&Client> {
        self.docs.iter().find(|d| d.uid == uid).and_then(|d| d.collab.as_ref()).map(LiveDoc::client)
    }

    /// A message from the session server for document `uid`: others' edits show at once (under
    /// the user's own unconfirmed ones and any gesture in progress), and a refused edit of the
    /// user's is rolled back with a short reason in [`Session::message`] (the UI's toast). Errors
    /// mean the connection is unusable: reconnect ([`collab_reconnect`](Self::collab_reconnect)).
    pub fn collab_receive(&mut self, uid: u64, msg: ServerMsg) -> Result<()> {
        let ds = self.live(uid)?;
        // A live text preview shows the document as it was: end it first.
        if let Some(base) = ds.text_preview.take() {
            ds.doc = base;
        }
        let changes_doc = matches!(msg, ServerMsg::Changes(_) | ServerMsg::Reject { .. } | ServerMsg::Welcome { .. });
        let Some(live) = ds.collab.as_mut() else { return Err(EngineError::NoDocument) };
        if ds.interaction.is_none() && ds.preview.is_none() {
            live.record(&ds.doc, None);
        }
        let before = live.client.view().clone();
        let replies = live.client.receive(msg, &EngineRecompute).map_err(|e| EngineError::Other(e.to_string()));
        if let Ok(r) = &replies {
            live.outbox.extend(r.iter().cloned());
        }
        let notices: Vec<String> = live.client.take_notices().iter().map(figcraft_collab::Notice::message).collect();
        if changes_doc {
            ds.show_live(&before);
        }
        if !notices.is_empty() {
            self.message = Some(notices.join("; "));
        }
        replies.map(|_| ())
    }

    /// Messages for the session server from document `uid` (taken: send them in order).
    pub fn collab_outgoing(&mut self, uid: u64) -> Vec<ClientMsg> {
        self.live(uid).ok().and_then(|ds| ds.collab.as_mut()).map(|l| std::mem::take(&mut l.outbox)).unwrap_or_default()
    }

    /// The connection of document `uid` dropped: editing goes on, batches wait for the reconnect
    /// (what was waiting to go out stays queued in the client, so the outbox is cleared).
    pub fn collab_disconnected(&mut self, uid: u64) {
        if let Some(live) = self.live(uid).ok().and_then(|ds| ds.collab.as_mut()) {
            live.client.disconnect();
            live.outbox.clear();
        }
    }

    /// The first message of a new connection for document `uid` (`hello`: as for the first join);
    /// the server's welcome then goes to [`collab_receive`](Self::collab_receive).
    pub fn collab_reconnect(&mut self, uid: u64, hello: Hello) -> Option<ClientMsg> {
        let live = self.live(uid).ok()?.collab.as_mut()?;
        live.outbox.clear();
        Some(live.client.reconnect(hello))
    }
}
