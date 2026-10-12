//! Sharing with a join code against a scripted link: `collab.share` online and on the local
//! network, the codes in `collab.peers`, switching what the code lets people do, stopping, and
//! `collab.join` with a code of either kind or a link (what typing tolerates, and what it says
//! when it can't be a code).

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use figcraft_collab::code::{self, Local};
use figcraft_collab::message::{Admin, ClientMsg};
use serde_json::{Value, json};

use crate::live::{
    Access, Bind, DEFAULT_SERVER, Event, HostOptions, LiveLink, NetRequest, Presence, PublishOptions, Published, Role, SessionInfo, Status,
    join_target,
};
use crate::{EngineError, Session};

/// What the link was asked to do, and the events it hands out next (as the session started last).
#[derive(Default)]
struct Script {
    hosted: Vec<HostOptions>,
    published: Vec<PublishOptions>,
    joined: Vec<String>,
    asked: Vec<(u64, NetRequest)>,
    left: Vec<u64>,
    events: Vec<Event>,
    session: u64,
}

struct Link(Arc<Mutex<Script>>);

/// Where the scripted host listens, and its session's secret.
const HOST: Local = Local { ip: Ipv4Addr::new(192, 168, 1, 20), port: 47_900, secret: 0x123_4567_89AB };

impl LiveLink for Link {
    fn host(&mut self, session: u64, opts: HostOptions) -> Result<SessionInfo, String> {
        let mut s = self.0.lock().unwrap();
        s.session = session;
        // As the network link does: a short secret when the session is shared with a code.
        let link = if opts.short { HOST.link() } else { "figcraft://join?host=192.168.1.20:47900&file=f&token=AbCdEfGhIjKlMnOpQrStUv".to_string() };
        s.hosted.push(opts);
        Ok(SessionInfo {
            role: Role::Host,
            me: 1,
            name: "Me".into(),
            color: "#0D99FF".into(),
            file_name: "Shared".into(),
            pages: vec![],
            url: link.replace("figcraft://join?host=", "ws://"),
            link,
            address: "192.168.1.20:47900".into(),
            network: true,
            access: Access::Edit,
        })
    }
    fn join(&mut self, session: u64, link: &str, _: &str) -> Result<(), String> {
        if !link.starts_with("figcraft://join?") {
            return Err("that's not a live session invite".into());
        }
        let mut s = self.0.lock().unwrap();
        s.session = session;
        s.joined.push(link.to_string());
        Ok(())
    }
    fn publish(&mut self, session: u64, opts: PublishOptions) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        s.session = session;
        s.published.push(opts);
        Ok(())
    }
    fn background(&mut self, job: u64, req: NetRequest) -> bool {
        self.0.lock().unwrap().asked.push((job, req));
        true
    }
    fn leave(&mut self, session: u64) {
        self.0.lock().unwrap().left.push(session);
    }
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

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn err(s: &mut Session, cmd: &str, p: Value) -> String {
    s.execute(cmd, &p).unwrap_err().to_string()
}

/// The file is on the server and joined: its links, then (when given) its codes.
fn on_the_server(s: &mut Session, script: &Arc<Mutex<Script>>, codes: Option<(&str, &str)>) {
    let link = |token: &str| format!("figcraft://join?server=wss://share.example:443/&file=f1&token={token}");
    let published = Published {
        server: "wss://share.example:443/".into(),
        file: "f1".into(),
        name: "Untitled".into(),
        edit: link("EditEditEditEditEdit00"),
        view: link("ViewViewViewViewView00"),
        owner: Some("OwnerOwnerOwnerOwner00".into()),
        expires: None,
    };
    let info = SessionInfo {
        role: Role::Guest,
        me: 1,
        name: "Me".into(),
        color: "#0D99FF".into(),
        file_name: "Untitled".into(),
        pages: vec![],
        link: published.edit.clone(),
        url: String::new(),
        address: "wss://share.example:443/".into(),
        network: true,
        access: Access::Edit,
    };
    let mut events = vec![Event::Published(published)];
    events.extend(codes.map(|(edit, view)| Event::Codes { edit: edit.into(), view: view.into() }));
    events.push(Event::Welcome { info, peers: vec![] });
    script.lock().unwrap().events.extend(events);
    s.live_poll();
}

