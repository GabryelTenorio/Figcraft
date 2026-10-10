//! The session server for one file: the single authority that orders, validates and rebroadcasts
//! batches. A pure state machine: the transport feeds it messages and delivers what it returns.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use figcraft_doc::{Document, NodeId};
use serde::{Deserialize, Serialize};

use crate::change::{Change, Prop, Target};
use crate::message::{Accepted, CatchUp, ClientMsg, Hello, PROTOCOL, Peer, Presence, Processed, ServerMsg};
use crate::replica::{Mode, Replica, create_change};
use crate::{Error, Limits, docprops};

/// A message for the transport to deliver.
#[derive(Clone, Debug, PartialEq)]
pub enum Outgoing {
    /// To one client.
    To(u32, ServerMsg),
    /// To every client.
    All(ServerMsg),
    /// To every client but one.
    Others(u32, ServerMsg),
}

/// A session server for one file.
#[derive(Clone, Debug)]
pub struct Server {
    /// This session's identity (random): clients resume only into the session they were in.
    session: u64,
    state: Replica,
    seq: u64,
    token: String,
    limits: Limits,
    next_client: u32,
    peers: BTreeMap<u32, Peer>,
    presence: BTreeMap<u32, Presence>,
    acked: BTreeMap<u32, u64>,
    /// Deleted node ids: only these may be created again (by undo); new ids must come from the
    /// creating client's id space.
    deleted: BTreeSet<NodeId>,
    /// Every client id handed out (a reconnecting client keeps its id) and what was done with
    /// its batches.
    issued: BTreeMap<u32, Processed>,
    /// The latest accepted batches, oldest first (a reconnecting client gets what it missed).
    log: VecDeque<Accepted>,
    /// Changes in `log`.
    log_changes: usize,
}

/// The most client ids a session remembers for reconnecting (the oldest disconnected ones go
/// first; a forgotten client joins as new).
const MAX_REMEMBERED: usize = 65_536;

/// What a session server needs to carry on after a restart: the document, the sequence number,
/// the client-id counter (so ids are never handed out twice), the clients that can reconnect,
/// the deleted ids that may be created again and the log of recent batches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub session: u64,
    pub snapshot: Vec<Change>,
    pub seq: u64,
    pub next_client: u32,
    pub issued: BTreeMap<u32, Processed>,
    pub deleted: BTreeSet<NodeId>,
    pub log: Vec<Accepted>,
}

impl Server {
    /// A session for `doc`. Joining requires `token` (empty: no token). Client ids start above every
    /// client id space already used in the document, so ids from earlier sessions are never handed
    /// out again.
    pub fn new(doc: Document, token: impl Into<String>, limits: Limits) -> Server {
        let used = doc.ids().map(|id| id.0 >> 32).max().unwrap_or(0);
        let next_client = u32::try_from(used.saturating_add(1)).unwrap_or(u32::MAX).max(1);
        Server {
            session: new_session_id(),
            state: Replica::new(doc),
            seq: 0,
            token: token.into(),
            limits,
            next_client,
            peers: BTreeMap::new(),
            presence: BTreeMap::new(),
            acked: BTreeMap::new(),
            deleted: BTreeSet::new(),
            issued: BTreeMap::new(),
            log: VecDeque::new(),
            log_changes: 0,
        }
    }

    /// The session's state, to save (see [`restore`](Self::restore)).
    pub fn state(&self) -> Result<SessionState, Error> {
        Ok(SessionState {
            session: self.session,
            snapshot: self.state.snapshot()?,
            seq: self.seq,
            next_client: self.next_client,
            issued: self.issued.clone(),
            deleted: self.deleted.clone(),
            log: self.log.iter().cloned().collect(),
        })
    }

    /// A session carrying on from a saved [`state`](Self::state) (nobody is connected yet; clients
    /// reconnect with their ids). A log that doesn't lead up to the sequence number is dropped
    /// (clients then catch up with a snapshot).
    pub fn restore(state: SessionState, token: impl Into<String>, limits: Limits) -> Result<Server, Error> {
        let replica = Replica::from_snapshot(&state.snapshot)?;
        let mut server = Server::new(replica.document().clone(), token, limits);
        server.state = replica;
        server.session = state.session;
        server.seq = state.seq;
        server.next_client = server.next_client.max(state.next_client);
        server.issued = state.issued.into_iter().filter(|(id, _)| *id < server.next_client).collect();
        server.deleted = state.deleted;
        let contiguous = state.log.windows(2).all(|w| matches!(w, [a, b] if a.seq + 1 == b.seq));
        if contiguous && state.log.last().is_none_or(|a| a.seq == state.seq) {
            for a in state.log {
                server.remember(a);
            }
        }
        Ok(server)
    }

