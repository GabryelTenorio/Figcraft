//! The Manage plugins window's Browse tab, driven headless against a scripted directory: Install
//! only asks (it shows the permissions, nothing is downloaded), the confirmation's Install installs,
//! and the installed plugin's row then offers Remove.

use std::sync::{Arc, Mutex};

use egui::Pos2;
use figcraft_engine::Session;
use figcraft_engine::figcraft_collab::message::{ClientMsg, PluginEntry, PluginReply, PluginReq};
use figcraft_engine::live::{Event, HostOptions, LiveLink, Presence, SessionInfo};
use figcraft_engine::plugin_directory::bundle::{Bundle, sha256_hex};
use serde_json::json;

use crate::layers_drag_tests::Ui;
use crate::plugins_ui::PluginsTab;

const ID: &str = "org.example.grid";

struct Link(Arc<Mutex<usize>>, Vec<u8>);

impl LiveLink for Link {
    fn host(&mut self, _: HostOptions) -> Result<SessionInfo, String> {
        Err("no".into())
    }
    fn join(&mut self, _: &str, _: &str) -> Result<(), String> {
        Err("no".into())
    }
    fn leave(&mut self) {}
    fn presence(&mut self, _: &Presence) {}
    fn send(&mut self, _: ClientMsg) {}
    fn poll(&mut self) -> Vec<Event> {
        Vec::new()
    }
    fn plugins(&mut self, _: &str, req: PluginReq) -> Result<PluginReply, String> {
        let entry = PluginEntry {
            id: ID.into(),
            name: "Grid maker".into(),
            version: "1.0.0".into(),
            author: "Ada".into(),
            access: "write".into(),
            commands: vec!["shape.rectangle".into()],
            bytes: self.1.len() as u64,
            sha256: sha256_hex(&self.1),
            ..PluginEntry::default()
        };
        Ok(match req {
            PluginReq::List { .. } => PluginReply::List(vec![entry]),
            PluginReq::Fetch { .. } => {
                *self.0.lock().unwrap() += 1;
                PluginReply::Bundle { entry, bundle: self.1.clone() }
            }
            _ => PluginReply::Failed { reason: "no".into() },
        })
    }
}

fn centre(n: &egui::accesskit::Node) -> Pos2 {
    let b = n.bounds().unwrap();
    Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
}

/// Every node labelled `label`, in tree order.
fn named(u: &mut Ui, label: &str) -> Vec<egui::accesskit::Node> {
    let tree = u.frame_output(vec![]).platform_output.accesskit_update.unwrap().nodes;
    tree.into_iter().map(|(_, n)| n).filter(|n| n.label() == Some(label)).collect()
}

/// Some node's label (a text's value) holds `text`.
fn shows(u: &mut Ui, text: &str) -> bool {
    let tree = u.frame_output(vec![]).platform_output.accesskit_update.unwrap().nodes;
    tree.iter().any(|(_, n)| n.label().or(n.value()).is_some_and(|l| l.contains(text)))
}

fn click(u: &mut Ui, at: Pos2) {
    u.button(at, true);
    u.button(at, false);
}

#[test]
fn install_asks_first_and_then_installs() {
    let wasm =
        wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "figcraft_run") (param i32) (result i32) i32.const 0))"#).unwrap();
    let manifest = json!({"id": ID, "name": "Grid maker", "version": "1.0.0", "main": "g.wasm", "permissions": {"access": "write", "commands": ["shape.rectangle"]}});
    let bytes = Bundle { manifest: manifest.to_string(), wasm, assets: Vec::new() }.encode();
    let fetched = Arc::new(Mutex::new(0));
    let mut s = Session::new();
    s.live.install(Box::new(Link(fetched.clone(), bytes)));
    s.execute("plugin.directory.list", &json!({"server": "ws://127.0.0.1:9/"})).unwrap();
    let mut u = Ui::new(s);
    u.app.ui.plugins_window = true;
    u.app.ui.plugins_manage.tab = PluginsTab::Browse;
    u.ctx.enable_accesskit();
    u.frame(vec![]);
    let row = named(&mut u, "Install");
    assert_eq!(row.len(), 1, "the plugin's row offers Install");
    let row_at = centre(&row[0]);
    click(&mut u, row_at);
    assert_eq!(u.app.ui.plugins_manage.confirm.as_deref(), Some(ID), "Install asks first");
    assert!(u.app.session.prefs.plugins.is_empty());
    assert_eq!(*fetched.lock().unwrap(), 0, "nothing is downloaded before the user confirms");
    u.frame(vec![]);
    for shown in ["read and edit the document", "call only: shape.rectangle", "From ws://127.0.0.1:9/"] {
        assert!(shows(&mut u, shown), "the confirmation shows {shown:?}");
    }
    let confirm = named(&mut u, "Install").into_iter().map(|n| centre(&n)).find(|p| p.distance(row_at) > 4.0).unwrap();
    click(&mut u, confirm);
    assert_eq!(*fetched.lock().unwrap(), 1);
    assert_eq!(u.app.session.prefs.plugins.len(), 1, "installed");
    assert_eq!(u.app.ui.plugins_manage.confirm, None);
    u.frame(vec![]);
    assert!(named(&mut u, "Install").is_empty() && !named(&mut u, "Remove").is_empty(), "the row now offers Remove");
}