#[test]
fn the_built_in_server_is_the_default_and_can_be_changed_or_turned_off() {
    let (mut s, _) = session();
    assert_eq!(s.prefs.publish_server, DEFAULT_SERVER);
    assert!(DEFAULT_SERVER.starts_with("wss://") && DEFAULT_SERVER.ends_with('/'), "TLS, and an address the preference itself accepts");
    assert_eq!(ex(&mut s, "pref.publishServer", json!({"url": DEFAULT_SERVER}))["url"], DEFAULT_SERVER);
    // Another server; back to the built-in one; off.
    assert_eq!(ex(&mut s, "pref.publishServer", json!({"url": "wss://share.example/"}))["url"], "wss://share.example/");
    let reset = ex(&mut s, "pref.publishServer", json!({}));
    assert_eq!((&reset["url"], &reset["default"]), (&json!(DEFAULT_SERVER), &json!(DEFAULT_SERVER)));
    assert_eq!(ex(&mut s, "pref.publishServer", json!({"url": ""}))["url"], "");
    // Off is saved as off (an empty address isn't left out, or the default would come back).
    let saved = serde_json::to_value(&s.prefs).unwrap();
    assert_eq!(saved["publish_server"], "");
    let loaded: crate::Prefs = serde_json::from_value(saved).unwrap();
    assert!(loaded.publish_server.is_empty());
    // Preferences saved before there was a default get it.
    let old: crate::Prefs = serde_json::from_value(json!({"theme": "dark"})).unwrap();
    assert_eq!(old.publish_server, DEFAULT_SERVER);
}

#[test]
fn sharing_online_publishes_to_the_server_in_preferences_and_shows_its_code() {
    let (mut s, script) = session();
    ex(&mut s, "pref.publishServer", json!({"url": "wss://share.example/"}));
    let r = ex(&mut s, "collab.share", json!({}));
    assert_eq!(
        (&r["status"], &r["share"]["via"], &r["share"]["access"], &r["code"]),
        (&json!("connecting"), &json!("online"), &json!("edit"), &Value::Null)
    );
    assert_eq!(r["readOnly"], true, "as for any publish, until the file is there");
    {
        let sc = script.lock().unwrap();
        let [p] = sc.published.as_slice() else { panic!("published {}", sc.published.len()) };
        assert_eq!((p.server.as_str(), p.key.as_deref(), p.name.as_str(), p.codes), ("wss://share.example/", None, "Untitled", true));
        assert!(sc.hosted.is_empty());
    }
    on_the_server(&mut s, &script, Some(("K7M2QX9TD", "4FHJ8R2WN")));
    let r = ex(&mut s, "collab.peers", json!({}));
    assert_eq!((&r["status"], &r["code"]), (&json!("live"), &json!("K7M-2QX-9TD")));
    assert_eq!(r["share"], json!({"via": "online", "access": "edit", "editCode": "K7M-2QX-9TD", "viewCode": "4FH-J8R-2WN"}));
    assert!(s.message.is_none(), "the popover shows the code: no toast about publishing");
    // Asking again only switches which code is the one to give out.
    let r = ex(&mut s, "collab.share", json!({"access": "view"}));
    assert_eq!((&r["code"], &r["share"]["access"]), (&json!("4FH-J8R-2WN"), &json!("view")));
    assert_eq!(ex(&mut s, "collab.share", json!({"via": "online"}))["code"], "4FH-J8R-2WN", "nothing new starts");
    assert_eq!(ex(&mut s, "collab.share", json!({"access": "edit"}))["code"], "K7M-2QX-9TD");
    assert_eq!(script.lock().unwrap().published.len(), 1);
    assert!(err(&mut s, "collab.share", json!({"via": "local"})).contains("stop sharing it first"));
    assert!(err(&mut s, "collab.share", json!({"access": "admin"})).contains("`access` must be"));
    // Stopping: the session ends and the server is asked, as the file's owner, to remove it.
    assert_eq!(ex(&mut s, "collab.stop", json!({}))["ended"], true);
    let sc = script.lock().unwrap();
    assert_eq!(sc.left, vec![1]);
    let [(0, NetRequest::Hosting { server, file, owner, req: Some(Admin::Remove { file: removed }) })] = sc.asked.as_slice() else {
        panic!("asked {:?}", sc.asked)
    };
    assert_eq!((server.as_str(), file.as_str(), owner.as_str(), removed.as_str()), ("wss://share.example/", "f1", "OwnerOwnerOwnerOwner00", "f1"));
    drop(sc);
    let r = ex(&mut s, "collab.peers", json!({}));
    assert_eq!((&r["status"], &r["code"], &r["share"]), (&json!("off"), &Value::Null, &Value::Null));
}

