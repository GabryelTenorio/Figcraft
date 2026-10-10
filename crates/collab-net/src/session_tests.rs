//! Two engine sessions in one process, linked over loopback through `collab.*` commands: hosting,
//! joining with the invite, presence in each other's live state, following, leaving.

use std::sync::Arc;
use std::time::{Duration, Instant};

use figcraft_doc::NodeId;
use figcraft_engine::Session;
use figcraft_engine::live::Status;
use serde_json::{Value, json};

use crate::NetLink;
use crate::loopback_tests::quick;

fn session(name: &str) -> Session {
    let mut s = Session::new();
    s.live.install(Box::new(NetLink::new(Arc::new(|| {})).with_guest_config(quick)));
    s.execute("pref.userName", &json!({"name": name})).unwrap();
    s
}

/// Pump both sessions (as their UIs would every frame) until `f` holds.
fn pump(a: &mut Session, b: &mut Session, what: &str, f: impl Fn(&Session, &Session) -> bool) {
    let start = Instant::now();
    loop {
        a.live_poll();
        b.live_poll();
        a.live_sync();
        b.live_sync();
        if f(a, b) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "timed out waiting for {what}: {} / {}", a.live_summary(), b.live_summary());
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn host_and_join() -> (Session, Session, u32, u32) {
    let mut host = session("Hana");
    let mut guest = session("Ada");
    let r = host.execute("collab.host", &json!({"port": 0})).unwrap();
    assert_eq!(r["status"], "live");
    assert_eq!(r["role"], "host");
    assert_eq!(r["network"], false, "shared on this machine only by default");
    let link = r["link"].as_str().unwrap().to_string();
    assert!(link.starts_with("figcraft://join?host=127.0.0.1:"), "{link}");
    let r = guest.execute("collab.join", &json!({"link": link})).unwrap();
    assert_eq!(r["status"], "connecting");
    pump(&mut host, &mut guest, "the join", |h, g| {
        g.live.status == Status::Live && h.live.peers.len() == 1 && g.live.info.as_ref().is_some_and(|i| !i.color.is_empty())
    });
    let hana = host.live.info.as_ref().unwrap().me;
    let ada = guest.live.info.as_ref().unwrap().me;
    (host, guest, hana, ada)
}

#[test]
fn host_join_and_see_each_other() {
    let (mut host, mut guest, hana, ada) = host_and_join();
    let g = guest.live_summary();
    assert_eq!(g["role"], "guest");
    assert_eq!(g["fileName"], "Untitled", "the host's file name");
    assert_eq!(g["name"], "Ada");
    assert_eq!(g["peers"][0]["name"], "Hana");
    assert_eq!(host.live.peers[&ada].name, "Ada");
    // The pointer: the guest's cursor shows up in the host's state.
    guest.execute("collab.cursor", &json!({"x": 120, "y": 80})).unwrap();
    pump(&mut host, &mut guest, "the guest's cursor", |h, _| {
        h.live.presence.get(&ada).and_then(|p| p.cursor).is_some_and(|c| c.1 == 120.0 && c.2 == 80.0)
    });
    // A selection travels too.
    let r = host.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 50, "height": 50})).unwrap();
    let id = r["id"].as_u64().unwrap();
    host.execute("select.set", &json!({"ids": [id]})).unwrap();
    pump(&mut host, &mut guest, "the host's selection", |_, g| {
        g.live.presence.get(&hana).and_then(|p| p.selection.clone()).is_some_and(|s| s == vec![NodeId(id)])
    });
    // Leaving: the host sees it, the guest is back to no session.
    assert_eq!(guest.execute("collab.leave", &json!({})).unwrap()["ended"], true);
    assert_eq!(guest.live.status, Status::Off);
    pump(&mut host, &mut guest, "the leave", |h, _| h.live.peers.is_empty() && h.live.presence.is_empty());
    host.execute("collab.stop", &json!({})).unwrap();
    assert_eq!(host.live_summary()["status"], "off");
}

#[test]
fn following_a_viewport_until_you_move() {
    let (mut host, mut guest, hana, ada) = host_and_join();
    host.execute("collab.follow", &json!({"client": ada})).unwrap();
    assert_eq!(host.live.follow, Some(ada));
    // The guest pans and zooms: the host's view goes along.
    {
        let ds = guest.ds_mut().unwrap();
        ds.view.cx = 5000.0;
        ds.view.cy = -300.0;
        ds.view.zoom = 2.0;
    }
    pump(&mut host, &mut guest, "the follow", |h, _| {
        (h.ds().unwrap().view.cx - 5000.0).abs() < 1e-6 && (h.ds().unwrap().view.cy + 300.0).abs() < 1e-6
    });
    let (hv, gv) = (host.ds().unwrap().view.clone(), guest.ds().unwrap().view.clone());
    assert!((hv.zoom - gv.zoom).abs() < 1e-9, "same size viewports, same zoom");
    // The host pans: following stops, and later moves of the guest don't move the host.
    host.ds_mut().unwrap().view.cx += 100.0;
    host.live_sync();
    assert_eq!(host.live.follow, None);
    guest.ds_mut().unwrap().view.cx = -9000.0;
    pump(&mut host, &mut guest, "the guest's new viewport at the host", |h, _| {
        h.live.presence.get(&ada).and_then(|p| p.viewport).is_some_and(|v| (v.1 + v.3 / 2.0 + 9000.0).abs() < 1e-6)
    });
    assert!((host.ds().unwrap().view.cx - 5100.0).abs() < 1e-6);
    // Escape stops following too; following someone who isn't there fails.
    guest.execute("collab.follow", &json!({"client": hana})).unwrap();
    guest.escape().unwrap();
    assert_eq!(guest.live.follow, None);
    assert!(guest.execute("collab.follow", &json!({"client": 999})).is_err());
    // The followed peer leaving ends the follow.
    host.execute("collab.follow", &json!({"client": ada})).unwrap();
    guest.execute("collab.leave", &json!({})).unwrap();
    pump(&mut host, &mut guest, "the leave", |h, _| h.live.follow.is_none() && h.live.peers.is_empty());
}

#[test]
fn the_host_stopping_ends_the_session_for_guests() {
    let (mut host, mut guest, _, _) = host_and_join();
    host.execute("collab.stop", &json!({})).unwrap();
    pump(&mut host, &mut guest, "the end", |_, g| g.live.status == Status::Off);
    assert!(guest.live.peers.is_empty());
    assert_eq!(guest.message.as_deref(), Some(crate::host::ENDED));
}

#[test]
fn a_bad_invite_is_reported() {
    let mut guest = session("Ada");
    let e = guest.execute("collab.join", &json!({"link": "figcraft://join?host=nope"})).unwrap_err().to_string();
    assert!(e.contains("invite"), "{e}");
    let mut host = session("Hana");
    let r: Value = host.execute("collab.host", &json!({"port": 0})).unwrap();
    let link = r["link"].as_str().unwrap().replace("token=", "token=x");
    guest.execute("collab.join", &json!({"link": link})).unwrap();
    pump(&mut host, &mut guest, "the refusal", |_, g| matches!(&g.live.status, Status::Failed { reason } if reason.contains("token")));
    assert!(host.live.peers.is_empty());
}
