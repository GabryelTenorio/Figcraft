//! The control channel's UI-thread side. The app crate accepts JSON lines on a localhost socket
//! and queues [`ControlRequest`]s; [`drain`] runs them at the start of a frame so replies reflect
//! the real UI. Methods:
//!
//! - `engine.execute {command, params}` — run any command (no dialogs open)
//! - `engine.commands {}` — every command with its params doc
//! - `document.inspect {depth?, allPages?}` / `node.get {id, depth?}`
//! - `ui.inspect {}` — panel/tab/tool state, canvas rect, timings
//! - `ui.pointer {kind: down|move|up|click|double|hover|drag, x, y (page) | sx, sy (screen), to?, shift?, alt?, cmd?}`
//! - `ui.key {key, shift?, cmd?, alt?}` — a shortcut as if typed on the canvas
//! - `ui.type {text}` — type into the text being edited
//! - `ui.menu.invoke {id}` — a menu item exactly like clicking it (may open a dialog)
//! - `ui.set {rightTab?, leftTab?, theme?, home?, palette?, preview?, presentScroll?, …}`
//! - `ui.screenshot {path}` — PNG of the window (needs a visible window)
//! - `ui.render {path, scale?, ids?}` — PNG of the document (no UI chrome; works without a window)
//! - `app.quit {}`

use egui::Context;
use figcraft_engine::{PointerEvent, PointerKind};
use serde_json::{Value, json};

use crate::FigApp;

pub struct ControlReply {
    pub id: Value,
    pub result: Result<Value, String>,
}

pub struct ControlRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
    pub reply: std::sync::mpsc::Sender<ControlReply>,
}

/// Pending screenshot replies.
pub struct PendingShot {
    pub id: Value,
    pub path: String,
    pub reply: std::sync::mpsc::Sender<ControlReply>,
}

fn pending(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<Vec<PendingShot>>> {
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new("pending_shots"), || std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))).clone())
}

pub fn drain(app: &mut FigApp, ctx: &Context) {
    // Finish screenshots that arrived.
    let shots: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
            .collect()
    });
    if let Some(img) = shots.last() {
        let list = pending(ctx);
        let mut list = list.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for p in list.drain(..) {
            let res = save_color_image(img, &p.path).map(|_| json!({"path": p.path, "width": img.size[0], "height": img.size[1]}));
            let _ = p.reply.send(ControlReply { id: p.id, result: res });
        }
    }
    let reqs = std::mem::take(&mut app.control_queue);
    for r in reqs {
        if r.method == "ui.screenshot" {
            let path = r.params.get("path").and_then(Value::as_str).unwrap_or("figcraft-screenshot.png").to_string();
            pending(ctx).lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(PendingShot { id: r.id, path, reply: r.reply });
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
            continue;
        }
        let res = handle(app, ctx, &r.method, &r.params);
        let _ = r.reply.send(ControlReply { id: r.id, result: res });
        ctx.request_repaint();
    }
}

fn save_color_image(img: &egui::ColorImage, path: &str) -> Result<(), String> {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        rgba.extend_from_slice(&p.to_srgba_unmultiplied());
    }
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("bad screenshot buffer")?;
    buf.save(path).map_err(|e| e.to_string())
}

