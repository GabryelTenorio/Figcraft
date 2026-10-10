//! Live session state and the `collab.*` commands against a scripted link (the real one, over
//! the network, is tested in `figcraft-collab-net`).

use std::sync::{Arc, Mutex};

use figcraft_doc::NodeId;
use serde_json::json;

use crate::live::{Event, HostOptions, LiveLink, Peer, Presence, Role, SessionInfo, Status};
use crate::{EngineError, Session};

/// What the scripted link was asked to do, and the events it hands out next.
#[derive(Default)]
struct Script {
    events: Vec<Event>,
    sent: Vec<Presence>,
    hosted: Vec<HostOptions>,
    joined: Vec<String>,
    left: usize,
}

struct Mock(Arc<Mutex<Script>>);

impl LiveLink for Mock {
    fn host(&mut self, opts: HostOptions) -> Result<SessionInfo, String> {
        self.0.lock().unwrap().hosted.push(opts);
        Ok(info(Role::Host, 1))
    }
    fn join(&mut self, link: &str, _: &str) -> Result<(), String> {
        if !link.starts_with("figcraft://") {
            return Err("not an invite".into());
        }
        self.0.lock().unwrap().joined.push(link.to_string());
        Ok(())
    }
    fn leave(&mut self) {
        self.0.lock().unwrap().left += 1;
    }
    fn presence(&mut self, p: &Presence) {
        self.0.lock().unwrap().sent.push(p.clone());
    }
    fn poll(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.0.lock().unwrap().events)
    }
}

fn info(role: Role, me: u32) -> SessionInfo {
    SessionInfo {
        role,
        me,
        name: "Me".into(),
        color: "#0D99FF".into(),
        file_name: "Shared".into(),
        pages: vec![],
        link: "figcraft://join?host=127.0.0.1:47900&file=f&token=t".into(),
        url: "ws://127.0.0.1:47900/?file=f&token=t".into(),
        address: "127.0.0.1:47900".into(),
        network: false,
    }
}

fn peer(id: u32, name: &str) -> Peer {
    Peer { client_id: id, name: name.into(), color: "#9747FF".into() }
}

fn session() -> (Session, Arc<Mutex<Script>>) {
    let mut s = Session::new();
    let script = Arc::new(Mutex::new(Script::default()));
    s.live.install(Box::new(Mock(script.clone())));
    (s, script)
}

fn push(script: &Arc<Mutex<Script>>, events: Vec<Event>) {
    script.lock().unwrap().events.extend(events);
}

fn viewport(page: NodeId, x: f64, y: f64, w: f64, h: f64) -> Presence {
    Presence { viewport: Some((page, x, y, w, h)), ..Presence::default() }
}

#[test]
fn without_a_link_live_sessions_are_unavailable() {
    let mut s = Session::new();
    for cmd in ["collab.host", "collab.join"] {
        let e = s.execute(cmd, &json!({"link": "figcraft://join?host=a:1&file=f&token=t"})).unwrap_err();
        assert!(matches!(e, EngineError::Disabled(..)), "{cmd}: {e}");
    }
    assert_eq!(s.execute("collab.leave", &json!({})).unwrap()["ended"], false);
    let p = s.execute("collab.peers", &json!({})).unwrap();
    assert_eq!(p["available"], false);
    assert_eq!(p["status"], "off");
}

