//! A plugin's panel in the UI, driven headless: its controls are drawn with names a screen reader
//! reads, pressing its button calls the plugin back, closing it ends the plugin's session, and an
//! open panel draws no frames while nothing changes.

use base64::Engine as _;
use egui::Pos2;
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::idle_tests::Harness;
use crate::layers_drag_tests::Ui;

/// A session with a plugin whose first call shows a panel with every kind of control and whose
/// later calls (the panel's events) make a rectangle.
fn session() -> Session {
    let spec = json!({"title": "Test panel", "controls": [
        {"type": "note", "label": "Hello from the plugin"},
        {"type": "number", "name": "size", "label": "Size", "value": 10},
        {"type": "boolean", "name": "labels", "label": "Labels"},
        {"type": "list", "name": "palette", "label": "Palette", "options": ["Warm", "Cool"]},
        {"type": "color", "name": "accent", "label": "Accent", "value": "#FF5A5F"},
        {"type": "select", "name": "shape", "label": "Shape", "options": ["Square", "Circle"]},
        {"type": "text", "name": "title", "label": "Title"},
        {"type": "button", "name": "go", "label": "Go"}
    ]})
    .to_string();
    let rect = r#"{"x":0,"y":0,"width":10,"height":10}"#;
    let wat = format!(
        r#"(module
          (import "figcraft" "call" (func $call (param i32 i32 i32 i32) (result i32)))
          (import "figcraft" "ui_show" (func $ui_show (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 1000) "{}") (data (i32.const 100) "shape.rectangle") (data (i32.const 200) "{}")
          (func (export "figcraft_run") (param $n i32) (result i32)
            (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
            (if (i32.eq (i32.load (i32.const 0)) (i32.const 1))
              (then (return (call $ui_show (i32.const 1000) (i32.const {})))))
            (call $call (i32.const 100) (i32.const 15) (i32.const 200) (i32.const {}))))"#,
        spec.replace('"', "\\\""),
        rect.replace('"', "\\\""),
        spec.len(),
        rect.len()
    );
    let wasm = wat::parse_str(&wat).unwrap();
    let manifest = json!({"id": "org.test.panel", "name": "Panel test", "main": "p.wasm", "api": 2, "permissions": {"access": "write"}});
    let mut s = Session::new();
    s.execute("plugin.import", &json!({"manifest": manifest, "wasm": base64::engine::general_purpose::STANDARD.encode(wasm)})).unwrap();
    s.execute("plugin.run", &json!({"id": "org.test.panel"})).unwrap();
    s
}

fn layers(u: &Ui) -> usize {
    let ds = u.app.session.ds().unwrap();
    ds.doc.node(ds.page).unwrap().children.len()
}

#[test]
fn the_panel_names_its_controls_and_its_button_calls_the_plugin_back() {
    let mut u = Ui::new(session());
    u.ctx.enable_accesskit();
    u.frame(vec![]);
    let tree = u.frame_output(vec![]).platform_output.accesskit_update.unwrap().nodes;
    let named = |label: &str| tree.iter().map(|(_, n)| n).find(|n| n.label() == Some(label)).cloned();
    for label in ["Size", "Labels", "Warm", "Cool", "Accent", "Shape", "Title", "Go"] {
        assert!(named(label).is_some(), "no control named {label:?}");
    }
    let go = named("Go").and_then(|n| n.bounds()).unwrap();
    let at = Pos2::new(((go.x0 + go.x1) / 2.0) as f32, ((go.y0 + go.y1) / 2.0) as f32);
    let before = layers(&u);
    u.button(at, true);
    u.button(at, false);
    assert_eq!(layers(&u), before + 1, "Go made the plugin draw a rectangle");
    assert_eq!(u.app.session.ds().unwrap().history.undo.len(), 1);
    assert!(u.app.session.plugins.panel.is_some(), "the panel stays open");
    // Its close button ends the session.
    u.app.run("plugin.close", json!({"id": "org.test.panel"}));
    u.frame(vec![]);
    assert_eq!(u.app.session.execute("plugin.ui", &json!({})).unwrap(), Value::Null);
}

#[test]
fn an_open_plugin_panel_draws_no_frames_while_idle() {
    Harness::new(session()).assert_idle("a plugin panel");
}
