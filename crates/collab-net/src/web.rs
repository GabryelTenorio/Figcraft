//! The browser build's stand-in: live sessions need the desktop app until the `web-sys`
//! WebSocket client lands (step 4 of `docs/collab-protocol.md`).

use figcraft_engine::live::{Event, HostOptions, LiveLink, Presence, SessionInfo};

use crate::inbox::Waker;

pub struct NetLink;

impl NetLink {
    pub fn new(_wake: Waker) -> Self {
        NetLink
    }
}

impl LiveLink for NetLink {
    fn host(&mut self, _: HostOptions) -> Result<SessionInfo, String> {
        Err("hosting a live session needs the desktop app".into())
    }
    fn join(&mut self, link: &str, _: &str) -> Result<(), String> {
        crate::invite::Invite::parse(link)?;
        Err("joining live sessions from the browser comes in a later version; use the desktop app".into())
    }
    fn leave(&mut self) {}
    fn presence(&mut self, _: &Presence) {}
    fn send(&mut self, _: figcraft_collab::message::ClientMsg) {}
    fn poll(&mut self) -> Vec<Event> {
        Vec::new()
    }
}
