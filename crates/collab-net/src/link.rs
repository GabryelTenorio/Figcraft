//! [`NetLink`]: the engine's [`LiveLink`] over the network.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use figcraft_collab::message::{Access, ClientMsg, LibraryReply, LibraryReq, PluginReply, PluginReq};
use figcraft_engine::live::{Bind, Event, HostOptions, LiveLink, PageInfo, Presence, PublishOptions, Published, Role, SessionInfo};

use crate::guest::{Guest, GuestConfig, Publish, upload_file};
use crate::host::{Host, HostConfig};
use crate::inbox::{Inbox, Waker};
use crate::invite::{Endpoint, Invite};
use crate::manage;
use crate::net::NetLimits;
use crate::token;

/// The port a hosted session tries first.
pub const DEFAULT_PORT: u16 = 47900;

/// How long a request to a server (outside a session) waits for its connection.
const CONNECT: Duration = Duration::from_secs(5);

enum Active {
    Host(Host),
    Guest(Guest),
}

/// Hosts or joins one session at a time.
pub struct NetLink {
    wake: Waker,
    limits: NetLimits,
    active: Option<Active>,
    inbox: Option<Inbox>,
    /// Tweaks for joining (tests shorten the reconnect pauses).
    guest: fn(GuestConfig) -> GuestConfig,
    /// Answers to library requests sent in the background (outlives sessions).
    libraries: Inbox,
    /// Background library requests are out.
    fetching: Arc<AtomicBool>,
}

impl NetLink {
    /// `wake` is called (from network threads) whenever something arrives for the UI.
    pub fn new(wake: Waker) -> Self {
        let libraries = Inbox::new(wake.clone());
        Self { wake, limits: NetLimits::default(), active: None, inbox: None, guest: |c| c, libraries, fetching: Arc::default() }
    }

    /// With other transport limits.
    pub fn with_limits(mut self, limits: NetLimits) -> Self {
        self.limits = limits;
        self
    }

    /// With joins configured by `f` (reconnect pauses, attempts).
    pub fn with_guest_config(mut self, f: fn(GuestConfig) -> GuestConfig) -> Self {
        self.guest = f;
        self
    }

    /// How this link joins `invite`.
    fn guest_config(&self, invite: Invite, name: &str) -> GuestConfig {
        let mut cfg = (self.guest)(GuestConfig::new(invite, name.to_string()));
        cfg.limits = self.limits;
        cfg
    }

    /// The running host, if hosting.
    pub fn host_handle(&self) -> Option<&Host> {
        match &self.active {
            Some(Active::Host(h)) => Some(h),
            _ => None,
        }
    }
}

/// This machine's address on the local network: the interface the default route leaves by (no
/// packet is sent).
pub fn lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).ok()?;
    // Documentation and private ranges: only the route matters.
    for probe in ["192.0.2.1:9", "10.255.255.255:9"] {
        if sock.connect(probe).is_ok()
            && let Ok(a) = sock.local_addr()
            && !a.ip().is_loopback()
            && !a.ip().is_unspecified()
        {
            return Some(a.ip());
        }
    }
    None
}

impl LiveLink for NetLink {
    fn host(&mut self, o: HostOptions) -> Result<SessionInfo, String> {
        self.leave();
        let ip = match o.bind {
            Bind::Local => IpAddr::V4(Ipv4Addr::LOCALHOST),
            Bind::Lan => lan_ip().ok_or("this machine has no local network address (connect to a network, or share on this machine only)")?,
            Bind::Addr(ip) => ip,
        };
        let (file, token) = (token::file_id()?, token::secret()?);
        let inbox = Inbox::new(self.wake.clone());
        let doc = (*o.doc).clone();
        let pages = doc.pages.iter().map(|p| PageInfo { id: p.0, name: doc.node(*p).map(|n| n.name.clone()).unwrap_or_default() }).collect();
        let file_name = doc.name.clone();
        let cfg = HostConfig {
            addr: SocketAddr::new(ip, o.port.unwrap_or(DEFAULT_PORT)),
            any_port_if_taken: o.port.is_none(),
            file: file.clone(),
            token: token.clone(),
            doc,
            name: o.name,
            limits: self.limits,
            core: figcraft_collab::Limits::default(),
        };
        let host = Host::start(cfg, inbox.clone()).map_err(|e| format!("can't listen on {ip}: {e}"))?;
        let invite = Invite::new(host.addr().to_string(), file, token);
        let info = SessionInfo {
            role: Role::Host,
            me: host.me().client_id,
            name: host.me().name.clone(),
            color: host.me().color.clone(),
            file_name,
            pages,
            link: invite.link(),
            url: invite.url(),
            address: host.addr().to_string(),
            network: !ip.is_loopback(),
            access: Access::Edit,
        };
        self.inbox = Some(inbox);
        self.active = Some(Active::Host(host));
        Ok(info)
    }

