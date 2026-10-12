//! Recovery of your online files: a document that was one of them, in step with its server, when
//! a run ended without closing opens again from the server as its owner (`collab.openFile`), not
//! as a stale unsaved copy; the note beside its copy names the server and file and never a token;
//! and without the record the copy opens as any other.

use std::sync::{Arc, Mutex};

use figcraft_engine::Session;
use figcraft_engine::figcraft_collab::message::{ClientMsg, ServerMsg};
use figcraft_engine::figcraft_collab::{Client, Limits, Outgoing, Server};
use figcraft_engine::live::{Access, Event, HostOptions, LiveLink, Presence, PublishOptions, Published, Role, SessionInfo};
use serde_json::json;

use crate::recovery::{Recovery, recover};

const SERVER: &str = "wss://share.example/";
const OWNER: &str = "OwnerSecretOwnerSecret";
const EDIT: &str = "figcraft://join?server=wss://share.example/&file=f1&token=EditSecretEditSecret00";

/// The joins asked for, and the events handed out next (as the session started last).
#[derive(Default)]
struct Script {
    joined: Vec<String>,
    events: Vec<Event>,
    session: u64,
}

struct Link(Arc<Mutex<Script>>);

impl LiveLink for Link {
    fn host(&mut self, _: u64, _: HostOptions) -> Result<SessionInfo, String> {
        Err("not in this test".into())
    }
    fn join(&mut self, session: u64, link: &str, _: &str) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        s.session = session;
        s.joined.push(link.to_string());
        Ok(())
    }
    fn publish(&mut self, session: u64, _: PublishOptions) -> Result<(), String> {
        self.0.lock().unwrap().session = session;
        Ok(())
    }
    fn leave(&mut self, _: u64) {}
    fn presence(&mut self, _: u64, _: &Presence) {}
    fn send(&mut self, _: u64, _: ClientMsg) {}
    fn poll(&mut self) -> Vec<(u64, Event)> {
        let mut s = self.0.lock().unwrap();
        let session = s.session;
        std::mem::take(&mut s.events).into_iter().map(|e| (session, e)).collect()
    }
}

fn session() -> (Session, Arc<Mutex<Script>>) {
    let mut s = Session::new();
    let script = Arc::new(Mutex::new(Script::default()));
    s.live.install(Box::new(Link(script.clone())));
    (s, script)
}

fn root(name: &str) -> std::path::PathBuf {
    let r = std::env::temp_dir().join(format!("figcraft-recovery-online-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    r
}

/// A run whose open file was shared online and is in step with the server, then edited.
fn shared_and_edited() -> Session {
    let (mut s, script) = session();
    s.execute("pref.publishServer", &json!({"url": SERVER})).unwrap();
    s.execute("collab.share", &json!({})).unwrap();
    let published = Published {
        server: "share.example".into(),
        file: "f1".into(),
        name: "Homepage".into(),
        edit: EDIT.into(),
        view: String::new(),
        owner: Some(OWNER.into()),
        expires: None,
    };
    let info = SessionInfo {
        role: Role::Guest,
        me: 1,
        name: "Me".into(),
        color: "#0D99FF".into(),
        file_name: "Homepage".into(),
        pages: vec![],
        link: EDIT.into(),
        url: String::new(),
        address: SERVER.into(),
        network: true,
        access: Access::Edit,
    };
    let mut server = Server::new(s.doc().unwrap().clone(), "", Limits::default());
    let ClientMsg::Hello(me) = Client::hello("f1", "", "Me", "") else { panic!("no hello") };
    let (_, outs) = server.join(me).unwrap();
    let welcome = outs.into_iter().find_map(|o| if let Outgoing::To(_, w @ ServerMsg::Welcome { .. }) = o { Some(w) } else { None }).unwrap();
    script.lock().unwrap().events.extend([Event::Published(published), Event::Welcome { info, peers: vec![] }, Event::Sync(welcome)]);
    s.live_poll();
    s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    assert!(s.docs[0].is_dirty() && s.online_of(s.docs[0].uid).is_some());
    s
}

#[test]
fn an_online_file_comes_back_from_its_server_not_from_the_stale_copy() {
    let root = root("reopen");
    let crashed = shared_and_edited();
    let mut r = Recovery::start(&root).unwrap();
    r.write(&crashed, true);
    let dir = std::fs::read_dir(&root).unwrap().flatten().next().unwrap().path();
    let note = std::fs::read_to_string(dir.join(format!("{}.online", crashed.docs[0].uid))).unwrap();
    assert_eq!(note, format!("{SERVER}\nf1"), "which file it is");
    assert!(!note.contains(OWNER) && !note.contains("token="), "never a secret");
    // The run ends without closing; the next one has the preferences (with the record).
    drop(r);
    let (mut next, script) = session();
    next.docs.clear();
    next.prefs = serde_json::from_value(serde_json::to_value(&crashed.prefs).unwrap()).unwrap();
    assert_eq!(recover(&root, &mut next), 0, "nothing comes back as an unsaved copy");
    assert_eq!(script.lock().unwrap().joined, [EDIT], "it is joined again as its owner");
    assert!(next.docs.is_empty(), "its tab opens when the server's copy arrives");
    assert!(std::fs::read_dir(&root).unwrap().next().is_none(), "the folder is gone, note and all");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_its_record_the_copy_opens_as_any_other() {
    let root = root("fallback");
    let crashed = shared_and_edited();
    let mut r = Recovery::start(&root).unwrap();
    r.write(&crashed, true);
    drop(r);
    // The record was forgotten (or the preferences lost): the copy is what there is.
    let (mut next, script) = session();
    next.docs.clear();
    assert_eq!(recover(&root, &mut next), 1);
    assert!(script.lock().unwrap().joined.is_empty());
    assert!(next.docs.first().is_some_and(|d| d.is_dirty() && d.path.is_none()));
    assert!(std::fs::read_dir(&root).unwrap().next().is_none(), "the folder is gone");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_note_goes_when_the_file_leaves_its_session_or_is_closed() {
    let root = root("note");
    let mut s = shared_and_edited();
    let mut r = Recovery::start(&root).unwrap();
    r.write(&s, true);
    let dir = std::fs::read_dir(&root).unwrap().flatten().next().unwrap().path();
    let notes = |d: &std::path::Path| std::fs::read_dir(d).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|e| e == "online")).count();
    assert_eq!(notes(&dir), 1);
    // Left: the copy is an ordinary one from the next write on.
    s.execute("collab.leave", &json!({})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 20, "y": 0, "width": 10, "height": 10})).unwrap();
    r.write(&s, true);
    assert_eq!(notes(&dir), 0);
    r.finish();
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    let _ = std::fs::remove_dir_all(&root);
}
