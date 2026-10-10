//! The session host the app embeds: a WebSocket server for one file that runs the collaboration
//! core's [`Server`] (the ordering authority) and carries its messages.
//!
//! Threads: one accepts connections; each connection has a reader and a writer (see [`crate::net`]);
//! one hub owns the [`Server`] and everything shared, fed by a channel. The person hosting takes
//! part through the hub directly (no socket): their presence goes in with [`Host::presence`],
//! what others do comes out through their [`Inbox`].
//!
//! On top of the core the hub checks the invite's file id, hands out colours and default names,
//! throttles presence to about 30 updates a second per person (latest wins), tells a newcomer
//! its own name and colour (a `PeerJoined` of itself, after its welcome) and where everyone
//! already is, and disconnects clients that send too much, too fast, or can't keep up.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use figcraft_collab::message::{self, ClientMsg, Hello, PROTOCOL, Peer, Presence, ServerMsg};
use figcraft_collab::{Outgoing, Server};
use figcraft_doc::Document;
use figcraft_engine::live::{Event, MAX_NAME, PEOPLE, sanitize};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, Role, WebSocket};
use tungstenite::{Bytes, Message};

use crate::inbox::Inbox;
pub use crate::join::ENDED;
use crate::net::{Bucket, Duplex, NetLimits, config, shutdown};
use crate::throttle::{PRESENCE_INTERVAL, Throttle, earliest};

/// How long a closing connection waits for the peer to hang up.
const LINGER: Duration = Duration::from_secs(2);

/// What a hosted session needs.
pub struct HostConfig {
    /// The address to listen on (port 0: any free port).
    pub addr: SocketAddr,
    /// When `addr`'s port is taken, take any free one instead.
    pub any_port_if_taken: bool,
    /// The file's session id and secret (both in the invite).
    pub file: String,
    pub token: String,
    /// The shared document (the session's authoritative copy starts from it).
    pub doc: Document,
    /// The host's own name (empty: "Host").
    pub name: String,
    pub limits: NetLimits,
    pub core: figcraft_collab::Limits,
}

/// A running session host. Dropping it stops it.
pub struct Host {
    addr: SocketAddr,
    me: Peer,
    hub: Sender<HubIn>,
    stop: Arc<AtomicBool>,
    open: Arc<AtomicUsize>,
    hub_thread: Option<JoinHandle<()>>,
    accept_thread: Option<JoinHandle<()>>,
}

/// A message for one connection's writer.
enum Out {
    Bytes(Bytes),
    Close(CloseCode, String),
}

enum HubIn {
    Join {
        conn: u64,
        hello: Hello,
        out: SyncSender<Out>,
        sock: TcpStream,
    },
    Msg {
        conn: u64,
        msg: ClientMsg,
    },
    Gone {
        conn: u64,
    },
    Local(Presence),
    LocalMsg(ClientMsg),
    /// Cut every connection without a word (tests: a network failure).
    #[cfg(test)]
    Cut,
    Stop,
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name(name.to_string()).spawn(f)
}

/// A name others may see: trimmed, no control characters, at most [`MAX_NAME`] characters.
fn clean_name(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME).collect()
}

