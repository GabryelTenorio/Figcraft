//! The browser client (step 4 of `docs/collab-protocol.md`): the web build joins a session a
//! desktop app hosts, or a file on a `figcraft-server` (also over `wss://` behind a TLS reverse
//! proxy), through the browser's WebSocket API (`web-sys`), with the same messages as the native
//! guest. The connection logic is the [`Driver`]'s; this file feeds it the socket's
//! events and wakes it with a timer. Hosting needs the desktop app (a page can't listen on a
//! socket).
//!
//! The page runs everything on one thread: each session's socket, callbacks and timer live in a
//! thread-local table, and [`NetLink`] (the engine needs it to be `Send`) holds only the session's
//! number and its [`Inbox`]. Whatever arrives wakes egui through the [`Waker`]; nothing polls.
//!
//! An `https://` page can't open an unencrypted `ws://` connection (browsers block mixed content)
//! except to this machine; [`NetLink::join`] says so up front.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use figcraft_collab::message::{ClientMsg, Presence};
use figcraft_engine::live::{Event, HostOptions, LiveLink, SessionInfo};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, MessageEvent, WebSocket};
use web_time::Instant;

use crate::driver::{Driver, Socket};
use crate::inbox::{Inbox, Waker};
use crate::invite::Invite;
use crate::join::GuestConfig;

/// Close code for leaving normally.
const CLOSE_NORMAL: u16 = 1000;

/// One session's driver and the timer that wakes it.
struct Guest {
    driver: Driver<BrowserSocket>,
    /// Runs the driver's `tick` (one closure for the session's life, reused for every wake).
    timer: Closure<dyn FnMut()>,
    /// The pending `setTimeout`.
    timeout: Option<i32>,
}

impl Guest {
    /// Set the timer for the driver's next deadline (replacing the last one).
    fn arm(&mut self) {
        let Some(w) = web_sys::window() else { return };
        if let Some(t) = self.timeout.take() {
            w.clear_timeout_with_handle(t);
        }
        let Some(at) = self.driver.deadline() else { return };
        let ms = i32::try_from(at.saturating_duration_since(Instant::now()).as_millis()).unwrap_or(i32::MAX);
        self.timeout = w.set_timeout_with_callback_and_timeout_and_arguments_0(self.timer.as_ref().unchecked_ref(), ms).ok();
    }
}

