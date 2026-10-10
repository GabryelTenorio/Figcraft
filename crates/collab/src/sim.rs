//! An in-memory session for tests: one [`Server`], several [`Client`]s and a link each way per
//! client that delivers in order (like a WebSocket) after a random delay, so messages on different
//! links interleave differently for every seed. Every message goes through the binary or the JSON
//! encoding on its way.

use std::collections::VecDeque;

use figcraft_doc::Document;

use crate::message::{self, ClientMsg, ServerMsg};
use crate::{Client, Error, Limits, Outgoing, Recompute, Replica, Server};

/// A small deterministic random number generator (SplitMix64) for simulations and fuzzing.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as usize }
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }

    /// A random element of `items`.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        items.get(self.below(items.len()))
    }
}

/// A message on the wire.
#[derive(Clone, Debug)]
enum Wire {
    Binary(Vec<u8>),
    Json(String),
}

/// One direction of one connection: in order, each message due at a time.
#[derive(Clone, Debug, Default)]
struct Link {
    queue: VecDeque<(u64, Wire)>,
    last_due: u64,
}

/// A simulated session.
pub struct Sim {
    pub server: Server,
    clients: Vec<Client>,
    up: Vec<Link>,
    down: Vec<Link>,
    rng: Rng,
    now: u64,
    max_delay: u64,
    limits: Limits,
    /// Messages delivered so far.
    pub delivered: usize,
    /// Batches the server rejected.
    pub rejected: usize,
    /// Follow-up batches clients sent to repair derived data.
    pub repairs: usize,
}

impl Sim {
    /// A session for `doc` with `n` clients joined; message delays are random in `1..=max_delay`.
    pub fn new(doc: Document, n: usize, seed: u64, max_delay: u64) -> Result<Sim, Error> {
        let limits = Limits::default();
        let mut sim = Sim {
            server: Server::new(doc, "secret", limits),
            clients: Vec::new(),
            up: Vec::new(),
            down: Vec::new(),
            rng: Rng::new(seed),
            now: 0,
            max_delay: max_delay.max(1),
            limits,
            delivered: 0,
            rejected: 0,
            repairs: 0,
        };
        for i in 0..n {
            let ClientMsg::Hello(hello) = Client::hello("file", "secret", &format!("Person {}", i + 1), "#3366FF") else {
                return Err(Error::Protocol("hello".into()));
            };
            let (id, out) = sim.server.join(hello).map_err(|m| Error::Protocol(format!("{m:?}")))?;
            let mut welcome = None;
            let mut rest = Vec::new();
            for o in out {
                match o {
                    Outgoing::To(to, m) if to == id => welcome = Some(m),
                    o => rest.push(o),
                }
            }
            let welcome = welcome.ok_or_else(|| Error::Protocol("no welcome".into()))?;
            // The welcome travels encoded like everything else.
            let welcome: ServerMsg = message::decode(&message::encode(&welcome)?, limits.max_message_bytes)?;
            sim.clients.push(Client::join(welcome)?);
            sim.up.push(Link::default());
            sim.down.push(Link::default());
            sim.route(rest)?;
        }
        Ok(sim)
    }

    /// The clients.
    pub fn clients(&self) -> &[Client] {
        &self.clients
    }

    /// Client `i`'s document (to edit a copy of).
    pub fn document(&self, i: usize) -> Option<&Document> {
        self.clients.get(i).map(Client::document)
    }

