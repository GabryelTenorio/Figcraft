//! Text made by commands in the app (the interface reported its theme) is readable on its page
//! like text drawn with the Text tool: white on a dark page unless a fill is named. A session
//! without an interface (CLI, MCP, exports) keeps the model's black everywhere.

use figcraft_color::{Color, Paint};
use figcraft_doc::NodeId;
use serde_json::{Value, json};

use crate::Session;
use crate::params::id_of;

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn fills(s: &Session, id: &Value) -> Vec<Paint> {
    s.doc().unwrap().node(id_of(&id["id"]).unwrap()).unwrap().fills.clone()
}

/// A new file made with the interface in `theme` (None: no interface).
fn session(theme: Option<bool>) -> Session {
    let mut s = Session::empty();
    if let Some(dark) = theme {
        s.set_ui_dark(dark);
    }
    ex(&mut s, "file.new", json!({}));
    s
}

#[test]
fn text_made_by_commands_is_readable_on_its_page() {
    let (white, black) = (vec![Paint::solid(Color::WHITE)], vec![Paint::solid(Color::BLACK)]);
    // (the interface's theme, the page's colour set afterwards, what new text is filled with, why)
    let cases = [
        (Some(true), None, &white, "the app in the dark theme: a dark page, white text"),
        (Some(false), None, &black, "the app in the light theme: a light page, black text"),
        (Some(false), Some("#101010"), &white, "it is the page that counts, not the theme: a page made dark"),
        (Some(true), Some("#FFFFFF"), &black, "and a page made light in the dark theme"),
        (None, None, &black, "no interface: black"),
        (None, Some("#101010"), &black, "no interface, a dark page: still black, scripts make the same thing everywhere"),
    ];
    for (theme, page_color, want, why) in cases {
        let mut s = session(theme);
        if let Some(c) = page_color {
            ex(&mut s, "page.setBackground", json!({"color": c}));
        }
        let made = [
            ex(&mut s, "text.create", json!({"text": "Hi", "x": 0, "y": 0})),
            ex(&mut s, "node.create", json!({"type": "text", "text": "Hi", "x": 0, "y": 40})),
            ex(&mut s, "text.create", json!({"text": "Hi", "x": 0, "y": 80, "props": {"fontSize": 20}})),
        ];
        for m in &made {
            assert_eq!(&fills(&s, m), want, "{why}");
        }
        // A fill that is asked for is the fill.
        let red = ex(&mut s, "text.create", json!({"text": "Hi", "x": 0, "y": 120, "props": {"fill": "#FF0000"}}));
        assert_eq!(fills(&s, &red), vec![Paint::solid(Color::from_hex("#FF0000").unwrap())], "{why}: a named fill");
        // Other layers keep their defaults.
        let rect = ex(&mut s, "shape.rectangle", json!({"x": 0, "y": 200, "width": 10, "height": 10}));
        assert_ne!(fills(&s, &rect), white, "{why}: shapes are not text");
    }
}

#[test]
fn text_in_a_frame_follows_the_page_it_is_on() {
    // As with the Text tool: the page decides, also inside a frame; made in one undo step.
    let mut s = session(Some(true));
    let frame: NodeId = id_of(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 200, "height": 100}))["id"]).unwrap();
    let t = ex(&mut s, "text.create", json!({"text": "Hi", "x": 10, "y": 10, "parent": frame}));
    assert_eq!(fills(&s, &t), vec![Paint::solid(Color::WHITE)]);
    ex(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().node(id_of(&t["id"]).unwrap()).is_none());
}