    fn join(&mut self, link: &str, name: &str) -> Result<(), String> {
        let invite = Invite::parse(link)?;
        let cfg = self.guest_config(invite, name);
        self.leave();
        let inbox = Inbox::new(self.wake.clone());
        let guest = Guest::start(cfg, inbox.clone()).map_err(|e| format!("can't start the connection: {e}"))?;
        self.inbox = Some(inbox);
        self.active = Some(Active::Guest(guest));
        Ok(())
    }

    fn publish(&mut self, o: PublishOptions) -> Result<(), String> {
        let (server, query) = Endpoint::parse(&o.server)?;
        let key = o.key.or_else(|| query.into_iter().find(|(k, _)| k == "key").map(|(_, v)| v)).filter(|k| !k.trim().is_empty());
        let key = key.ok_or("the server address needs the server's key (ws://host:port/?key=…; `figcraft-server key` prints it)")?;
        // Until the upload is done the invite names only the server.
        let cfg = self.guest_config(Invite { server: server.clone(), file: String::new(), token: String::new() }, &o.me);
        self.leave();
        let inbox = Inbox::new(self.wake.clone());
        let job = Publish { server, key, name: o.name, doc: o.doc };
        let guest = Guest::publish(job, cfg, inbox.clone()).map_err(|e| format!("can't start the connection: {e}"))?;
        self.inbox = Some(inbox);
        self.active = Some(Active::Guest(guest));
        Ok(())
    }

    fn upload(&mut self, o: PublishOptions) -> Result<Published, String> {
        let (server, query) = Endpoint::parse(&o.server)?;
        let key = o.key.or_else(|| query.into_iter().find(|(k, _)| k == "key").map(|(_, v)| v)).filter(|k| !k.trim().is_empty());
        let key = key.ok_or("the server address needs the server's key (ws://host:port/?key=…; `figcraft-server key` prints it)")?;
        upload_file(&server, &key, &o.name, &o.doc, &self.limits, CONNECT).map(|(_, published)| published)
    }

    fn library(&mut self, link: &str, req: LibraryReq) -> Result<LibraryReply, String> {
        manage::library(link, req, &self.limits, CONNECT)
    }

    fn plugins(&mut self, server: &str, req: PluginReq) -> Result<PluginReply, String> {
        let (server, query) = Endpoint::parse(server)?;
        let key = query.into_iter().find(|(k, _)| k == "key").map(|(_, v)| v).unwrap_or_default();
        manage::plugins(&server, key.trim(), req, &self.limits, CONNECT)
    }

    fn library_background(&mut self, jobs: Vec<(String, LibraryReq)>) {
        if jobs.is_empty() || self.fetching.swap(true, Ordering::SeqCst) {
            return;
        }
        let (inbox, fetching, limits) = (self.libraries.clone(), self.fetching.clone(), self.limits);
        let spawned = std::thread::Builder::new().name("library-fetch".into()).spawn(move || {
            for (link, req) in jobs {
                let reply = manage::library(&link, req, &limits, CONNECT);
                inbox.push(Event::Library { link, reply });
            }
            fetching.store(false, Ordering::SeqCst);
        });
        if let Err(e) = spawned {
            log::warn!("team libraries: can't start a thread: {e}");
            self.fetching.store(false, Ordering::SeqCst);
        }
    }

    fn leave(&mut self) {
        match self.active.take() {
            Some(Active::Host(mut h)) => h.stop(),
            Some(Active::Guest(mut g)) => g.leave(),
            None => {}
        }
        self.inbox = None;
    }

    fn presence(&mut self, p: &Presence) {
        match &self.active {
            Some(Active::Host(h)) => h.presence(p.clone()),
            Some(Active::Guest(g)) => g.presence(p.clone()),
            None => {}
        }
    }

    fn send(&mut self, msg: ClientMsg) {
        match &self.active {
            Some(Active::Host(h)) => h.send(msg),
            Some(Active::Guest(g)) => g.send(msg),
            None => {}
        }
    }

    fn poll(&mut self) -> Vec<Event> {
        let mut out = self.libraries.drain();
        out.extend(self.inbox.as_ref().map(Inbox::drain).unwrap_or_default());
        out
    }
}

impl Drop for NetLink {
    fn drop(&mut self) {
        self.leave();
    }
}