#[test]
fn hosting_and_joining_drive_the_link() {
    let (mut s, script) = session();
    let r = s.execute("collab.host", &json!({"bind": "lan", "port": 50000})).unwrap();
    assert_eq!(r["status"], "live");
    assert_eq!(r["role"], "host");
    assert_eq!(r["link"], "figcraft://join?host=127.0.0.1:47900&file=f&token=t");
    {
        let sc = script.lock().unwrap();
        assert_eq!(sc.hosted.len(), 1);
        assert_eq!(sc.hosted[0].port, Some(50000));
        assert_eq!(sc.hosted[0].bind, crate::live::Bind::Lan);
        assert_eq!(sc.hosted[0].doc.name, "Untitled");
        assert_eq!(sc.hosted[0].name, "", "the default name lets the host pick one");
    }
    for bad in [json!({"bind": "everywhere"}), json!({"port": 70000}), json!({"port": -1}), json!({"port": "x"})] {
        assert!(matches!(s.execute("collab.host", &bad), Err(EngineError::BadParams { .. })), "{bad}");
    }
    assert!(s.execute("collab.host", &json!({"bind": "10.1.2.3"})).is_ok());
    assert!(matches!(s.execute("collab.join", &json!({"link": "http://x"})), Err(EngineError::BadParams { .. })));
    let r = s.execute("collab.join", &json!({"link": " figcraft://join?host=a:1&file=f&token=t "})).unwrap();
    assert_eq!(r["status"], "connecting");
    assert_eq!(script.lock().unwrap().joined, ["figcraft://join?host=a:1&file=f&token=t"]);
    assert_eq!(s.execute("collab.stop", &json!({})).unwrap()["ended"], true);
    assert!(script.lock().unwrap().left >= 1);
    assert_eq!(s.live.status, Status::Off);
}

#[test]
fn events_build_who_is_here() {
    let (mut s, script) = session();
    s.execute("collab.join", &json!({"link": "figcraft://join?host=a:1&file=f&token=t"})).unwrap();
    push(&script, vec![Event::Welcome { info: info(Role::Guest, 2), peers: vec![peer(1, "Hana")] }, Event::PeerJoined(peer(3, "Bo"))]);
    assert!(s.live_poll());
    assert_eq!(s.live.status, Status::Live);
    assert_eq!(s.live.peers.len(), 2);
    assert_eq!(s.message.as_deref(), Some("Bo joined"));
    // Presence of someone not in the session is ignored; a leaver's presence goes with them.
    push(&script, vec![Event::Presence(9, viewport(NodeId(1), 0.0, 0.0, 10.0, 10.0)), Event::Presence(3, viewport(NodeId(1), 0.0, 0.0, 10.0, 10.0))]);
    s.live_poll();
    assert_eq!(s.live.presence.keys().copied().collect::<Vec<_>>(), [3]);
    push(&script, vec![Event::PeerLeft(3)]);
    s.live_poll();
    assert!(s.live.presence.is_empty());
    assert_eq!(s.message.as_deref(), Some("Bo left"));
    let p = s.live_summary();
    assert_eq!(p["peers"][0]["name"], "Hana");
    assert_eq!(p["fileName"], "Shared");
    // Reconnecting forgets who's here until the next welcome; failing ends it all.
    push(&script, vec![Event::Status(Status::Reconnecting { attempt: 1 })]);
    s.live_poll();
    assert!(s.live.peers.is_empty());
    assert_eq!(s.live_summary()["attempt"], 1);
    push(&script, vec![Event::Status(Status::Failed { reason: "Lost the connection".into() })]);
    s.live_poll();
    assert!(s.live.info.is_none());
    assert_eq!(s.live_summary()["reason"], "Lost the connection");
    assert_eq!(s.message.as_deref(), Some("Lost the connection"));
    assert!(!s.live_poll(), "nothing new");
}

#[test]
fn presence_is_sent_when_it_changes_and_only_when_live() {
    let (mut s, script) = session();
    s.live.cursor = Some((10.0, 20.0));
    s.live_sync();
    assert!(script.lock().unwrap().sent.is_empty(), "not in a session");
    s.execute("collab.host", &json!({})).unwrap();
    s.live_sync();
    s.live_sync();
    let sent = script.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), 1, "unchanged presence isn't sent again");
    let page = s.page().unwrap();
    assert_eq!(sent[0].cursor, Some((page, 10.0, 20.0)));
    assert_eq!(sent[0].selection, Some(vec![]));
    let v = sent[0].viewport.unwrap();
    assert_eq!(v.0, page);
    assert!((v.3 - 1200.0).abs() < 1e-9 && (v.4 - 800.0).abs() < 1e-9, "the viewport at 100%: {v:?}");
    // `collab.cursor` moves it (agents); a non-finite pointer never goes out.
    s.execute("collab.cursor", &json!({"x": 1, "y": 2})).unwrap();
    assert_eq!(script.lock().unwrap().sent.last().unwrap().cursor, Some((page, 1.0, 2.0)));
    assert!(s.execute("collab.cursor", &json!({"x": 1})).is_err());
    s.live.cursor = Some((f64::NAN, 0.0));
    s.live_sync();
    assert_eq!(script.lock().unwrap().sent.last().unwrap().cursor, None);
    s.execute("collab.cursor", &json!({})).unwrap();
    assert_eq!(s.live.cursor, None);
}