    /// The random number generator (drivers use it to pick edits).
    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.clients.iter().position(|c| c.id() == id)
    }

    fn wire<T: serde::Serialize>(&mut self, msg: &T) -> Result<Wire, Error> {
        // Mostly binary; now and then the JSON form, so both encodings are exercised.
        if self.rng.chance(0.15) { message::encode_json(msg).map(Wire::Json) } else { message::encode(msg).map(Wire::Binary) }
    }

    fn unwire<T: serde::de::DeserializeOwned>(&self, w: &Wire) -> Result<T, Error> {
        match w {
            Wire::Binary(b) => message::decode(b, self.limits.max_message_bytes),
            Wire::Json(t) => message::decode_json(t, self.limits.max_message_bytes),
        }
    }

    fn send(&mut self, up: bool, i: usize, w: Wire) {
        let delay = 1 + self.rng.next_u64() % self.max_delay;
        let now = self.now;
        let link = if up { self.up.get_mut(i) } else { self.down.get_mut(i) };
        if let Some(link) = link {
            let due = (now + delay).max(link.last_due);
            link.last_due = due;
            link.queue.push_back((due, w));
        }
    }

    fn route(&mut self, out: Vec<Outgoing>) -> Result<(), Error> {
        for o in out {
            let (msg, targets): (ServerMsg, Vec<usize>) = match o {
                Outgoing::To(id, m) => (m, self.index_of(id).into_iter().collect()),
                Outgoing::All(m) => (m, (0..self.clients.len()).collect()),
                Outgoing::Others(id, m) => (m, (0..self.clients.len()).filter(|i| self.clients.get(*i).is_some_and(|c| c.id() != id)).collect()),
            };
            for t in targets {
                let w = self.wire(&msg)?;
                self.send(false, t, w);
            }
        }
        Ok(())
    }

    /// Client `i` edited its document into `doc`: it applies it at once and sends the batch.
    /// Returns whether anything synced changed.
    pub fn edit(&mut self, i: usize, doc: &Document) -> Result<bool, Error> {
        let c = self.clients.get_mut(i).ok_or_else(|| Error::Invalid(format!("no client {i}")))?;
        match c.edit(doc)? {
            Some(msg) => {
                let w = self.wire(&msg)?;
                self.send(true, i, w);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Deliver the next due message (the earliest; ties go to the lowest link). False when none.
    pub fn step(&mut self, recompute: &dyn Recompute) -> Result<bool, Error> {
        let next = self
            .up
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.queue.front().map(|(due, _)| (*due, 0u8, i)))
            .chain(self.down.iter().enumerate().filter_map(|(i, l)| l.queue.front().map(|(due, _)| (*due, 1u8, i))))
            .min();
        let Some((due, dir, i)) = next else { return Ok(false) };
        self.now = self.now.max(due);
        self.delivered += 1;
        if dir == 0 {
            let Some((_, w)) = self.up.get_mut(i).and_then(|l| l.queue.pop_front()) else { return Ok(false) };
            let msg: ClientMsg = self.unwire(&w)?;
            let id = self.clients.get(i).map(Client::id).unwrap_or(0);
            let out = self.server.receive(id, msg);
            self.route(out)?;
        } else {
            let Some((_, w)) = self.down.get_mut(i).and_then(|l| l.queue.pop_front()) else { return Ok(false) };
            let msg: ServerMsg = self.unwire(&w)?;
            self.rejected += usize::from(matches!(msg, ServerMsg::Reject { .. }));
            let replies = match self.clients.get_mut(i) {
                Some(c) => c.receive(msg, recompute)?,
                None => Vec::new(),
            };
            self.repairs += replies.len();
            for r in replies {
                let w = self.wire(&r)?;
                self.send(true, i, w);
            }
        }
        Ok(true)
    }

    /// Deliver until no message is in flight (at most `max_steps`). Returns the steps taken.
    pub fn settle(&mut self, recompute: &dyn Recompute, max_steps: usize) -> Result<usize, Error> {
        let mut n = 0;
        while n < max_steps && self.step(recompute)? {
            n += 1;
        }
        if self.up.iter().chain(&self.down).any(|l| !l.queue.is_empty()) {
            return Err(Error::Protocol(format!("still busy after {max_steps} steps")));
        }
        Ok(n)
    }

    /// Whether every client caught up with the server and holds exactly its state; the first
    /// difference otherwise.
    pub fn converged(&self) -> Result<(), String> {
        let server: &Replica = self.server.replica();
        for c in &self.clients {
            if c.pending() > 0 || c.seq() != self.server.seq() {
                return Err(format!("client {} at seq {} with {} pending; server at {}", c.id(), c.seq(), c.pending(), self.server.seq()));
            }
            if c.view() != server || c.confirmed() != server {
                return Err(format!("client {}: {}", c.id(), describe(server, c.view())));
            }
        }
        Ok(())
    }
}

/// The first difference between two replicas, for test failures.
pub fn describe(a: &Replica, b: &Replica) -> String {
    let (da, db) = (a.document(), b.document());
    if let Some(&id) = db.changed_ids(da, usize::MAX).unwrap_or_default().first() {
        return match (da.node(id), db.node(id)) {
            (Some(x), Some(y)) => format!("node {id} differs:\n  {x:?}\n  {y:?}"),
            (x, y) => format!("node {id}: {} vs {}", x.is_some(), y.is_some()),
        };
    }
    if da.pages != db.pages {
        return format!("pages {:?} vs {:?}", da.pages, db.pages);
    }
    if a.positions() != b.positions() {
        let k = a.positions().iter().find(|(k, v)| b.positions().get(*k) != Some(*v)).map(|(k, _)| *k);
        return format!("positions differ (first at {k:?})");
    }
    if da != db {
        return "document-level data differs".into();
    }
    "equal".into()
}
