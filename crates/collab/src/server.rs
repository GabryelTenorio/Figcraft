//! The session server for one file: the single authority that orders, validates and rebroadcasts
//! batches. A pure state machine: the transport feeds it messages and delivers what it returns.

use std::collections::{BTreeMap, BTreeSet};

use figcraft_doc::{Document, NodeId};

use crate::change::{Change, Prop};
use crate::message::{ClientMsg, Hello, PROTOCOL, Peer, Presence, ServerMsg};
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
}

impl Server {
    /// A session for `doc`. Joining requires `token` (empty: no token). Client ids start above every
    /// client id space already used in the document, so ids from earlier sessions are never handed
    /// out again.
    pub fn new(doc: Document, token: impl Into<String>, limits: Limits) -> Server {
        let used = doc.ids().map(|id| id.0 >> 32).max().unwrap_or(0);
        let next_client = u32::try_from(used.saturating_add(1)).unwrap_or(u32::MAX).max(1);
        Server {
            state: Replica::new(doc),
            seq: 0,
            token: token.into(),
            limits,
            next_client,
            peers: BTreeMap::new(),
            presence: BTreeMap::new(),
            acked: BTreeMap::new(),
            deleted: BTreeSet::new(),
        }
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
    /// `Welcome`, everyone else's `PeerJoined`); otherwise the `Refused` to send back.
    pub fn join(&mut self, hello: Hello) -> Result<(u32, Vec<Outgoing>), ServerMsg> {
        let refuse = |reason: &str| ServerMsg::Refused { reason: reason.to_string() };
        if hello.protocol != PROTOCOL {
            return Err(refuse(&format!("protocol {} isn't supported (this server speaks {PROTOCOL})", hello.protocol)));
        }
        if !same_secret(&hello.token, &self.token) {
            return Err(refuse("wrong session token"));
        }
        if self.peers.len() >= self.limits.max_peers {
            return Err(refuse("the session is full"));
        }
        if hello.name.chars().count() > self.limits.max_name || hello.color.chars().count() > self.limits.max_name {
            return Err(refuse("name or colour too long"));
        }
        if self.next_client == u32::MAX {
            return Err(refuse("no client ids left in this session"));
        }
        let snapshot = self.state.snapshot().map_err(|e| refuse(&e.to_string()))?;
        let id = self.next_client;
        self.next_client += 1;
        let peer = Peer { client_id: id, name: hello.name, color: hello.color };
        let welcome = ServerMsg::Welcome { client_id: id, seq: self.seq, snapshot, peers: self.peers.values().cloned().collect() };
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
            ClientMsg::Changes { base_seq, batch_id, changes } => match self.accept(from, base_seq, &changes) {
                Ok(()) => vec![Outgoing::All(ServerMsg::Changes { seq: self.seq, author: from, batch_id, changes })],
                Err(e) => {
                    let fixes = self.fixes(&changes).unwrap_or_default();
                    vec![Outgoing::To(from, ServerMsg::Reject { batch_id, reason: e.to_string(), fixes })]
                }
            },
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
            let Some(n) = doc.node(id) else {
                nodes.push(Change::Delete { id });
                continue;
            };
            for p in ps {
                let value = match p {
                    Prop::Parent => crate::Json::of(&n.parent)?,
                    Prop::Position => crate::Json::of(self.state.key(id))?,
                    p => p.get(n)?,
                };
                nodes.push(Change::Set { id, prop: p, value });
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

/// Compare secrets without stopping at the first difference.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