#[test]
fn following_shows_their_viewport_and_page() {
    let (mut s, script) = session();
    s.execute("collab.host", &json!({})).unwrap();
    let second = s.execute("page.add", &json!({"name": "Two"})).unwrap()["id"].as_u64().map(NodeId).unwrap();
    let first = s.doc().unwrap().pages[0];
    s.execute("page.select", &json!({"id": first.0})).unwrap();
    push(&script, vec![Event::PeerJoined(peer(5, "Ada")), Event::Presence(5, viewport(second, 100.0, 200.0, 600.0, 400.0))]);
    s.live_poll();
    assert!(s.execute("collab.follow", &json!({"client": 6})).is_err());
    // Documents not synced yet: their page ids aren't this document's, only the viewport counts.
    s.execute("collab.follow", &json!({"client": 5})).unwrap();
    assert_eq!(s.ds().unwrap().page, first);
    assert_eq!(s.ds().unwrap().view.cx, 400.0);
    s.live.shared_document = true;
    s.execute("collab.follow", &json!({"client": 5})).unwrap();
    let ds = s.ds().unwrap();
    assert_eq!(ds.page, second, "their page, in a shared document");
    assert_eq!((ds.view.cx, ds.view.cy), (400.0, 400.0));
    assert!((ds.view.zoom - 2.0).abs() < 1e-9, "their area fills the viewport");
    // A page this document doesn't have: only the viewport.
    push(&script, vec![Event::Presence(5, viewport(NodeId(987_654), 0.0, 0.0, 1200.0, 800.0))]);
    s.live_poll();
    assert_eq!(s.ds().unwrap().page, second);
    assert_eq!(s.ds().unwrap().view.cx, 600.0);
    // Zooming yourself stops following.
    s.ds_mut().unwrap().view.zoom = 3.0;
    s.live_sync();
    assert_eq!(s.live.follow, None);
    // So does Escape, before anything else it does.
    s.execute("collab.follow", &json!({"client": 5})).unwrap();
    s.execute("select.all", &json!({})).ok();
    s.escape().unwrap();
    assert_eq!(s.live.follow, None);
    // And `collab.follow {}`.
    s.execute("collab.follow", &json!({"client": 5})).unwrap();
    assert_eq!(s.execute("collab.follow", &json!({})).unwrap()["follow"], serde_json::Value::Null);
}

#[test]
fn the_host_ending_resets_everything() {
    let (mut s, script) = session();
    s.execute("collab.join", &json!({"link": "figcraft://join?host=a:1&file=f&token=t"})).unwrap();
    push(&script, vec![Event::Welcome { info: info(Role::Guest, 2), peers: vec![peer(1, "Hana")] }]);
    s.live_poll();
    s.execute("collab.follow", &json!({"client": 1})).unwrap();
    push(&script, vec![Event::Ended { reason: "The host ended the session".into() }]);
    s.live_poll();
    assert_eq!(s.live.status, Status::Off);
    assert!(s.live.peers.is_empty() && s.live.follow.is_none() && s.live.info.is_none());
    assert_eq!(s.message.as_deref(), Some("The host ended the session"));
}

#[test]
fn collab_commands_survive_junk() {
    let junk = [
        json!(null),
        json!([1, 2]),
        json!({"port": 1e300, "bind": 5, "link": 7, "client": -3, "x": "NaN", "y": {}}),
        json!({"link": "", "client": 4_294_967_296u64}),
    ];
    for cmd in crate::all_commands().iter().filter(|c| c.id.starts_with("collab.")) {
        for j in &junk {
            let (mut s, _) = session();
            if let Err(EngineError::Internal { msg, .. }) = s.execute(cmd.id, j) {
                panic!("{} panicked on {j}: {msg}", cmd.id);
            }
        }
    }
}