    /// The authoritative document.
    pub fn document(&self) -> &Document {
        self.state.document()
    }

    /// The authoritative state (document and positions).
    pub fn replica(&self) -> &Replica {
        &self.state
    }

    /// The sequence number of the last accepted batch (0: none yet).
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Who is in the session.
    pub fn peers(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    /// The highest sequence number client `id` reported applying.
    pub fn acked(&self, id: u32) -> Option<u64> {
        self.acked.get(&id).copied()
    }

    /// A new connection says hello: on success its client id and the messages to send (its
    /// `Welcome`, everyone else's `PeerJoined`); otherwise the `Refused` to send back. A client
    /// coming back ([`Hello::resume`]) keeps its id (taking over from its old connection if that
    /// is still open: the transport closes the old one) and gets the batches it missed, or a
    /// snapshot when they are no longer in the log; an id this session never gave out joins as
    /// new.
    pub fn join(&mut self, hello: Hello) -> Result<(u32, Vec<Outgoing>), ServerMsg> {
        let refuse = |reason: &str| ServerMsg::Refused { reason: reason.to_string() };
        if hello.protocol != PROTOCOL {
            return Err(refuse(&format!("protocol {} isn't supported (this server speaks {PROTOCOL})", hello.protocol)));
        }
        if !same_secret(&hello.token, &self.token) {
            return Err(refuse("wrong session token"));
        }
        let resume = hello.resume.filter(|r| r.session == self.session && self.issued.contains_key(&r.client_id));
        if self.peers.len() >= self.limits.max_peers && resume.is_none_or(|r| !self.peers.contains_key(&r.client_id)) {
            return Err(refuse("the session is full"));
        }
        if hello.name.chars().count() > self.limits.max_name || hello.color.chars().count() > self.limits.max_name {
            return Err(refuse("name or colour too long"));
        }
        if resume.is_none() && self.next_client == u32::MAX {
            return Err(refuse("no client ids left in this session"));
        }
        // The log holds every batch after `floor`.
        let floor = self.log.front().map_or(self.seq, |a| a.seq.saturating_sub(1));
        let catch_up = match resume {
            Some(r) if r.seq <= self.seq && r.seq >= floor => CatchUp::Delta(self.log.iter().filter(|a| a.seq > r.seq).cloned().collect()),
            _ => CatchUp::Snapshot(self.state.snapshot().map_err(|e| refuse(&e.to_string()))?),
        };
        let id = match resume {
            Some(r) => r.client_id,
            None => {
                let id = self.next_client;
                self.next_client += 1;
                self.issued.insert(id, Processed::default());
                if self.issued.len() > MAX_REMEMBERED
                    && let Some(old) = self.issued.keys().copied().find(|k| !self.peers.contains_key(k))
                {
                    self.issued.remove(&old);
                }
                id
            }
        };
        let processed = self.issued.get(&id).cloned().unwrap_or_default();
        self.presence.remove(&id);
        let peer = Peer { client_id: id, name: hello.name, color: hello.color };
        let others = self.peers.values().filter(|p| p.client_id != id).cloned().collect();
        let welcome = ServerMsg::Welcome { session: self.session, client_id: id, seq: self.seq, catch_up, processed, peers: others };
        self.peers.insert(id, peer.clone());
        Ok((id, vec![Outgoing::To(id, welcome), Outgoing::Others(id, ServerMsg::PeerJoined(peer))]))
    }

    /// Client `id`'s connection closed.
    pub fn leave(&mut self, id: u32) -> Vec<Outgoing> {
        self.presence.remove(&id);
        self.acked.remove(&id);
        match self.peers.remove(&id) {
            Some(_) => vec![Outgoing::Others(id, ServerMsg::PeerLeft { client_id: id })],
            None => Vec::new(),
        }
    }

    /// A message from client `from` (which has joined).
    pub fn receive(&mut self, from: u32, msg: ClientMsg) -> Vec<Outgoing> {
        if !self.peers.contains_key(&from) {
            return vec![Outgoing::To(from, ServerMsg::Refused { reason: "say hello first".into() })];
        }
        match msg {
            ClientMsg::Hello(_) => vec![Outgoing::To(from, ServerMsg::Refused { reason: "already joined".into() })],
            ClientMsg::Changes { base_seq, batch_id, changes } => {
                let result = self.accept(from, base_seq, &changes);
                if let Some(p) = self.issued.get_mut(&from) {
                    p.note(batch_id, result.is_err());
                }
                match result {
                    Ok(()) => {
                        let acc = Accepted { seq: self.seq, author: from, batch_id, changes };
                        self.remember(acc.clone());
                        vec![Outgoing::All(ServerMsg::Changes(acc))]
                    }
                    Err(e) => {
                        let fixes = self.fixes(&changes).unwrap_or_default();
                        vec![Outgoing::To(from, ServerMsg::Reject { batch_id, reason: reason(&e), fixes })]
                    }
                }
            }
            ClientMsg::Presence(p) => {
                self.presence.insert(from, p.clone());
                vec![Outgoing::Others(from, ServerMsg::Presence { client_id: from, presence: p })]
            }
            ClientMsg::Ack { seq } => {
                self.acked.insert(from, seq.min(self.seq));
                Vec::new()
            }
            ClientMsg::Ping => vec![Outgoing::To(from, ServerMsg::Pong)],
        }
    }

    /// Keep an accepted batch in the log (dropping the oldest beyond the limits).
    fn remember(&mut self, acc: Accepted) {
        self.log_changes += acc.changes.len();
        self.log.push_back(acc);
        while self.log.len() > self.limits.max_log_batches || (self.log_changes > self.limits.max_log_changes && !self.log.is_empty()) {
            if let Some(old) = self.log.pop_front() {
                self.log_changes = self.log_changes.saturating_sub(old.changes.len());
            }
        }
    }

    /// Validate and apply a batch; on success it has the next sequence number.
    fn accept(&mut self, from: u32, base_seq: u64, changes: &[Change]) -> Result<(), Error> {
        if changes.len() > self.limits.max_changes {
            return Err(Error::Invalid(format!("{} changes in one batch (at most {})", changes.len(), self.limits.max_changes)));
        }
        if base_seq > self.seq {
            return Err(Error::Protocol(format!("based on sequence {base_seq}, but the server is at {}", self.seq)));
        }
        for c in changes {
            if let Change::Create { id, .. } = c
                && !self.state.document().contains(*id)
                && !self.deleted.contains(id)
                && id.0 >> 32 != u64::from(from)
            {
                return Err(Error::Invalid(format!("new node {id} isn't in client {from}'s id space")));
            }
        }
        let mut next = self.state.clone();
        let applied = next.apply(changes, Mode::Strict)?;
        if next.document().node_count() > self.limits.max_nodes {
            return Err(Error::Invalid(format!("more than {} layers", self.limits.max_nodes)));
        }
        for c in changes {
            if let Change::Create { id, .. } = c {
                self.deleted.remove(id);
            }
        }
        self.deleted.extend(applied.deleted);
        self.state = next;
        self.seq += 1;
        Ok(())
    }

    /// The current values of what a refused batch touched, so a client can roll its optimistic
    /// copy back: nodes that don't exist are deleted, deleted subtrees come back whole, edited
    /// properties get their current values, document-level data likewise.
    fn fixes(&self, changes: &[Change]) -> Result<Vec<Change>, Error> {
        let doc = self.state.document();
        let mut props: BTreeMap<NodeId, BTreeSet<Prop>> = BTreeMap::new();
        let mut gone: BTreeSet<NodeId> = BTreeSet::new();
        let mut out = Vec::new();
        for c in changes {
            match c {
                Change::Create { id, .. } => {
                    props.entry(*id).or_default().extend(Prop::FIELDS.iter().copied().chain([Prop::Parent, Prop::Position]));
                }
                Change::Set { id, prop, .. } => {
                    props.entry(*id).or_default().insert(*prop);
                }
                Change::Delete { id } => {
                    gone.insert(*id);
                }
                other => {
                    if let Some(fix) = docprops::current(doc, other)?
                        && !out.contains(&fix)
                    {
                        out.push(fix);
                    }
                }
            }
        }
        let mut nodes = Vec::new();
        for (id, ps) in props {
            if !doc.contains(id) {
                nodes.push(Change::Delete { id });
                continue;
            }
            for p in ps {
                let t = Target::Node(id, p);
                nodes.extend(t.write(self.state.value(&t)?.as_ref()));
            }
        }
        for id in gone {
            match doc.node(id) {
                Some(_) => {
                    for d in doc.subtree(id) {
                        if let Some(n) = doc.node(d) {
                            nodes.push(create_change(n, self.state.key(d))?);
                        }
                    }
                }
                None => nodes.push(Change::Delete { id }),
            }
        }
        nodes.extend(out);
        Ok(nodes)
    }
}

/// A fresh session identity: random per server (std's hasher keys are seeded from the OS).
fn new_session_id() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u8(0);
    h.finish()
}

/// Why a batch was refused, for the person who made it.
fn reason(e: &Error) -> String {
    match e {
        Error::Unknown(_) => "a layer it changes was deleted by someone else".into(),
        e => e.to_string(),
    }
}

/// Compare secrets without stopping at the first difference.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
