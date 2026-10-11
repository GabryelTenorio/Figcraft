//! The hub every host runs, for one file (a session the app hosts) or many (`figcraft-server`):
//! a WebSocket listener, a reader and a writer thread per connection, and one hub thread that owns
//! a collaboration core [`Server`] per open file and routes its messages.
//!
//! What differs between the two lives behind a [`Backend`]: which hellos it admits and with what
//! access, where a file that isn't open comes from, what is kept of the changes (a server appends
//! them to its log; the app keeps nothing), and the managing connections a server answers. The
//! person hosting an app session takes part through the hub directly (no socket): their presence
//! and edits go in with [`Hosting::presence`] and [`Hosting::send`], what others do comes out
//! through their [`Inbox`].
//!
//! On top of the core the hub hands out colours and default names per file, throttles presence to
//! about 30 updates a second per person (latest wins), tells a newcomer its own name and colour (a
//! `PeerJoined` of itself, after its welcome) and where everyone already is, and disconnects
//! clients that send too much, too fast, or can't keep up. Whatever the servers say goes out only
//! after the backend has made what changed durable ([`Backend::flush`], once per burst of
//! messages), so a confirmed edit is never lost to a crash.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use figcraft_collab::message::{self, Access, Admin, AdminReply, ClientMsg, Hello, Manage, PROTOCOL, Presence, ServerMsg};
use figcraft_collab::{Outgoing, Server};
use figcraft_engine::live::{Event, MAX_NAME, PEOPLE, sanitize};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, Role, WebSocket};
use tungstenite::{Bytes, Message};

use crate::inbox::Inbox;
use crate::join::{ENDED, RESTARTING};
use crate::net::{Bucket, Duplex, NetLimits, config, shutdown};
use crate::throttle::{PRESENCE_INTERVAL, Throttle, earliest};

/// How long a closing connection waits for the peer to hang up.
const LINGER: Duration = Duration::from_secs(2);

/// Most messages the hub handles before it sends what they produced (bounds the delay a burst
/// adds).
const BURST: usize = 256;

/// What a link lets a hello do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admission {
    pub access: Access,
    /// The link's id, when the backend has several per file (revoking it disconnects whoever
    /// joined with it).
    pub link: Option<String>,
}

/// An admin request's reply, and what the hub must do about connections it affects.
#[derive(Clone, Debug, PartialEq)]
pub struct Handled {
    pub reply: AdminReply,
    /// A link was revoked: `(file, link)`; whoever joined with it is disconnected.
    pub revoked: Option<(String, String)>,
    /// A file was removed: whoever is in it is disconnected and it is closed without saving.
    pub removed: Option<String>,
}

impl From<AdminReply> for Handled {
    fn from(reply: AdminReply) -> Self {
        Self { reply, revoked: None, removed: None }
    }
}

/// What everyone is told when a hub stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closing {
    /// The session is over (the app stopped hosting): final.
    Ended,
    /// The server is restarting: clients reconnect until it is back.
    Restarting,
}

/// What the hub knows of an open file (for listings).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenFile {
    /// People connected to it.
    pub people: u32,
    /// Its latest sequence number.
    pub seq: u64,
}

