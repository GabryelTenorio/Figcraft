//! Joining a session: the client end of the transport, with reconnect.
//!
//! A supervisor thread connects, says hello, then writes (presence throttled to about 30 a
//! second, latest wins; a ping when quiet) while a reader thread takes in what the host sends.
//! When the connection drops after having been live, it reconnects with growing pauses; a
//! refusal or the host ending the session is final.

use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use figcraft_collab::message::{self, ClientMsg, Peer, Presence, ServerMsg};
use figcraft_collab::{Client, Replica};
use figcraft_engine::live::{Event, PageInfo, Role, SessionInfo, Status};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, Role as WsRole, WebSocket};
use tungstenite::{Bytes, Message};

use crate::host::ENDED;
use crate::inbox::Inbox;
use crate::invite::Invite;
use crate::net::{Duplex, NetLimits, config, shutdown};
use crate::throttle::{PRESENCE_INTERVAL, Throttle, earliest};

/// How a guest connects and reconnects.
#[derive(Clone, Debug)]
pub struct GuestConfig {
    pub invite: Invite,
    /// Your name (empty: the host picks one).
    pub name: String,
    pub limits: NetLimits,
    pub connect_timeout: Duration,
    /// The pause before the first reconnect; it doubles each time up to `max_backoff`.
    pub backoff: Duration,
    pub max_backoff: Duration,
    /// Reconnect attempts before giving up.
    pub attempts: u32,
}

impl GuestConfig {
    pub fn new(invite: Invite, name: String) -> Self {
        Self {
            invite,
            name,
            limits: NetLimits::default(),
            connect_timeout: Duration::from_secs(5),
            backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(10),
            attempts: 12,
        }
    }
}

/// A joined (or joining) session. Dropping it leaves.
pub struct Guest {
    cmd: Sender<Cmd>,
    thread: Option<JoinHandle<()>>,
}

enum Cmd {
    Presence(Presence),
    Leave,
    /// The reader of connection `epoch` got its welcome.
    Welcomed(u64),
    /// Connection `epoch` ended.
    Lost(u64, Outcome),
}

/// How a connection ended.
enum Outcome {
    /// Dropped: try again.
    Retry(String),
    /// The host said no (bad token, full…): final.
    Refused(String),
    /// The host ended the session: final.
    Ended(String),
    /// You left.
    Left,
}

impl Guest {
    /// Start joining (returns at once; progress arrives in `inbox`).
    pub fn start(cfg: GuestConfig, inbox: Inbox) -> io::Result<Guest> {
        let (tx, rx) = channel();
        let me = tx.clone();
        let thread = std::thread::Builder::new().name("live-guest".into()).spawn(move || supervise(&cfg, &inbox, &rx, &me))?;
        Ok(Guest { cmd: tx, thread: Some(thread) })
    }

    /// Your latest presence.
    pub fn presence(&self, p: Presence) {
        let _ = self.cmd.send(Cmd::Presence(p));
    }

    /// Leave (returns at once: a connection attempt in progress ends on its own).
    pub fn leave(&mut self) {
        let _ = self.cmd.send(Cmd::Leave);
    }

    /// The connection thread has ended.
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for Guest {
    fn drop(&mut self) {
        self.leave();
        if let Some(t) = self.thread.take()
            && t.is_finished()
        {
            let _ = t.join();
        }
    }
}

/// A connected socket: reader and writer ends.
struct Conn {
    reader: WebSocket<Duplex>,
    writer: WebSocket<Duplex>,
    sock: TcpStream,
}

fn connect(cfg: &GuestConfig) -> Result<Conn, String> {
    let addrs: Vec<SocketAddr> = cfg.invite.host.to_socket_addrs().map_err(|e| format!("can't find {}: {e}", cfg.invite.host))?.collect();
    let mut last = format!("can't find {}", cfg.invite.host);
    let mut stream = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, cfg.connect_timeout) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = format!("no answer from {a}: {e}"),
        }
    }
    let stream = stream.ok_or(last)?;
    let _ = stream.set_nodelay(true);
    stream.set_read_timeout(Some(cfg.limits.hello_timeout)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(cfg.limits.write_timeout)).map_err(|e| e.to_string())?;
    let (rd, wr, sock) = Duplex::split(stream).map_err(|e| e.to_string())?;
    let (reader, _) = tungstenite::client::client_with_config(cfg.invite.endpoint(), rd, Some(config(cfg.limits.max_server_message)))
        .map_err(|e| format!("{} doesn't answer as a FigCraft session: {e}", cfg.invite.host))?;
    let writer = WebSocket::from_raw_socket(wr, WsRole::Client, Some(config(cfg.limits.max_client_message)));
    reader.get_ref().set_read_timeout(Some(cfg.limits.idle_timeout)).map_err(|e| e.to_string())?;
    Ok(Conn { reader, writer, sock })
}