#[test]
fn a_server_without_join_codes_leaves_the_links() {
    let (mut s, script) = session();
    ex(&mut s, "collab.share", json!({"via": "online", "server": "wss://old.example/?key=KeyKeyKeyKeyKeyKeyKey0", "name": "Poster"}));
    {
        let sc = script.lock().unwrap();
        assert_eq!((sc.published[0].server.as_str(), sc.published[0].name.as_str()), ("wss://old.example/?key=KeyKeyKeyKeyKeyKeyKey0", "Poster"));
    }
    on_the_server(&mut s, &script, None);
    let r = ex(&mut s, "collab.peers", json!({}));
    assert_eq!((&r["status"], &r["code"], &r["share"]["editCode"]), (&json!("live"), &Value::Null, &Value::Null));
    assert!(r["published"]["edit"].as_str().unwrap().contains("token="), "the links are what there is to share");
    assert!(!r.to_string().contains("KeyKey"), "the server's key is nowhere in the summary");
    // Codes that aren't codes are not shown either.
    script.lock().unwrap().events.push(Event::Codes { edit: "<script>".into(), view: "K7M2QX9TDX".into() });
    s.live_poll();
    assert_eq!(ex(&mut s, "collab.peers", json!({}))["share"]["viewCode"], Value::Null);
    // `collab.leave` keeps the file on the server: nothing is asked of it.
    ex(&mut s, "collab.leave", json!({}));
    assert!(script.lock().unwrap().asked.is_empty());
}

#[test]
fn online_sharing_is_off_without_a_server() {
    let (mut s, script) = session();
    ex(&mut s, "pref.publishServer", json!({"url": ""}));
    assert!(matches!(s.execute("collab.share", &json!({})), Err(EngineError::Disabled(_, why)) if why.contains("online sharing is off")));
    assert!(err(&mut s, "collab.join", json!({"code": "K7M-2QX-9TD"})).contains("online sharing is off"));
    // The local network needs no server, and a server named for one share is enough.
    ex(&mut s, "collab.share", json!({"server": "wss://share.example/"}));
    assert_eq!(script.lock().unwrap().published[0].server, "wss://share.example/");
    ex(&mut s, "collab.stop", json!({}));
    assert_eq!(ex(&mut s, "collab.share", json!({"via": "local"}))["status"], "live");
    // Without a link (the one-shot CLI) there is nothing to share with.
    let mut bare = Session::new();
    assert!(matches!(bare.execute("collab.share", &json!({"via": "local"})), Err(EngineError::Disabled(..))));
    assert!(matches!(bare.execute("collab.join", &json!({"code": HOST.code()})), Err(EngineError::Disabled(..))));
}

#[test]
fn sharing_on_the_local_network_shows_a_code_that_carries_the_address() {
    let (mut s, script) = session();
    let r = ex(&mut s, "collab.share", json!({"via": "local"}));
    assert_eq!((&r["status"], &r["role"], &r["share"]["via"]), (&json!("live"), &json!("host"), &json!("local")));
    let shown = r["code"].as_str().unwrap();
    assert_eq!(code::parse(shown), Ok(code::Code::Local(HOST)), "{shown}");
    assert_eq!(shown, code::grouped(&HOST.code()));
    assert_eq!((&r["link"], &r["share"]["viewCode"]), (&json!(HOST.link()), &Value::Null));
    {
        let sc = script.lock().unwrap();
        let [h] = sc.hosted.as_slice() else { panic!("hosted {}", sc.hosted.len()) };
        assert!(h.short && h.bind == Bind::Lan && h.port.is_none(), "the local network unless told otherwise");
    }
    // Everyone edits there.
    assert!(err(&mut s, "collab.share", json!({"access": "view"})).contains("share it online"));
    assert_eq!(ex(&mut s, "collab.stop", json!({}))["ended"], true);
    assert!(script.lock().unwrap().asked.is_empty(), "no server to tell");
    assert!(err(&mut s, "collab.share", json!({"via": "local", "access": "view"})).contains("share it online"));
    // Where to listen is the advanced part.
    ex(&mut s, "collab.share", json!({"via": "local", "bind": "local", "port": 5000}));
    let sc = script.lock().unwrap();
    assert!(sc.hosted[1].bind == Bind::Local && sc.hosted[1].port == Some(5000));
    drop(sc);
    ex(&mut s, "collab.stop", json!({}));
    for bad in [json!({"via": "post"}), json!({"via": "local", "port": 70000}), json!({"via": "local", "bind": "everywhere"})] {
        assert!(matches!(s.execute("collab.share", &bad), Err(EngineError::BadParams { .. })), "{bad}");
    }
    // A plain `collab.host` session has an invite link, not a code.
    let r = ex(&mut s, "collab.host", json!({}));
    assert_eq!((&r["code"], &r["share"]), (&Value::Null, &Value::Null));
    assert!(!script.lock().unwrap().hosted[2].short);
}