/// Where a hub's files come from and what it keeps of them (see the module docs). The hub calls it
/// from its own thread only.
pub trait Backend: Send + 'static {
    /// A hello for `file` with `token`: what it may do, or why it may not join (the reason is
    /// shown to the person joining; never echo the token).
    fn admit(&mut self, file: &str, token: &str) -> Result<Admission, String>;

    /// The session for a file that isn't open (after [`admit`](Self::admit) let someone in).
    fn open(&mut self, file: &str) -> Result<Server, String>;

    /// The hub handled a message for an open file: keep what changed (its server's journal).
    fn changed(&mut self, _file: &str, _server: &mut Server) {}

    /// Make what [`changed`](Self::changed) kept durable; the hub sends the servers' answers
    /// after this.
    fn flush(&mut self) {}

    /// Everyone left `file`: save it. True closes it (the next hello opens it again).
    fn idle(&mut self, _file: &str, _server: &mut Server) -> bool {
        false
    }

    /// The hub is stopping: save the open files.
    fn stop(&mut self, _open: Vec<(&str, &mut Server)>) {}

    /// A managing connection's key: whether it may manage files.
    fn manage(&mut self, _key: &str) -> Result<(), String> {
        Err("this session is hosted by an app: it has no files to manage".into())
    }

    /// A request on a managing connection; `open` tells about a file that is open.
    fn admin(&mut self, _req: Admin, _open: &dyn Fn(&str) -> Option<OpenFile>) -> Handled {
        AdminReply::Failed { reason: "nothing to manage here".into() }.into()
    }

    /// What everyone is told when the hub stops.
    fn closing(&self) -> Closing;
}

/// A message for one connection's writer.
enum Out {
    Bytes(Bytes),
    Close(CloseCode, String),
}

enum HubIn {
    /// A connection's first message (`Hello` or `Manage`).
    Open {
        conn: u64,
        first: ClientMsg,
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

/// The person hosting an app session: in the file's session without a socket.
pub(crate) struct Local {
    pub file: String,
    pub server: Server,
    /// Their client id (they joined `server` already).
    pub id: u32,
    pub inbox: Inbox,
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name(name.to_string()).spawn(f)
}

/// A name others may see: trimmed, no control characters, at most [`MAX_NAME`] characters.
pub(crate) fn clean_name(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME).collect()
}

/// A running hub: its listener, connections and hub thread. Dropping it stops it.
pub struct Hosting {
    addr: SocketAddr,
    hub: Sender<HubIn>,
    stop: Arc<AtomicBool>,
    open: Arc<AtomicUsize>,
    hub_thread: Option<JoinHandle<()>>,
    accept_thread: Option<JoinHandle<()>>,
}

impl Hosting {
    /// Serve `backend`'s files on `listener`.
    pub fn serve<B: Backend>(listener: TcpListener, backend: B, limits: NetLimits) -> io::Result<Hosting> {
        Self::start(listener, backend, limits, None)
    }

