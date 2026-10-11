//! Files on a server in the window: the Publish to server dialog (from the File menu and the
//! Share popover; its field takes the keyboard; Escape closes it; publishing leaves the tab
//! read-only until the file is there), the Share popover's address, access and a published file's
//! links, and a view link's "View only" badge with only the moving tools.

use egui::{Event, Key};
use figcraft_engine::figcraft_collab::message::{ClientMsg, ServerMsg};
use figcraft_engine::figcraft_collab::{Client, Limits, Outgoing, Server};
use figcraft_engine::live::{Access, Event as LiveEvent, Published, Role, SessionInfo, Status, VIEW_ONLY};
use figcraft_engine::tools::ToolId;
use serde_json::json;

use crate::live_tests::{Harness, has_text, text_center};

fn key(h: &mut Harness, key: Key) {
    for pressed in [true, false] {
        h.frame(vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() }]);
    }
}

fn focused(h: &Harness, id: &str) -> bool {
    h.ctx.memory(|m| m.has_focus(egui::Id::new(id)))
}

/// Joined through a server's link with `access`: the welcome and the document (a new tab).
fn joined(h: &mut Harness, access: Access) {
    h.app.session.execute("collab.join", &json!({"link": "figcraft://join?host=127.0.0.1:47900&file=f&token=AbCdEfGhIjKlMnOpQrStUv"})).unwrap();
    let mut server = Server::new(h.app.session.doc().unwrap().clone(), "", Limits::default());
    let ClientMsg::Hello(me) = Client::hello("f", "", "Ada", "") else { panic!("no hello") };
    let (id, outs) = server.join_as(me, access).unwrap();
    let w = outs.into_iter().find_map(|o| if let Outgoing::To(_, w @ ServerMsg::Welcome { .. }) = o { Some(w) } else { None }).unwrap();
    let info = SessionInfo {
        role: Role::Guest,
        me: id,
        name: "Ada".into(),
        color: "#14AE5C".into(),
        file_name: String::new(),
        pages: vec![],
        link: String::new(),
        url: String::new(),
        address: "127.0.0.1:47900".into(),
        network: false,
        access,
    };
    h.send(vec![LiveEvent::Welcome { info, peers: vec![] }, LiveEvent::Sync(w)]);
    h.frames(2);
    assert!(h.app.session.live_shared());
}

#[test]
fn the_publish_dialog() {
    let (mut h, _) = Harness::new();
    crate::menus::invoke(&mut h.app, "collab.publish");
    h.frames(2);
    assert_eq!(h.app.ui.live.publish.as_ref().map(|f| f.name.as_str()), Some("Untitled"), "the file's name to start with");
    assert!(focused(&h, "live_publish_server"), "the address field takes the keyboard");
    key(&mut h, Key::Escape);
    assert!(h.app.ui.live.publish.is_none(), "Escape closes it");
    // From the Share popover, then publish.
    let shapes = h.frames(2);
    h.click(text_center(&shapes, "Share"));
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "Or keep the file on a FigCraft server and share links to edit or to view it."));
    h.click(text_center(&shapes, "Publish to server…"));
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "On the server's machine, `figcraft-server key` prints it."));
    h.frame(vec![Event::Text("ws://127.0.0.1:47900/?key=KeyKeyKeyKeyKeyKeyKey0".into())]);
    let shapes = h.frames(1);
    h.click(text_center(&shapes, "Publish"));
    let shapes = h.frames(2);
    assert!(h.app.ui.live.publish.is_none(), "it closes once publishing starts");
    assert_eq!(h.app.session.live.status, Status::Connecting);
    assert!(has_text(&shapes, "Publishing…"), "the toolbar says why the file can't be edited yet");
    assert_eq!(crate::live::inspect(&h.app)["readOnly"], figcraft_engine::live::PUBLISHING);
}

#[test]
fn a_published_file_shows_its_links() {
    let (mut h, _) = Harness::new();
    joined(&mut h, Access::Edit);
    h.app.session.live.published = Some(Published {
        server: "127.0.0.1:47900".into(),
        file: "f".into(),
        name: "Team".into(),
        edit: "figcraft://join?host=127.0.0.1:47900&file=f&token=EditEditEditEditEdit00".into(),
        view: "figcraft://join?host=127.0.0.1:47900&file=f&token=ViewViewViewViewView00".into(),
    });
    let shapes = h.frames(2);
    h.click(text_center(&shapes, "Share"));
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "Published · nobody else here yet"));
    assert!(has_text(&shapes, "127.0.0.1:47900 · Can edit"));
    assert!(has_text(&shapes, "Edit link") && has_text(&shapes, "View link"));
    assert!(has_text(&shapes, "Leave session"));
}

#[test]
fn a_view_link_shows_the_badge_and_only_the_moving_tools() {
    let (mut h, _) = Harness::new();
    h.app.session.execute("tool.set", &json!({"tool": "rectangle"})).unwrap();
    joined(&mut h, Access::View);
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "View only"), "the badge");
    assert_eq!(h.app.session.tool.tool, ToolId::Move);
    assert_eq!(crate::live::inspect(&h.app)["readOnly"], VIEW_ONLY);
    // A tool's shortcut changes nothing, and says why.
    key(&mut h, Key::R);
    assert_eq!(h.app.session.tool.tool, ToolId::Move);
    assert!(h.app.ui.last_error.as_deref().is_some_and(|e| e.contains("View only")));
    // The Share popover says what the link allows.
    let shapes = h.frames(1);
    h.click(text_center(&shapes, "Share"));
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "127.0.0.1:47900 · View only"));
    assert!(has_text(&shapes, "View only: you see every change as it happens but can't edit. Leave to keep a copy you can edit."));
    // Leaving: the copy is yours, the full toolbar is back.
    h.click(text_center(&shapes, "Leave session"));
    let shapes = h.frames(2);
    assert!(!has_text(&shapes, "View only"));
    assert!(h.app.session.execute("tool.set", &json!({"tool": "rectangle"})).is_ok());
}