fn backoff(cfg: &GuestConfig, attempt: u32) -> Duration {
    cfg.backoff.saturating_mul(1u32 << attempt.saturating_sub(1).min(16)).min(cfg.max_backoff)
}

fn supervise(cfg: &GuestConfig, inbox: &Inbox, rx: &Receiver<Cmd>, me: &Sender<Cmd>) {
    let mut attempt = 0u32;
    let mut was_live = false;
    let mut pending: Option<Presence> = None;
    let mut epoch = 0u64;
    inbox.push(Event::Status(Status::Connecting));
    loop {
        epoch += 1;
        let outcome = match connect(cfg) {
            Ok(c) => session(c, cfg, inbox, rx, me, epoch, &mut pending, &mut was_live, &mut attempt),
            Err(e) => Outcome::Retry(e),
        };
        match outcome {
            Outcome::Left => return,
            Outcome::Refused(why) => return inbox.push(Event::Status(Status::Failed { reason: format!("The host refused: {why}") })),
            Outcome::Ended(why) => return inbox.push(Event::Ended { reason: why }),
            Outcome::Retry(why) if !was_live => {
                return inbox.push(Event::Status(Status::Failed { reason: format!("Couldn't join the live session: {why}") }));
            }
            Outcome::Retry(why) => {
                attempt += 1;
                if attempt > cfg.attempts {
                    return inbox.push(Event::Status(Status::Failed { reason: format!("Lost the connection to the host ({why})") }));
                }
                log::info!("live session: connection lost ({why}); reconnecting, attempt {attempt}");
                inbox.push(Event::Status(Status::Reconnecting { attempt }));
                let until = Instant::now() + backoff(cfg, attempt);
                loop {
                    match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                        Ok(Cmd::Leave) | Err(RecvTimeoutError::Disconnected) => return,
                        Ok(Cmd::Presence(p)) => pending = Some(p),
                        Ok(_) => {}
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                }
            }
        }
    }
}

/// One connection: say hello, then write until it ends.
fn session(
    c: Conn,
    cfg: &GuestConfig,
    inbox: &Inbox,
    rx: &Receiver<Cmd>,
    me: &Sender<Cmd>,
    epoch: u64,
    pending: &mut Option<Presence>,
    was_live: &mut bool,
    attempt: &mut u32,
) -> Outcome {
    let Conn { reader, mut writer, sock } = c;
    let reader = {
        let ctx = ReaderCtx { inbox: inbox.clone(), tx: me.clone(), epoch, cfg: cfg.clone() };
        match std::thread::Builder::new().name("live-guest-read".into()).spawn(move || read_loop(reader, &ctx)) {
            Ok(t) => t,
            Err(e) => {
                shutdown(&sock);
                return Outcome::Retry(e.to_string());
            }
        }
    };
    let mut last_sent = Instant::now();
    let hello = Client::hello(&cfg.invite.file, &cfg.invite.token, &cfg.name, "");
    let mut welcomed = false;
    let mut throttle = Throttle::new(PRESENCE_INTERVAL);
    let outcome = if send(&mut writer, &hello, &mut last_sent) {
        loop {
            let ping_at = last_sent + cfg.limits.keepalive;
            let wake = earliest([throttle.due(), Some(ping_at)]).unwrap_or(ping_at);
            let mut out = None;
            match rx.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Cmd::Presence(p)) if welcomed => out = throttle.offer(p, Instant::now()),
                Ok(Cmd::Presence(p)) => *pending = Some(p),
                Ok(Cmd::Welcomed(e)) if e == epoch => {
                    welcomed = true;
                    *was_live = true;
                    *attempt = 0;
                    out = pending.take().and_then(|p| throttle.offer(p, Instant::now()));
                }
                Ok(Cmd::Lost(e, o)) if e == epoch => break o,
                Ok(Cmd::Leave) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = writer.close(Some(CloseFrame { code: CloseCode::Normal, reason: "".into() }));
                    break Outcome::Left;
                }
                // From an earlier connection.
                Ok(Cmd::Welcomed(_) | Cmd::Lost(..)) => {}
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            let out = out.or_else(|| throttle.take_due(now));
            let ok = match out {
                Some(p) => send(&mut writer, &ClientMsg::Presence(p), &mut last_sent),
                None if now >= last_sent + cfg.limits.keepalive => send(&mut writer, &ClientMsg::Ping, &mut last_sent),
                None => true,
            };
            if !ok {
                break Outcome::Retry("the connection dropped".into());
            }
        }
    } else {
        Outcome::Retry("the connection dropped".into())
    };
    shutdown(&sock);
    let _ = reader.join();
    outcome
}