    pub(crate) fn start<B: Backend>(listener: TcpListener, backend: B, limits: NetLimits, local: Option<Local>) -> io::Result<Hosting> {
        let addr = listener.local_addr()?;
        let mut hub = Hub { backend, rooms: HashMap::new(), conns: HashMap::new(), local: None, outbox: Vec::new() };
        if let Some(l) = local {
            let mut room = Room::new(l.server);
            room.local = Some((l.id, l.inbox));
            hub.rooms.insert(l.file.clone(), room);
            hub.local = Some(l.file);
        }
        let (hub_tx, hub_rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let open = Arc::new(AtomicUsize::new(0));
        let hub_thread = spawn("live-hub", move || hub.run(&hub_rx))?;
        let accept_thread = {
            let (hub_tx, stop, open) = (hub_tx.clone(), stop.clone(), open.clone());
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
        log::info!("live session: listening on {addr}");
        Ok(Hosting { addr, hub: hub_tx, stop, open, hub_thread: Some(hub_thread), accept_thread: Some(accept_thread) })
    }

    /// Where it listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Open connections (joined or not).
    pub fn connections(&self) -> usize {
        self.open.load(Ordering::Relaxed)
    }

    /// The hosting person's presence (app sessions).
    pub(crate) fn presence(&self, p: Presence) {
        let _ = self.hub.send(HubIn::Local(p));
    }

    /// A message from the hosting person's sync client (their edits).
    pub(crate) fn send(&self, msg: ClientMsg) {
        let _ = self.hub.send(HubIn::LocalMsg(msg));
    }

    /// Cut every connection as a network failure would (no close frame).
    #[cfg(test)]
    pub(crate) fn cut_connections(&self) {
        let _ = self.hub.send(HubIn::Cut);
    }

    /// Stop: the backend saves what's open, everyone is told and disconnected, the port is
    /// released.
    pub fn stop(&mut self) {
        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.hub.send(HubIn::Stop);
        if let Some(t) = self.hub_thread.take() {
            let _ = t.join();
        }
        // The accept loop checks the flag after each connection: wake it with one.
        let wake = if self.addr.ip().is_unspecified() { SocketAddr::new(loopback(self.addr), self.addr.port()) } else { self.addr };
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
        log::info!("live session: stopped listening on {}", self.addr);
    }
}

impl Drop for Hosting {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The loopback address of `addr`'s family.
fn loopback(addr: SocketAddr) -> std::net::IpAddr {
    if addr.is_ipv6() { std::net::Ipv6Addr::LOCALHOST.into() } else { std::net::Ipv4Addr::LOCALHOST.into() }
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
        Ok(Some(first @ (ClientMsg::Hello(_) | ClientMsg::Manage(_)))) => {
            ws.set_config(|c| *c = cfg);
            let opened = sock.try_clone().is_ok_and(|s| hub.send(HubIn::Open { conn, first, out: out.clone(), sock: s }).is_ok());
            if opened && ws.get_ref().set_read_timeout(Some(limits.idle_timeout)).is_ok() {
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

/// What a connection is for.
enum Kind {
    /// In `file`'s session as client `id`, through link `link`.
    Joined { file: String, id: u32, link: Option<String> },
    /// Managing the server's files.
    Managing,
}

/// A joined or managing connection.
struct Conn {
    out: SyncSender<Out>,
    sock: TcpStream,
    kind: Kind,
}

/// One open file's session.
struct Room {
    server: Server,
    /// Client id → connection.
    ids: BTreeMap<u32, u64>,
    throttles: BTreeMap<u32, Throttle<Presence>>,
    /// Everyone's latest presence (for newcomers).
    latest: BTreeMap<u32, Presence>,
    /// Default names handed out ("Guest 1", "Guest 2"…).
    guests: u32,
    /// The person hosting (app sessions): their client id and inbox.
    local: Option<(u32, Inbox)>,
}

impl Room {
    fn new(server: Server) -> Self {
        Self { server, ids: BTreeMap::new(), throttles: BTreeMap::new(), latest: BTreeMap::new(), guests: 0, local: None }
    }

    /// Nobody is in it.
    fn empty(&self) -> bool {
        self.ids.is_empty() && self.local.is_none()
    }
}

/// Owns the open files' servers and routes their messages.
struct Hub<B> {
    backend: B,
    rooms: HashMap<String, Room>,
    conns: HashMap<u64, Conn>,
    /// The file the person hosting is in (app sessions).
    local: Option<String>,
    /// What the servers said, sent once the backend made what changed durable.
    outbox: Vec<(String, Outgoing)>,
}

impl<B: Backend> Hub<B> {
    fn run(mut self, rx: &Receiver<HubIn>) {
        'run: loop {
            let due = earliest(self.rooms.values().flat_map(|r| r.throttles.values().map(Throttle::due)));
            let first = match due {
                Some(d) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                    Ok(m) => Some(m),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => Some(HubIn::Stop),
                },
                None => Some(rx.recv().unwrap_or(HubIn::Stop)),
            };
            // A burst: handle what's waiting, then make it durable and answer.
            let mut next = first;
            let mut handled = 0;
            while let Some(m) = next.take() {
                if matches!(m, HubIn::Stop) {
                    break 'run;
                }
                self.handle(m);
                handled += 1;
                if handled < BURST {
                    next = rx.try_recv().ok();
                }
            }
            self.flush_due(Instant::now());
            self.send_out();
        }
        self.send_out();
        self.backend.stop(self.rooms.iter_mut().map(|(f, r)| (f.as_str(), &mut r.server)).collect());
        let (code, why) = match self.backend.closing() {
            Closing::Ended => (CloseCode::Away, ENDED),
            Closing::Restarting => (CloseCode::Restart, RESTARTING),
        };
        for c in self.conns.values() {
            if c.out.try_send(Out::Close(code, why.into())).is_err() {
                shutdown(&c.sock);
            }
        }
    }

    fn handle(&mut self, m: HubIn) {
        match m {
            HubIn::Open { conn, first: ClientMsg::Hello(hello), out, sock } => self.join(conn, hello, out, sock),
            HubIn::Open { conn, first: ClientMsg::Manage(m), out, sock } => self.manage(conn, &m, out, sock),
            HubIn::Open { out, .. } => refuse(&out, "say hello first"),
            HubIn::Msg { conn, msg } => match self.conns.get(&conn).map(|c| &c.kind) {
                Some(Kind::Joined { file, id, .. }) => {
                    let (file, id) = (file.clone(), *id);
                    self.client_message(&file, id, msg);
                }
                Some(Kind::Managing) => match msg {
                    ClientMsg::Admin(req) => self.admin(conn, req),
                    ClientMsg::Ping => self.answer(conn, &ServerMsg::Pong),
                    _ => self.kick(&[conn], "only requests to manage files are spoken on this connection"),
                },
                None => {}
            },
            HubIn::Gone { conn } => self.gone(conn),
            HubIn::Local(p) | HubIn::LocalMsg(ClientMsg::Presence(p)) => {
                if let Some((file, id)) = self.local_id() {
                    self.presence(&file, id, p);
                }
            }
            HubIn::LocalMsg(msg) => {
                if let Some((file, id)) = self.local_id() {
                    self.client_message(&file, id, msg);
                }
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

    /// The hosting person's file and client id.
    fn local_id(&self) -> Option<(String, u32)> {
        let file = self.local.clone()?;
        let id = self.rooms.get(&file)?.local.as_ref()?.0;
        Some((file, id))
    }

    /// A message from client `id` of `file`.
    fn client_message(&mut self, file: &str, id: u32, msg: ClientMsg) {
        if let ClientMsg::Presence(p) = msg {
            return self.presence(file, id, p);
        }
        let Some(room) = self.rooms.get_mut(file) else { return };
        let outs = room.server.receive(id, msg);
        self.backend.changed(file, &mut room.server);
        self.queue(file, outs);
    }

    fn queue(&mut self, file: &str, outs: Vec<Outgoing>) {
        self.outbox.extend(outs.into_iter().map(|o| (file.to_string(), o)));
    }

    fn join(&mut self, conn: u64, mut hello: Hello, out: SyncSender<Out>, sock: TcpStream) {
        let admission = match self.backend.admit(&hello.file, &hello.token) {
            Ok(a) => a,
            Err(why) => return refuse(&out, &why),
        };
        // The backend checked the secret: the sessions don't keep it.
        hello.token.clear();
        let file = hello.file.clone();
        if !self.rooms.contains_key(&file) {
            match self.backend.open(&file) {
                Ok(server) => {
                    self.rooms.insert(file.clone(), Room::new(server));
                }
                Err(why) => return refuse(&out, &why),
            }
        }
        let Some(room) = self.rooms.get_mut(&file) else { return };
        // Colours: the first one nobody has.
        let taken: Vec<&str> = room.server.peers().map(|p| p.color.as_str()).collect();
        let color = PEOPLE.iter().find(|c| !taken.contains(c)).copied().unwrap_or(PEOPLE[taken.len() % PEOPLE.len()]);
        hello.color = color.to_string();
        hello.name = match clean_name(&hello.name) {
            n if n.is_empty() => {
                room.guests += 1;
                format!("Guest {}", room.guests)
            }
            n => n,
        };
        match room.server.join_as(hello, admission.access) {
            Ok((id, outs)) => {
                // A guest back after a drop keeps its id: its old connection, if the hub hasn't
                // noticed it's gone yet, is closed without the guest leaving.
                let old = room.ids.insert(id, conn).filter(|old| *old != conn);
                self.backend.changed(&file, &mut room.server);
                // The newcomer learns how it is shown, then where everyone is.
                let mut more = outs;
                if let Some(me) = room.server.peers().find(|p| p.client_id == id) {
                    more.push(Outgoing::To(id, ServerMsg::PeerJoined(me.clone())));
                }
                more.extend(room.latest.iter().map(|(&from, p)| Outgoing::To(id, ServerMsg::Presence { client_id: from, presence: p.clone() })));
                if let Some(c) = old.and_then(|old| self.conns.remove(&old)) {
                    shutdown(&c.sock);
                }
                self.conns.insert(conn, Conn { out, sock, kind: Kind::Joined { file: file.clone(), id, link: admission.link } });
                self.queue(&file, more);
            }
            Err(ServerMsg::Refused { reason }) => {
                refuse(&out, &reason);
                self.close_if_empty(&file);
            }
            Err(_) => {
                refuse(&out, "refused");
                self.close_if_empty(&file);
            }
        }
    }

    fn manage(&mut self, conn: u64, m: &Manage, out: SyncSender<Out>, sock: TcpStream) {
        if m.protocol != PROTOCOL {
            return refuse(&out, &format!("protocol {} isn't supported (this server speaks {PROTOCOL})", m.protocol));
        }
        if let Err(why) = self.backend.manage(&m.key) {
            return refuse(&out, &why);
        }
        log::info!("live session: connection {conn} manages files");
        self.conns.insert(conn, Conn { out, sock, kind: Kind::Managing });
        self.answer(conn, &ServerMsg::Admin(Box::new(AdminReply::Ready)));
    }

    fn admin(&mut self, conn: u64, req: Admin) {
        let rooms = &self.rooms;
        let open = |file: &str| {
            rooms.get(file).map(|r| OpenFile { people: u32::try_from(r.server.peers().count()).unwrap_or(u32::MAX), seq: r.server.seq() })
        };
        let handled = self.backend.admin(req, &open);
        self.answer(conn, &ServerMsg::Admin(Box::new(handled.reply)));
        if let Some((file, link)) = handled.revoked {
            let users: Vec<u64> = self
                .conns
                .iter()
                .filter(|(_, c)| matches!(&c.kind, Kind::Joined { file: f, link: Some(l), .. } if *f == file && *l == link))
                .map(|(k, _)| *k)
                .collect();
            self.kick(&users, "the link you joined with was revoked");
        }
        if let Some(file) = handled.removed {
            let users: Vec<u64> =
                self.conns.iter().filter(|(_, c)| matches!(&c.kind, Kind::Joined { file: f, .. } if *f == file)).map(|(k, _)| *k).collect();
            // Closed without saving: the file is gone.
            self.rooms.remove(&file);
            self.kick(&users, "this file was removed from the server");
        }
    }

    /// Send one message to one connection now (managing connections' answers).
    fn answer(&mut self, conn: u64, msg: &ServerMsg) {
        let Some(c) = self.conns.get(&conn) else { return };
        let sent = encode(msg).is_some_and(|b| c.out.try_send(Out::Bytes(b)).is_ok());
        if !sent {
            shutdown(&c.sock);
            self.gone(conn);
        }
    }

    /// Refuse connections with `why` and drop them at once (nothing more they send counts).
    fn kick(&mut self, conns: &[u64], why: &str) {
        for conn in conns {
            if let Some(c) = self.conns.get(conn) {
                refuse(&c.out, why);
            }
            self.gone(*conn);
        }
    }

    fn gone(&mut self, conn: u64) {
        let Some(c) = self.conns.remove(&conn) else { return };
        let Kind::Joined { file, id, .. } = c.kind else { return };
        let Some(room) = self.rooms.get_mut(&file) else { return };
        if room.ids.get(&id) == Some(&conn) {
            room.ids.remove(&id);
        }
        room.throttles.remove(&id);
        room.latest.remove(&id);
        let outs = room.server.leave(id);
        self.queue(&file, outs);
        self.close_if_empty(&file);
    }

    /// Nobody is in `file`: the backend saves it and may close it.
    fn close_if_empty(&mut self, file: &str) {
        let Some(room) = self.rooms.get_mut(file).filter(|r| r.empty()) else { return };
        if self.backend.idle(file, &mut room.server) {
            self.rooms.remove(file);
        }
    }

    fn presence(&mut self, file: &str, id: u32, p: Presence) {
        let Some(room) = self.rooms.get_mut(file) else { return };
        let p = sanitize(p);
        room.latest.insert(id, p.clone());
        if let Some(p) = room.throttles.entry(id).or_insert_with(|| Throttle::new(PRESENCE_INTERVAL)).offer(p, Instant::now()) {
            let outs = room.server.receive(id, ClientMsg::Presence(p));
            self.queue(file, outs);
        }
    }

    fn flush_due(&mut self, now: Instant) {
        let mut outs = Vec::new();
        for (file, room) in &mut self.rooms {
            let due: Vec<(u32, Presence)> = room.throttles.iter_mut().filter_map(|(id, t)| t.take_due(now).map(|p| (*id, p))).collect();
            for (id, p) in due {
                outs.extend(room.server.receive(id, ClientMsg::Presence(p)).into_iter().map(|o| (file.clone(), o)));
            }
        }
        self.outbox.extend(outs);
    }

    /// Make what changed durable, then send what the servers said; connections that can't keep up
    /// are dropped (which may say more: that they left).
    fn send_out(&mut self) {
        self.backend.flush();
        while !self.outbox.is_empty() {
            let mut stuck = Vec::new();
            for (file, o) in std::mem::take(&mut self.outbox) {
                let Some(room) = self.rooms.get(&file) else { continue };
                let (to, msg): (Vec<u32>, ServerMsg) = match o {
                    Outgoing::To(id, m) => (vec![id], m),
                    Outgoing::All(m) => (room.server.peers().map(|p| p.client_id).collect(), m),
                    Outgoing::Others(x, m) => (room.server.peers().map(|p| p.client_id).filter(|id| *id != x).collect(), m),
                };
                let mut bytes = None;
                for id in to {
                    if let Some((local, inbox)) = room.local.as_ref().filter(|(l, _)| *l == id) {
                        to_local(inbox, *local, &msg);
                        continue;
                    }
                    let Some((&conn, c)) = room.ids.get(&id).and_then(|conn| self.conns.get_key_value(conn)) else { continue };
                    let Some(b) = bytes.get_or_insert_with(|| encode(&msg)).clone() else { continue };
                    match c.out.try_send(Out::Bytes(b)) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {
                            log::info!("live session: client {id} can't keep up; disconnecting it");
                            stuck.push(conn);
                        }
                        Err(TrySendError::Disconnected(_)) => stuck.push(conn),
                    }
                }
            }
            for conn in stuck {
                if let Some(c) = self.conns.get(&conn) {
                    shutdown(&c.sock);
                }
                self.gone(conn);
            }
        }
    }
}

/// What the hosting person's window should know.
fn to_local(inbox: &Inbox, local: u32, msg: &ServerMsg) {
    match msg {
        ServerMsg::PeerJoined(p) if p.client_id != local => inbox.push(Event::PeerJoined(p.clone())),
        ServerMsg::PeerLeft { client_id } => inbox.push(Event::PeerLeft(*client_id)),
        ServerMsg::Presence { client_id, presence } => inbox.presence(*client_id, presence.clone()),
        ServerMsg::Changes { .. } | ServerMsg::Reject { .. } => inbox.push(Event::Sync(msg.clone())),
        _ => {}
    }
}