impl Host {
    /// Listen and start the hub; the host joins its own session as the first person.
    pub fn start(cfg: HostConfig, inbox: Inbox) -> io::Result<Host> {
        let listener = match TcpListener::bind(cfg.addr) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::AddrInUse && cfg.any_port_if_taken => TcpListener::bind(SocketAddr::new(cfg.addr.ip(), 0))?,
            Err(e) => return Err(e),
        };
        let addr = listener.local_addr()?;
        let mut server = Server::new(cfg.doc, cfg.token.clone(), cfg.core);
        let name = match clean_name(&cfg.name) {
            n if n.is_empty() => "Host".to_string(),
            n => n,
        };
        let hello = Hello {
            protocol: PROTOCOL,
            file: cfg.file.clone(),
            token: cfg.token,
            name,
            color: PEOPLE[0].to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let (me, outs) = server.join(hello).map_err(|r| io::Error::other(format!("{r:?}")))?;
        // The host's window syncs through its own client too: its welcome carries the document.
        for o in outs {
            if let Outgoing::To(id, w @ ServerMsg::Welcome { .. }) = o
                && id == me
            {
                inbox.push(Event::Sync(w));
            }
        }
        let me = server.peers().find(|p| p.client_id == me).cloned().ok_or_else(|| io::Error::other("the host didn't join its own session"))?;
        let (hub_tx, hub_rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let open = Arc::new(AtomicUsize::new(0));
        let hub = Hub {
            server,
            file: cfg.file,
            local: me.client_id,
            inbox,
            conns: HashMap::new(),
            ids: BTreeMap::new(),
            throttles: BTreeMap::new(),
            latest: BTreeMap::new(),
            guests: 0,
        };
        let hub_thread = spawn("live-hub", move || hub.run(&hub_rx))?;
        let accept_thread = {
            let (hub_tx, stop, open, limits) = (hub_tx.clone(), stop.clone(), open.clone(), cfg.limits);
            spawn("live-accept", move || accept(&listener, &hub_tx, &stop, &open, limits))
        };
        let accept_thread = match accept_thread {
            Ok(t) => t,
            Err(e) => {
                let _ = hub_tx.send(HubIn::Stop);
                let _ = hub_thread.join();
                return Err(e);
            }
        };
        log::info!("live session: hosting on {addr}");
        Ok(Host { addr, me, hub: hub_tx, stop, open, hub_thread: Some(hub_thread), accept_thread: Some(accept_thread) })
    }

    /// Where it listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The host as the others see them.
    pub fn me(&self) -> &Peer {
        &self.me
    }

    /// Open connections (joined or not).
    pub fn connections(&self) -> usize {
        self.open.load(Ordering::Relaxed)
    }

    /// The host's own presence.
    pub fn presence(&self, p: Presence) {
        let _ = self.hub.send(HubIn::Local(p));
    }

    /// A message from the host's own sync client (its edits).
    pub fn send(&self, msg: ClientMsg) {
        let _ = self.hub.send(HubIn::LocalMsg(msg));
    }

    /// Cut every connection as a network failure would (no close frame).
    #[cfg(test)]
    pub(crate) fn cut_connections(&self) {
        let _ = self.hub.send(HubIn::Cut);
    }

    /// End the session: everyone is told and disconnected, the port is released.
    pub fn stop(&mut self) {
        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.hub.send(HubIn::Stop);
        if let Some(t) = self.hub_thread.take() {
            let _ = t.join();
        }
        // The accept loop checks the flag after each connection: wake it with one.
        let wake = if self.addr.ip().is_unspecified() { SocketAddr::from(([127, 0, 0, 1], self.addr.port())) } else { self.addr };
        let woke = TcpStream::connect_timeout(&wake, Duration::from_secs(1)).is_ok();
        if let Some(t) = self.accept_thread.take() {
            if woke {
                let _ = t.join();
            } else {
                log::warn!("live session: couldn't wake the accept loop; it ends with the next connection");
            }
        }
        // Readers end once their writers have closed the sockets.
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.open.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        log::info!("live session: stopped hosting");
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Counts an open connection until dropped.
struct Open(Arc<AtomicUsize>);

impl Drop for Open {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn accept(listener: &TcpListener, hub: &Sender<HubIn>, stop: &AtomicBool, open: &Arc<AtomicUsize>, limits: NetLimits) {
    let mut next = 0u64;
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                // Out of sockets or similar: don't spin.
                log::warn!("live session: accept failed: {e}");
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        if open.load(Ordering::Relaxed) >= limits.max_connections {
            log::info!("live session: too many connections; refusing one");
            continue;
        }
        open.fetch_add(1, Ordering::Relaxed);
        let guard = Open(open.clone());
        next += 1;
        let (conn, hub) = (next, hub.clone());
        if let Err(e) = spawn("live-conn", move || serve(stream, conn, &hub, limits, guard)) {
            log::warn!("live session: can't start a connection thread: {e}");
        }
    }
}

/// A close reason that fits a WebSocket close frame (123 bytes).
fn reason(s: &str) -> String {
    let mut end = s.len().min(123);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default().to_string()
}

fn encode(msg: &ServerMsg) -> Option<Bytes> {
    match message::encode(msg) {
        Ok(b) => Some(Bytes::from(b)),
        Err(e) => {
            log::warn!("live session: can't encode a message: {e}");
            None
        }
    }
}

/// Queue `Refused {reason}` and a close for one connection.
fn refuse(out: &SyncSender<Out>, why: &str) {
    if let Some(b) = encode(&ServerMsg::Refused { reason: why.to_string() }) {
        let _ = out.try_send(Out::Bytes(b));
    }
    let _ = out.try_send(Out::Close(CloseCode::Policy, reason(why)));
}

/// Read the next binary message (skipping pings), or why the connection should end.
fn next_message(ws: &mut WebSocket<Duplex>, max: usize) -> Result<Option<ClientMsg>, &'static str> {
    loop {
        return match ws.read() {
            Ok(Message::Binary(b)) => message::decode::<ClientMsg>(&b, max).map(Some).map_err(|_| "that message couldn't be read"),
            Ok(Message::Text(_)) => Err("only binary messages are spoken here"),
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
            Ok(Message::Close(_)) => Ok(None),
            Err(tungstenite::Error::Capacity(_)) => Err("message too large"),
            Err(_) => Ok(None),
        };
    }
}

/// One connection's reader (the writer runs on its own thread).
fn serve(stream: TcpStream, conn: u64, hub: &Sender<HubIn>, limits: NetLimits, _open: Open) {
    let _ = stream.set_nodelay(true);
    if stream.set_read_timeout(Some(limits.hello_timeout)).is_err() || stream.set_write_timeout(Some(limits.write_timeout)).is_err() {
        return;
    }
    let Ok((rd, wr, sock)) = Duplex::split(stream) else { return };
    let cfg = config(limits.max_client_message);
    let mut ws = match tungstenite::accept_with_config(rd, Some(config(limits.max_hello))) {
        Ok(ws) => ws,
        Err(e) => {
            log::debug!("live session: handshake failed: {e}");
            shutdown(&sock);
            return;
        }
    };
    let (out, rx) = sync_channel::<Out>(limits.queue.max(4));
    let writer =
        match sock.try_clone().map(|s| spawn("live-write", move || write_loop(WebSocket::from_raw_socket(wr, Role::Server, Some(cfg)), &rx, &s))) {
            Ok(Ok(t)) => t,
            _ => {
                shutdown(&sock);
                return;
            }
        };
    let end = |why: Option<&str>| {
        if let Some(why) = why {
            refuse(&out, why);
        } else {
            let _ = out.try_send(Out::Close(CloseCode::Normal, String::new()));
        }
    };
    match next_message(&mut ws, limits.max_hello) {
        Ok(Some(ClientMsg::Hello(hello))) => {
            ws.set_config(|c| *c = cfg);
            let joined = sock.try_clone().is_ok_and(|s| hub.send(HubIn::Join { conn, hello, out: out.clone(), sock: s }).is_ok());
            if joined && ws.get_ref().set_read_timeout(Some(limits.idle_timeout)).is_ok() {
                read_loop(&mut ws, conn, hub, limits, &end);
                let _ = hub.send(HubIn::Gone { conn });
            }
        }
        Ok(Some(_)) => end(Some("say hello first")),
        Ok(None) => {}
        Err(why) => end(Some(why)),
    }
    linger(&mut ws);
    drop(out);
    // The writer closes the socket once it has sent what's queued (a refusal, a close).
    let _ = writer.join();
    shutdown(&sock);
}

fn read_loop(ws: &mut WebSocket<Duplex>, conn: u64, hub: &Sender<HubIn>, limits: NetLimits, end: &dyn Fn(Option<&str>)) {
    let mut bucket = Bucket::new(limits.rate, limits.burst);
    loop {
        match next_message(ws, limits.max_client_message) {
            Ok(Some(msg)) => {
                if !bucket.take(Instant::now()) {
                    log::info!("live session: connection {conn} sends too much; disconnecting it");
                    return end(Some("too many messages"));
                }
                if hub.send(HubIn::Msg { conn, msg }).is_err() {
                    return;
                }
            }
            Ok(None) => return end(None),
            Err(why) => return end(Some(why)),
        }
    }
}

fn write_loop(mut ws: WebSocket<Duplex>, rx: &Receiver<Out>, sock: &TcpStream) {
    let mut close = Some((CloseCode::Normal, String::new()));
    while let Ok(o) = rx.recv() {
        match o {
            Out::Bytes(b) => {
                if ws.send(Message::Binary(b)).is_err() {
                    close = None;
                    break;
                }
            }
            Out::Close(code, why) => {
                close = Some((code, why));
                break;
            }
        }
    }
    if let Some((code, why)) = close {
        // Close politely: the close frame, then no more writing; the reader drains what the peer
        // still sends until it hangs up (closing on unread data would reset the connection and
        // could lose the close frame), or until the linger runs out.
        let _ = ws.close(Some(CloseFrame { code, reason: why.into() }));
        let _ = sock.shutdown(std::net::Shutdown::Write);
        let until = Instant::now() + LINGER;
        while let Ok(_) | Err(RecvTimeoutError::Timeout) = rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            if Instant::now() >= until {
                break;
            }
        }
    }
    shutdown(sock);
}

/// Read and drop what the peer still sends after the close, until it hangs up (at most
/// [`LINGER`]).
fn linger(ws: &mut WebSocket<Duplex>) {
    let until = Instant::now() + LINGER;
    while Instant::now() < until && ws.get_ref().set_read_timeout(Some(Duration::from_millis(250))).is_ok() {
        match ws.read() {
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
}

/// A joined or joining connection.
struct Conn {
    out: SyncSender<Out>,
    sock: TcpStream,
    id: u32,
}

/// Owns the collaboration server and routes its messages.
struct Hub {
    server: Server,
    file: String,
    /// The host's own client id.
    local: u32,
    inbox: Inbox,
    conns: HashMap<u64, Conn>,
    /// Client id → connection.
    ids: BTreeMap<u32, u64>,
    throttles: BTreeMap<u32, Throttle<Presence>>,
    /// Everyone's latest presence (for newcomers).
    latest: BTreeMap<u32, Presence>,
    /// Default names handed out ("Guest 1", "Guest 2"…).
    guests: u32,
}

impl Hub {
    fn run(mut self, rx: &Receiver<HubIn>) {
        loop {
            let due = earliest(self.throttles.values().map(Throttle::due));
            let msg = match due {
                Some(d) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                    Ok(m) => Some(m),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => Some(HubIn::Stop),
                },
                None => Some(rx.recv().unwrap_or(HubIn::Stop)),
            };
            match msg {
                Some(HubIn::Stop) => break,
                Some(m) => self.handle(m),
                None => {}
            }
            self.flush_due(Instant::now());
        }
        for c in self.conns.values() {
            if c.out.try_send(Out::Close(CloseCode::Away, ENDED.into())).is_err() {
                shutdown(&c.sock);
            }
        }
    }

    fn handle(&mut self, m: HubIn) {
        match m {
            HubIn::Join { conn, hello, out, sock } => self.join(conn, hello, out, sock),
            HubIn::Msg { conn, msg } => {
                let Some(id) = self.conns.get(&conn).map(|c| c.id) else { return };
                match msg {
                    ClientMsg::Presence(p) => self.presence(id, p),
                    other => {
                        let outs = self.server.receive(id, other);
                        self.deliver(outs);
                    }
                }
            }
            HubIn::Gone { conn } => self.gone(conn),
            HubIn::Local(p) => self.presence(self.local, p),
            HubIn::LocalMsg(ClientMsg::Presence(p)) => self.presence(self.local, p),
            HubIn::LocalMsg(msg) => {
                let outs = self.server.receive(self.local, msg);
                self.deliver(outs);
            }
            #[cfg(test)]
            HubIn::Cut => {
                for c in self.conns.values() {
                    shutdown(&c.sock);
                }
            }
            HubIn::Stop => {}
        }
    }

    fn join(&mut self, conn: u64, mut hello: Hello, out: SyncSender<Out>, sock: TcpStream) {
        if hello.file != self.file {
            return refuse(&out, "this invite is for another file or an earlier session");
        }
        // Colours: the first one nobody has.
        let taken: Vec<&str> = self.server.peers().map(|p| p.color.as_str()).collect();
        let color = PEOPLE.iter().find(|c| !taken.contains(c)).copied().unwrap_or(PEOPLE[taken.len() % PEOPLE.len()]);
        hello.color = color.to_string();
        hello.name = match clean_name(&hello.name) {
            n if n.is_empty() => {
                self.guests += 1;
                format!("Guest {}", self.guests)
            }
            n => n,
        };
        match self.server.join(hello) {
            Ok((id, outs)) => {
                self.conns.insert(conn, Conn { out, sock, id });
                self.ids.insert(id, conn);
                self.deliver(outs);
                // The newcomer learns how it is shown, then where everyone is.
                let mut more = Vec::new();
                if let Some(me) = self.server.peers().find(|p| p.client_id == id) {
                    more.push(Outgoing::To(id, ServerMsg::PeerJoined(me.clone())));
                }
                more.extend(self.latest.iter().map(|(&from, p)| Outgoing::To(id, ServerMsg::Presence { client_id: from, presence: p.clone() })));
                self.deliver(more);
            }
            Err(ServerMsg::Refused { reason }) => refuse(&out, &reason),
            Err(_) => refuse(&out, "refused"),
        }
    }

    fn gone(&mut self, conn: u64) {
        let Some(c) = self.conns.remove(&conn) else { return };
        self.ids.remove(&c.id);
        self.throttles.remove(&c.id);
        self.latest.remove(&c.id);
        let outs = self.server.leave(c.id);
        self.deliver(outs);
    }

    fn presence(&mut self, id: u32, p: Presence) {
        let p = sanitize(p);
        self.latest.insert(id, p.clone());
        let now = Instant::now();
        if let Some(p) = self.throttles.entry(id).or_insert_with(|| Throttle::new(PRESENCE_INTERVAL)).offer(p, now) {
            let outs = self.server.receive(id, ClientMsg::Presence(p));
            self.deliver(outs);
        }
    }

    fn flush_due(&mut self, now: Instant) {
        let due: Vec<(u32, Presence)> = self.throttles.iter_mut().filter_map(|(id, t)| t.take_due(now).map(|p| (*id, p))).collect();
        for (id, p) in due {
            let outs = self.server.receive(id, ClientMsg::Presence(p));
            self.deliver(outs);
        }
    }

    /// Send what the server said; connections that can't keep up are dropped.
    fn deliver(&mut self, outs: Vec<Outgoing>) {
        let mut stuck = Vec::new();
        for o in outs {
            let (to, msg): (Vec<u32>, ServerMsg) = match o {
                Outgoing::To(id, m) => (vec![id], m),
                Outgoing::All(m) => (self.server.peers().map(|p| p.client_id).collect(), m),
                Outgoing::Others(x, m) => (self.server.peers().map(|p| p.client_id).filter(|id| *id != x).collect(), m),
            };
            let mut bytes = None;
            for id in to {
                if id == self.local {
                    self.to_local(&msg);
                    continue;
                }
                let Some(c) = self.ids.get(&id).and_then(|conn| self.conns.get(conn)) else { continue };
                let Some(b) = bytes.get_or_insert_with(|| encode(&msg)).clone() else { continue };
                match c.out.try_send(Out::Bytes(b)) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        log::info!("live session: client {id} can't keep up; disconnecting it");
                        stuck.push(id);
                    }
                    Err(TrySendError::Disconnected(_)) => stuck.push(id),
                }
            }
        }
        for id in stuck {
            if let Some(&conn) = self.ids.get(&id) {
                if let Some(c) = self.conns.get(&conn) {
                    shutdown(&c.sock);
                }
                self.gone(conn);
            }
        }
    }

    /// What the host's own window should know.
    fn to_local(&self, msg: &ServerMsg) {
        match msg {
            ServerMsg::PeerJoined(p) if p.client_id != self.local => self.inbox.push(Event::PeerJoined(p.clone())),
            ServerMsg::PeerLeft { client_id } => self.inbox.push(Event::PeerLeft(*client_id)),
            ServerMsg::Presence { client_id, presence } => self.inbox.presence(*client_id, presence.clone()),
            ServerMsg::Changes { .. } | ServerMsg::Reject { .. } => self.inbox.push(Event::Sync(msg.clone())),
            _ => {}
        }
    }
}