/// Send one message; false when the connection is gone.
fn send(ws: &mut WebSocket<Duplex>, msg: &ClientMsg, last_sent: &mut Instant) -> bool {
    *last_sent = Instant::now();
    message::encode(msg).is_ok_and(|b| ws.send(Message::Binary(Bytes::from(b))).is_ok())
}

struct ReaderCtx {
    inbox: Inbox,
    tx: Sender<Cmd>,
    epoch: u64,
    cfg: GuestConfig,
}

/// What the reader knows of the session (to tell the UI who's here).
struct Seen {
    info: Option<SessionInfo>,
    peers: Vec<Peer>,
}

fn read_loop(mut ws: WebSocket<Duplex>, ctx: &ReaderCtx) {
    let mut seen = Seen { info: None, peers: Vec::new() };
    let outcome = loop {
        match ws.read() {
            Ok(Message::Binary(b)) => match message::decode::<ServerMsg>(&b, ctx.cfg.limits.max_server_message) {
                Ok(m) => {
                    if let Some(o) = take(m, ctx, &mut seen) {
                        break o;
                    }
                }
                Err(e) => break Outcome::Refused(format!("the host sent a message this version can't read ({e})")),
            },
            Ok(Message::Close(f)) => {
                break match f {
                    Some(f) if f.code == CloseCode::Away => {
                        Outcome::Ended(if f.reason.is_empty() { ENDED.to_string() } else { f.reason.to_string() })
                    }
                    Some(f) if f.code == CloseCode::Policy => Outcome::Refused(f.reason.to_string()),
                    _ => Outcome::Retry("the host closed the connection".into()),
                };
            }
            Ok(Message::Text(_)) => break Outcome::Refused("the host sent a text message".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Capacity(_)) => break Outcome::Refused("the host sent a message over the size limit".into()),
            Err(e) => break Outcome::Retry(e.to_string()),
        }
    };
    let _ = ctx.tx.send(Cmd::Lost(ctx.epoch, outcome));
}

/// Take in one message from the host; `Some` when the connection should end.
fn take(m: ServerMsg, ctx: &ReaderCtx, seen: &mut Seen) -> Option<Outcome> {
    match m {
        ServerMsg::Welcome { client_id, snapshot, peers, .. } => {
            // Documents don't sync yet: the snapshot only tells the shared file's name and pages.
            let (file_name, pages) = match Replica::from_snapshot(&snapshot) {
                Ok(r) => {
                    let d = r.document();
                    let pages = d.pages.iter().map(|p| PageInfo { id: p.0, name: d.node(*p).map(|n| n.name.clone()).unwrap_or_default() }).collect();
                    (d.name.clone(), pages)
                }
                Err(e) => {
                    log::warn!("live session: can't read the shared file's snapshot: {e}");
                    (String::new(), Vec::new())
                }
            };
            let inv = &ctx.cfg.invite;
            let network = inv.host.parse::<SocketAddr>().map_or(true, |a| !a.ip().is_loopback());
            let info = SessionInfo {
                role: Role::Guest,
                me: client_id,
                name: ctx.cfg.name.clone(),
                color: String::new(),
                file_name,
                pages,
                link: inv.link(),
                url: inv.url(),
                address: inv.host.clone(),
                network,
            };
            seen.peers = peers.clone();
            seen.info = Some(info.clone());
            ctx.inbox.push(Event::Welcome { info, peers });
            let _ = ctx.tx.send(Cmd::Welcomed(ctx.epoch));
        }
        ServerMsg::PeerJoined(p) => match seen.info.as_mut() {
            // The host tells you how you are shown.
            Some(info) if p.client_id == info.me => {
                info.name = p.name;
                info.color = p.color;
                ctx.inbox.push(Event::Welcome { info: info.clone(), peers: seen.peers.clone() });
            }
            _ => {
                seen.peers.retain(|q| q.client_id != p.client_id);
                seen.peers.push(p.clone());
                ctx.inbox.push(Event::PeerJoined(p));
            }
        },
        ServerMsg::PeerLeft { client_id } => {
            seen.peers.retain(|q| q.client_id != client_id);
            ctx.inbox.push(Event::PeerLeft(client_id));
        }
        ServerMsg::Presence { client_id, presence } => {
            if seen.info.as_ref().is_some_and(|i| i.me != client_id) {
                ctx.inbox.presence(client_id, presence);
            }
        }
        ServerMsg::Refused { reason } => return Some(Outcome::Refused(reason)),
        // Document changes: applied once document sync lands; keep-alive answers need nothing.
        ServerMsg::Changes { .. } | ServerMsg::Reject { .. } | ServerMsg::Pong => {}
    }
    None
}