#[test]
fn joining_takes_a_code_of_either_kind_or_a_link() {
    let (mut s, script) = session();
    ex(&mut s, "pref.publishServer", json!({"url": "wss://share.example/"}));
    let asks = "figcraft://join?server=wss://share.example/&code=K7M2QX9TD";
    let local = HOST.link();
    let link = "figcraft://join?host=10.0.0.5:47900&file=abc&token=AbCdEfGhIjKlMnOpQrStUv";
    let shown = code::grouped(&HOST.code());
    let table: [(Value, &str); 8] = [
        (json!({"code": "K7M-2QX-9TD"}), asks),
        (json!({"code": "  k7m 2qx 9td \n"}), asks),
        (json!({"code": "k7m2qx9td"}), asks),
        (json!({"link": "K7M2QX9TD"}), asks),
        (json!({"code": shown}), &local),
        (json!({"code": shown.to_lowercase().replace('-', "")}), &local),
        (json!({"link": link}), link),
        // The one field takes either: a link where a code is expected.
        (json!({"code": format!("  {link} ")}), link),
    ];
    for (params, want) in table {
        let r = ex(&mut s, "collab.join", params.clone());
        assert_eq!(r["status"], "connecting", "{params}");
        assert_eq!(script.lock().unwrap().joined.last().map(String::as_str), Some(want), "{params}");
    }
    // O for 0 and I or L for 1 are the same code.
    assert_eq!(join_target("wss://s.example/", "O1l-iLo-0oO"), join_target("wss://s.example/", "011-110-000"));
    // What can't be a code says why, and nothing is asked of the network.
    let before = script.lock().unwrap().joined.len();
    for (typed, why) in [
        ("", "type the join code"),
        ("K7M-2QX", "9 characters"),
        ("K7M-2QX-9TDX", "that one has 10"),
        ("K7M-2QU-9TD", "no U"),
        ("hello world!", "only letters and digits"),
        (&shown.replace(&shown[..1], if shown.starts_with('Z') { "Y" } else { "Z" }), "typing mistake"),
    ] {
        let e = err(&mut s, "collab.join", json!({"code": typed}));
        assert!(e.contains(why), "{typed:?}: {e}");
    }
    assert!(err(&mut s, "collab.join", json!({})).contains("give the join `code`"));
    assert!(err(&mut s, "collab.join", json!({"code": 7})).contains("9 characters"));
    assert!(s.execute("collab.join", &json!({"code": {"x": 1}})).is_err());
    // Something that looks like a link goes to the link's reader.
    assert!(err(&mut s, "collab.join", json!({"code": "https://example.com/x"})).contains("not a live session invite"));
    assert_eq!(script.lock().unwrap().joined.len(), before);
    // A failed lookup shows on the tab it was started from (the link's event).
    ex(&mut s, "collab.join", json!({"code": "K7M-2QX-9TD"}));
    script.lock().unwrap().events.push(Event::Status(Status::Failed { reason: "No session with that code".into() }));
    s.live_poll();
    let r = ex(&mut s, "collab.peers", json!({}));
    assert_eq!((&r["status"], &r["reason"]), (&json!("failed"), &json!("No session with that code")));
    assert_eq!(s.message.as_deref(), Some("No session with that code"));
}
