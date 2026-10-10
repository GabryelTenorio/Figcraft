//! The client side: the confirmed server state, this client's unconfirmed batches on top of it,
//! and the optimistic view the user edits.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use figcraft_doc::{Document, NodeId};

use crate::change::{Change, Prop};
use crate::message::{ClientMsg, Hello, PROTOCOL, Peer, Presence, ServerMsg};
use crate::replica::{Mode, Replica};
use crate::{Error, Recompute, diff};

/// A batch sent and not yet confirmed.
#[derive(Clone, Debug)]
struct Pending {
    batch_id: u64,
    changes: Vec<Change>,
    /// Someone else's batch was ordered before this one: its derived data may be stale.
    rebased: bool,
}

/// One participant's sync state.
#[derive(Clone, Debug)]
pub struct Client {
    id: u32,
    /// The server's state as of `seq`.
    confirmed: Replica,
    seq: u64,
    pending: VecDeque<Pending>,
    /// `confirmed` with the pending batches applied: what the user sees and edits.
    view: Replica,
    next_batch: u64,
    /// Ids below this were handed out; rebuilding the view never hands them out again.
    next_id: NodeId,
    peers: BTreeMap<u32, Peer>,
    presence: BTreeMap<u32, Presence>,
}

impl Client {
    /// The first message of a connection.
    pub fn hello(file: &str, token: &str, name: &str, color: &str) -> ClientMsg {
        ClientMsg::Hello(Hello {
            protocol: PROTOCOL,
            file: file.to_string(),
            token: token.to_string(),
            name: name.to_string(),
            color: color.to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// Start from the server's `Welcome`.
    pub fn join(welcome: ServerMsg) -> Result<Client, Error> {
        let ServerMsg::Welcome { client_id, seq, snapshot, peers } = welcome else {
            return Err(Error::Protocol("expected a welcome".into()));
        };
        let mut confirmed = Replica::from_snapshot(&snapshot)?;
        confirmed.document_mut().set_client_id(client_id);
        let next_id = confirmed.document().next_id();
        Ok(Client {
            id: client_id,
            view: confirmed.clone(),
            confirmed,
            seq,
            pending: VecDeque::new(),
            next_batch: 1,
            next_id,
            peers: peers.into_iter().map(|p| (p.client_id, p)).collect(),
            presence: BTreeMap::new(),
        })
    }

    /// This client's id (its node ids are `id << 32 | counter`).
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The document as this user sees it (server state plus their unconfirmed edits). Edit a copy
    /// (it allocates ids in this client's space) and hand it to [`edit`](Self::edit).
    pub fn document(&self) -> &Document {
        self.view.document()
    }

    /// The optimistic state (document and positions).
    pub fn view(&self) -> &Replica {
        &self.view
    }

    /// The last confirmed server state.
    pub fn confirmed(&self) -> &Replica {
        &self.confirmed
    }

    /// The sequence number of the last server batch applied.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Batches sent but not yet confirmed or rejected.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// The other people in the session.
    pub fn peers(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    /// The last presence each peer sent.
    pub fn presence_of(&self, id: u32) -> Option<&Presence> {
        self.presence.get(&id)
    }

    /// The user edited the document (`doc` is [`document`](Self::document) after a command): show
    /// it at once and return the batch to send, if anything synced changed.
    pub fn edit(&mut self, doc: &Document) -> Result<Option<ClientMsg>, Error> {
        let d = diff(&self.view, doc)?;
        if d.changes.is_empty() {
            return Ok(None);
        }
        self.view = d.replica;
        self.keep_ids();
        Ok(Some(self.queue(d.changes)))
    }

    /// Queue `changes` as a new batch (already shown in the view) and make its message.
    fn queue(&mut self, changes: Vec<Change>) -> ClientMsg {
        let batch_id = self.next_batch;
        self.next_batch += 1;
        self.pending.push_back(Pending { batch_id, changes: changes.clone(), rebased: false });
        ClientMsg::Changes { base_seq: self.seq, batch_id, changes }
    }

    /// Keep allocating in this client's space, after every id already handed out.
    fn keep_ids(&mut self) {
        let doc = self.view.document_mut();
        if doc.client_id() != self.id {
            doc.set_client_id(self.id);
        }
        if doc.next_id() < self.next_id {
            doc.reserve_ids(NodeId(self.next_id.0.saturating_sub(1)));
        }
        self.next_id = doc.next_id();
    }

    /// Handle a message from the server. Returns messages to send: a follow-up batch when this
    /// client's confirmed batch was ordered after someone else's and `recompute` changes derived
    /// data over it. Errors mean the connection is unusable (rejoin).
    pub fn receive(&mut self, msg: ServerMsg, recompute: &dyn Recompute) -> Result<Vec<ClientMsg>, Error> {
        match msg {
            ServerMsg::Changes { seq, author, batch_id, changes } => {
                if seq != self.seq + 1 {
                    return Err(Error::Protocol(format!("sequence {seq} after {}", self.seq)));
                }
                if author == self.id {
                    let mine = self.pending.pop_front().ok_or_else(|| Error::Protocol("confirmation of an unknown batch".into()))?;
                    if mine.batch_id != batch_id {
                        return Err(Error::Protocol(format!("batch {batch_id} confirmed, expected {}", mine.batch_id)));
                    }
                    let before = self.confirmed.clone();
                    let applied = self.confirmed.apply(&changes, Mode::Strict)?;
                    self.seq = seq;
                    if mine.rebased {
                        return self.repair(&before, &applied.touched, recompute);
                    }
                    Ok(Vec::new())
                } else {
                    self.confirmed.apply(&changes, Mode::Strict)?;
                    self.seq = seq;
                    for p in &mut self.pending {
                        p.rebased = true;
                    }
                    self.rebuild();
                    Ok(Vec::new())
                }
            }
            ServerMsg::Reject { batch_id, .. } => {
                // The confirmed state is what the server has: drop the batch and replay the rest
                // (the fixes are for clients that keep no confirmed copy).
                let mine = self.pending.pop_front().ok_or_else(|| Error::Protocol("rejection of an unknown batch".into()))?;
                if mine.batch_id != batch_id {
                    return Err(Error::Protocol(format!("batch {batch_id} rejected, expected {}", mine.batch_id)));
                }
                self.rebuild();
                Ok(Vec::new())
            }
            ServerMsg::Presence { client_id, presence } => {
                self.presence.insert(client_id, presence);
                Ok(Vec::new())
            }
            ServerMsg::PeerJoined(p) => {
                self.peers.insert(p.client_id, p);
                Ok(Vec::new())
            }
            ServerMsg::PeerLeft { client_id } => {
                self.peers.remove(&client_id);
                self.presence.remove(&client_id);
                Ok(Vec::new())
            }
            ServerMsg::Pong => Ok(Vec::new()),
            ServerMsg::Welcome { .. } => Err(Error::Protocol("already joined".into())),
            ServerMsg::Refused { reason } => Err(Error::Protocol(reason)),
        }
    }

    /// The view is the confirmed state with the pending batches replayed on top (changes that no
    /// longer apply are skipped; the server will reject their batch).
    fn rebuild(&mut self) {
        let mut v = self.confirmed.clone();
        for p in &self.pending {
            // Lenient application never fails.
            let _ = v.apply(&p.changes, Mode::Lenient);
        }
        self.view = v;
        self.keep_ids();
    }

    /// This client's batch landed after someone else's: recompute derived data over what it
    /// touched and send what changes (minus properties a later pending batch writes anyway).
    fn repair(&mut self, before: &Replica, touched: &BTreeSet<NodeId>, recompute: &dyn Recompute) -> Result<Vec<ClientMsg>, Error> {
        let mut work = self.confirmed.document().clone();
        // Ids the recompute makes must not collide with those of later pending batches.
        work.reserve_ids(NodeId(self.next_id.0.saturating_sub(1)));
        recompute.recompute(&mut work, before.document(), touched);
        let fix = diff(&self.confirmed, &work)?;
        let later: BTreeSet<(NodeId, Prop)> = self
            .pending
            .iter()
            .flat_map(|p| &p.changes)
            .filter_map(|c| match c {
                Change::Set { id, prop, .. } => Some((*id, *prop)),
                _ => None,
            })
            .collect();
        let changes: Vec<Change> =
            fix.changes.into_iter().filter(|c| !matches!(c, Change::Set { id, prop, .. } if later.contains(&(*id, *prop)))).collect();
        if changes.is_empty() {
            return Ok(Vec::new());
        }
        let _ = self.view.apply(&changes, Mode::Lenient);
        self.keep_ids();
        Ok(vec![self.queue(changes)])
    }
}