fn b(p: &Value, k: &str) -> bool {
    p.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// Handle one control method (also used by tests and the headless snapshot tool).
pub fn handle(app: &mut FigApp, ctx: &Context, method: &str, p: &Value) -> Result<Value, String> {
    match method {
        "engine.execute" => {
            let cmd = p.get("command").and_then(Value::as_str).ok_or("`command` required")?;
            let params = p.get("params").cloned().unwrap_or(json!({}));
            let r = app.session.execute(cmd, &params).map_err(|e| e.to_string());
            // Programmatic calls never open dialogs.
            app.session.ui_requests.retain(|u| !u.kind.starts_with("dialog."));
            app.handle_ui_requests();
            r
        }
        "engine.commands" => Ok(Value::Array(
            figcraft_engine::all_commands()
                .iter()
                .map(|c| json!({"id": c.id, "label": c.label, "params": c.params, "menu": c.menu(), "shortcut": c.shortcut(), "undoable": c.history}))
                .collect(),
        )),
        "document.inspect" => {
            let depth = p.get("depth").and_then(Value::as_u64).unwrap_or(8) as usize;
            Ok(figcraft_engine::inspect::document(&app.session, depth, b(p, "allPages")))
        }
        "node.get" => {
            let id = p.get("id").and_then(figcraft_engine::params::id_of).ok_or("`id` required")?;
            let depth = p.get("depth").and_then(Value::as_u64).unwrap_or(1) as usize;
            app.session.doc().map(|d| figcraft_engine::inspect::node_json(d, id, depth)).map_err(|e| e.to_string())
        }
        "ui.inspect" => Ok(app.inspect()),
        "ui.pointer" => pointer(app, p),
        "ui.key" => {
            let key = p.get("key").and_then(Value::as_str).ok_or("`key` required")?;
            let mut spec = String::new();
            for (k, m) in [("shift", "Shift"), ("alt", "Alt"), ("cmd", "Cmd"), ("ctrl", "Ctrl")] {
                if b(p, k) {
                    spec.push_str(m);
                    spec.push('+');
                }
            }
            spec.push_str(key);
            // Text editing keys go to the editor.
            if app.session.ds().map(|d| d.text_edit.is_some()).unwrap_or(false) {
                figcraft_engine::textedit::key(&mut app.session, key, b(p, "shift"), b(p, "alt"), b(p, "cmd")).map_err(|e| e.to_string())?;
                return Ok(json!({"ok": true}));
            }
            // Vector edit mode and shape builder keys (tools, nudging points, delete, join, Enter).
            if app.session.tool_key(key, b(p, "shift"), b(p, "alt"), b(p, "cmd")).map_err(|e| e.to_string())? {
                return Ok(json!({"ok": true}));
            }
            match crate::shortcuts::parse(&spec) {
                Some(want) => {
                    let id = figcraft_engine::catalog::MENU
                        .iter()
                        .find(|m| !m.shortcut.is_empty() && crate::shortcuts::parse(m.shortcut) == Some(want))
                        .map(|m| m.id);
                    match id {
                        Some(id) => {
                            if id == "select.none" {
                                app.session.escape().map_err(|e| e.to_string())?;
                            } else {
                                crate::menus::invoke(app, id);
                            }
                            Ok(json!({"command": id}))
                        }
                        None => match key {
                            "Escape" => app.session.escape().map(|_| json!({"ok": true})).map_err(|e| e.to_string()),
                            "Delete" | "Backspace" => app.session.execute("edit.delete", &json!({})).map_err(|e| e.to_string()),
                            "Enter" => {
                                let _ = app.session.pen_finish();
                                Ok(json!({"ok": true}))
                            }
                            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
                                let (dx, dy) = match key {
                                    "ArrowLeft" => (-1, 0),
                                    "ArrowRight" => (1, 0),
                                    "ArrowUp" => (0, -1),
                                    _ => (0, 1),
                                };
                                let (cmd, params) = if b(p, "cmd") {
                                    ("transform.nudgeSize", json!({"dw": dx, "dh": dy, "big": b(p, "shift")}))
                                } else {
                                    ("transform.nudge", json!({"dx": dx, "dy": dy, "big": b(p, "shift")}))
                                };
                                app.session.execute(cmd, &params).map_err(|e| e.to_string())
                            }
                            _ => Err(format!("no shortcut `{spec}`")),
                        },
                    }
                }
                None => Err(format!("unknown key `{key}`")),
            }
        }
        "ui.type" => {
            let text = p.get("text").and_then(Value::as_str).ok_or("`text` required")?;
            figcraft_engine::textedit::insert(&mut app.session, text).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "ui.menu.invoke" => {
            let id = p.get("id").and_then(Value::as_str).ok_or("`id` required")?;
            crate::menus::invoke(app, id);
            Ok(json!({"ok": true, "lastError": app.ui.last_error}))
        }
        "ui.set" => {
            if let Some(t) = p.get("rightTab").and_then(Value::as_str) {
                app.ui.right_tab = match t {
                    "prototype" => crate::RightTab::Prototype,
                    "inspect" => crate::RightTab::Inspect,
                    _ => crate::RightTab::Design,
                };
            }
            if let Some(t) = p.get("leftTab").and_then(Value::as_str) {
                app.ui.left_tab = if t == "assets" { crate::LeftTab::Assets } else { crate::LeftTab::File };
            }
            if let Some(t) = p.get("theme").and_then(Value::as_str) {
                app.session.prefs.theme = t.to_string();
            }
            if let Some(h) = p.get("home").and_then(Value::as_bool) {
                app.ui.home = h;
            }
            if let Some(h) = p.get("palette").and_then(Value::as_bool) {
                app.ui.palette = if h { Some(String::new()) } else { None };
            }
            if let Some(h) = p.get("mainMenu").and_then(Value::as_bool) {
                app.ui.main_menu = h;
            }
            // Comment thread popover: an id opens it, null closes it.
            if let Some(v) = p.get("commentOpen") {
                app.ui.comments.open = v.as_u64();
                app.ui.comments.draft = None;
            }
            if let Some(v) = p.get("commentDraft") {
                match (v.get("x").and_then(Value::as_f64), v.get("y").and_then(Value::as_f64)) {
                    (Some(x), Some(y)) => crate::comments::start_draft(app, x, y),
                    _ => app.ui.comments.draft = None,
                }
            }
            if let Some(v) = p.get("showResolvedComments").and_then(Value::as_bool) {
                app.ui.comments.show_resolved = v;
            }
            // Colour picker on the selection's paint `{fills?, index?}`; null closes it.
            if let Some(v) = p.get("colorPicker") {
                app.ui.picker = v.is_object().then(|| crate::colorpicker::PickerTarget::Paint {
                    fills: v.get("fills").and_then(Value::as_bool).unwrap_or(true),
                    index: v.get("index").and_then(Value::as_u64).unwrap_or(0).min(1000) as usize,
                    ids: app.session.selection(),
                });
            }
            // The Variables editor: true / a collection (id or name) opens it, false closes it.
            match p.get("variables") {
                Some(Value::Bool(open)) => app.ui.variables = open.then(Default::default),
                Some(Value::String(c)) => app.ui.variables = Some(crate::variables_panel::Editor::on(Some(c.clone()))),
                _ => {}
            }
            // Layers panel search: a string opens it with that filter, null closes it.
            if let Some(v) = p.get("layerFilter") {
                app.ui.layer_search = v.is_string();
                app.ui.layer_filter = v.as_str().unwrap_or_default().chars().take(200).collect();
            }
            if p.get("collapseLayers").and_then(Value::as_bool) == Some(true) {
                app.ui.expanded.clear();
            }
            // The prototype preview pane: true / a frame id opens it, false or null closes it.
            match p.get("preview") {
                Some(Value::Bool(false) | Value::Null) => app.ui.preview = None,
                Some(v @ (Value::Bool(true) | Value::Number(_))) => {
                    let params = if v.is_number() { json!({"start": v}) } else { json!({}) };
                    app.session.execute("proto.preview", &params).map_err(|e| e.to_string())?;
                    app.handle_ui_requests();
                }
                _ => {}
            }
            // Scroll the presented screen (the full-window player, else the preview pane) to [x, y].
            if let Some(s) = p.get("presentScroll").and_then(Value::as_array) {
                let at = |i: usize| s.get(i).and_then(Value::as_f64).unwrap_or(0.0);
                let player = match (app.ui.present.as_mut(), app.ui.preview.as_mut()) {
                    (Some(p), _) => Some(p),
                    (None, Some(pane)) => Some(&mut pane.player),
                    _ => None,
                };
                player.ok_or("nothing is presented")?.scroll_to(at(0), at(1));
            }
            Ok(app.inspect())
        }
        "ui.render" => {
            let path = p.get("path").and_then(Value::as_str).ok_or("`path` required")?;
            let scale = p.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
            let ids: Vec<figcraft_doc::NodeId> =
                p.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(figcraft_engine::params::id_of).collect()).unwrap_or_default();
            let mut params = json!({"format": "png", "scale": scale, "path": path});
            if !ids.is_empty() {
                params["ids"] = json!(ids);
            }
            app.session.execute("file.export", &params).map_err(|e| e.to_string())
        }
        "app.quit" => {
            app.ui.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            Ok(json!({"ok": true}))
        }
        other => Err(format!("unknown method `{other}`")),
    }
}