thread_local! {
    static GUESTS: RefCell<BTreeMap<u64, Rc<RefCell<Guest>>>> = RefCell::default();
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// Run `f` on session `id`'s driver, then set its timer for what's next, or forget the session
/// once it's over. (Callbacks may drop their own closures: wasm-bindgen frees a closure only
/// after it has returned.)
fn with_driver(id: u64, f: impl FnOnce(&mut Driver<BrowserSocket>, Instant)) {
    let Some(g) = GUESTS.with(|t| t.borrow().get(&id).cloned()) else { return };
    let done = {
        let Ok(mut g) = g.try_borrow_mut() else {
            log::warn!("live session: an event arrived while the session was busy");
            return;
        };
        f(&mut g.driver, Instant::now());
        g.arm();
        g.driver.done()
    };
    if done {
        GUESTS.with(|t| t.borrow_mut().remove(&id));
    }
}

/// The socket's callbacks (alive while the connection is in use).
struct Handlers {
    _open: Closure<dyn FnMut()>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _close: Closure<dyn FnMut(CloseEvent)>,
}

/// The browser's WebSocket behind the driver's [`Socket`].
struct BrowserSocket {
    id: u64,
    ws: Option<(WebSocket, Handlers)>,
}

impl Socket for BrowserSocket {
    fn open(&mut self, url: &str) -> Result<(), String> {
        self.close();
        let ws = WebSocket::new(url).map_err(|e| format!("the browser won't open {url}: {}", js_error(&e)))?;
        ws.set_binary_type(BinaryType::Arraybuffer);
        let id = self.id;
        let open = Closure::<dyn FnMut()>::new(move || with_driver(id, |d, now| d.opened(now)));
        let message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| match e.data().dyn_into::<js_sys::ArrayBuffer>() {
            Ok(buf) => {
                let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                with_driver(id, |d, now| d.message(&bytes, now));
            }
            Err(_) => with_driver(id, |d, now| d.text(now)),
        });
        let close = Closure::<dyn FnMut(CloseEvent)>::new(move |e: CloseEvent| {
            let (code, reason) = (e.code(), e.reason());
            with_driver(id, |d, now| d.closed(code, &reason, now));
        });
        ws.set_onopen(Some(open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(close.as_ref().unchecked_ref()));
        self.ws = Some((ws, Handlers { _open: open, _message: message, _close: close }));
        Ok(())
    }

    fn send(&mut self, bytes: &[u8]) -> bool {
        self.ws.as_ref().is_some_and(|(ws, _)| ws.ready_state() == WebSocket::OPEN && ws.send_with_u8_array(bytes).is_ok())
    }

    fn close(&mut self) {
        let Some((ws, _handlers)) = self.ws.take() else { return };
        ws.set_onopen(None);
        ws.set_onmessage(None);
        ws.set_onclose(None);
        let _ = ws.close_with_code(CLOSE_NORMAL);
    }
}

fn js_error(e: &wasm_bindgen::JsValue) -> String {
    e.dyn_ref::<js_sys::Error>().map_or_else(|| format!("{e:?}"), |e| String::from(e.message()))
}

/// The page was loaded over `https://`.
fn secure_page() -> bool {
    web_sys::window().and_then(|w| w.location().protocol().ok()).is_some_and(|p| p == "https:")
}

/// The browser's [`LiveLink`]: joins one session at a time; hosting needs the desktop app.
pub struct NetLink {
    wake: Waker,
    active: Option<u64>,
    inbox: Option<Inbox>,
}

impl NetLink {
    /// `wake` is called whenever something arrives for the UI (egui's `request_repaint`).
    pub fn new(wake: Waker) -> Self {
        Self { wake, active: None, inbox: None }
    }

    fn with(&self, f: impl FnOnce(&mut Driver<BrowserSocket>, Instant)) {
        if let Some(id) = self.active {
            with_driver(id, f);
        }
    }
}

impl LiveLink for NetLink {
    fn can_host(&self) -> bool {
        false
    }

    fn host(&mut self, _: HostOptions) -> Result<SessionInfo, String> {
        Err("hosting a live session needs the desktop app; in the browser you can join one".into())
    }

    fn join(&mut self, link: &str, name: &str) -> Result<(), String> {
        let invite = Invite::parse(link)?;
        if secure_page() && !invite.server.secure && !invite.loopback() {
            return Err("this page was loaded over https, and browsers block unencrypted ws:// connections; use a wss:// link (a server behind TLS), open FigCraft from an http:// address (like http://localhost:8767), or join from the desktop app".into());
        }
        self.leave();
        let id = NEXT_ID.with(|n| n.replace(n.get() + 1));
        let inbox = Inbox::new(self.wake.clone());
        let driver = Driver::start(BrowserSocket { id, ws: None }, GuestConfig::new(invite, name.to_string()), inbox.clone(), Instant::now());
        let timer = Closure::<dyn FnMut()>::new(move || with_driver(id, |d, now| d.tick(now)));
        let mut g = Guest { driver, timer, timeout: None };
        g.arm();
        if !g.driver.done() {
            GUESTS.with(|t| t.borrow_mut().insert(id, Rc::new(RefCell::new(g))));
        }
        self.inbox = Some(inbox);
        self.active = Some(id);
        Ok(())
    }

    fn leave(&mut self) {
        self.with(|d, now| d.leave(now));
        self.active = None;
        self.inbox = None;
    }

    fn presence(&mut self, p: &Presence) {
        self.with(|d, now| d.presence(p.clone(), now));
    }

    fn send(&mut self, msg: ClientMsg) {
        self.with(|d, now| d.send(msg, now));
    }

    fn poll(&mut self) -> Vec<Event> {
        self.inbox.as_ref().map(Inbox::drain).unwrap_or_default()
    }
}

impl Drop for NetLink {
    fn drop(&mut self) {
        self.leave();
    }
}