fn pointer(app: &mut FigApp, p: &Value) -> Result<Value, String> {
    let zoom = app.session.ds().map(|d| d.view.zoom).unwrap_or(1.0);
    let at = |x: Option<f64>, y: Option<f64>, sx: Option<f64>, sy: Option<f64>| -> Option<(f64, f64)> {
        match (x, y, sx, sy) {
            (Some(x), Some(y), _, _) => Some((x, y)),
            (_, _, Some(sx), Some(sy)) => {
                let pt = app.to_page(egui::pos2(sx as f32, sy as f32));
                Some((pt.x, pt.y))
            }
            _ => None,
        }
    };
    let f = |k: &str| p.get(k).and_then(Value::as_f64);
    let (x, y) = at(f("x"), f("y"), f("sx"), f("sy")).ok_or("pass x, y (page) or sx, sy (screen)")?;
    let mk = |kind: PointerKind, x: f64, y: f64| PointerEvent {
        kind,
        x,
        y,
        sx: x * zoom,
        sy: y * zoom,
        shift: b(p, "shift"),
        alt: b(p, "alt"),
        cmd: b(p, "cmd"),
        ctrl: b(p, "ctrl"),
        zoom,
    };
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("click");
    let run = |app: &mut FigApp, ev: PointerEvent| app.session.pointer(&ev).map_err(|e| e.to_string());
    match kind {
        "down" => run(app, mk(PointerKind::Down, x, y))?,
        "move" => run(app, mk(PointerKind::Move, x, y))?,
        "up" => run(app, mk(PointerKind::Up, x, y))?,
        "hover" => run(app, mk(PointerKind::Hover, x, y))?,
        "double" => run(app, mk(PointerKind::DoubleClick, x, y))?,
        "click" => {
            run(app, mk(PointerKind::Down, x, y))?;
            run(app, mk(PointerKind::Up, x, y))?;
        }
        "drag" => {
            let to = p.get("to").and_then(Value::as_array).ok_or("drag needs `to: [x, y]`")?;
            let tx = to.first().and_then(Value::as_f64).ok_or("bad `to`")?;
            let ty = to.get(1).and_then(Value::as_f64).ok_or("bad `to`")?;
            let steps = p.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 200);
            run(app, mk(PointerKind::Down, x, y))?;
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                run(app, mk(PointerKind::Move, x + (tx - x) * t, y + (ty - y) * t))?;
            }
            run(app, mk(PointerKind::Up, tx, ty))?;
        }
        other => return Err(format!("unknown pointer kind `{other}`")),
    }
    app.handle_ui_requests();
    Ok(json!({"selection": app.session.selection(), "tool": app.session.tool.tool.id()}))
}
