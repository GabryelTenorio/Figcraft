//! Plugins in the UI: the installed plugins in the Plugins menu, the parameters form a command
//! with parameters asks for before it runs, and the Manage plugins window. Everything runs through
//! the engine's `plugin.*` commands.

use egui::{Context, Id, RichText, Ui};
use figcraft_engine::plugins::{Access, InstalledPlugin, Param, ParamKind};
use serde_json::{Map, Value, json};

use crate::FigApp;
use crate::dialogs::window;
use crate::i18n::{tr, trf};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, FieldEdit, Lead};

/// The parameters form of a plugin command, with the values entered so far.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginForm {
    pub id: String,
    pub command: String,
    pub values: Map<String, Value>,
}

/// Width of the plugin windows' fields.
const FIELD_W: f32 = 220.0;

/// Start plugin `id`'s `command` (empty: its first): ask for its parameters first when it has any.
pub fn start(app: &mut FigApp, id: &str, command: &str) {
    let found = app.session.prefs.plugins.iter().find(|p| p.manifest.id == id).and_then(|p| p.manifest.command(Some(command)));
    match found {
        Some(c) if !c.params.is_empty() => {
            let values = c.params.iter().map(|p| (p.name.clone(), p.default_value())).collect();
            app.ui.plugin_form = Some(PluginForm { id: id.to_string(), command: c.id.clone(), values });
        }
        _ => run(app, id, command, Value::Object(Map::new())),
    }
}

/// Run a plugin command; a text output is shown as a toast unless the plugin showed a message.
fn run(app: &mut FigApp, id: &str, command: &str, params: Value) {
    let toast = app.ui.toast.clone();
    let Some(out) = app.run("plugin.run", json!({"id": id, "command": command, "params": params})) else { return };
    if app.ui.toast == toast
        && let Some(text) = out["output"].as_str().filter(|t| !t.trim().is_empty())
    {
        app.toast(text.chars().take(200).collect::<String>());
    }
}

/// Plugins ▸ Development ▸ Import plugin from manifest…: pick a `figcraft-plugin.json`.
pub fn import_dialog(app: &mut FigApp) {
    let Some(p) = app.services.pick_open(tr("Import plugin from manifest"), &["json"]) else { return };
    if let Some(v) = app.run("plugin.import", json!({"path": p.to_string_lossy()})) {
        app.toast(trf("Imported {name}", &[("name", &v["name"].as_str().unwrap_or(tr("the plugin")))]));
    }
}

/// The installed plugins at the top of the Plugins menu: a plugin with one command is an item, one
/// with several a submenu. True when one was clicked.
pub fn menu(app: &mut FigApp, ui: &mut Ui, t: &Tokens) -> bool {
    if app.session.prefs.plugins.is_empty() {
        return false;
    }
    let mut chosen: Option<(String, String)> = None;
    for p in &app.session.prefs.plugins {
        let m = &p.manifest;
        match m.commands.as_slice() {
            [only] => {
                // An ellipsis: it asks for parameters first.
                let label = if only.params.is_empty() { m.name.clone() } else { format!("{}…", m.name) };
                if crate::menus::item(ui, &label, "", true, false, t) {
                    chosen = Some((m.id.clone(), only.id.clone()));
                }
            }
            commands => {
                let picked = crate::menus::submenu(ui, "Plugins", &format!("Plugins/plugin:{}", m.id), &m.name, t, |ui| {
                    commands.iter().find(|c| crate::menus::item(ui, &c.label, "", true, false, t)).map(|c| c.id.clone())
                });
                if let Some(c) = picked {
                    chosen = Some((m.id.clone(), c));
                }
            }
        }
    }
    crate::menus::separator(ui);
    let Some((id, command)) = chosen else { return false };
    start(app, &id, &command);
    true
}

/// The plugin windows (Manage plugins, a command's parameters).
pub fn show(app: &mut FigApp, ctx: &Context) {
    if app.ui.plugins_window {
        manage_window(app, ctx);
    }
    if app.ui.plugin_form.is_some() {
        form_window(app, ctx);
    }
}

fn manage_window(app: &mut FigApp, ctx: &Context) {
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut remove = None;
    let mut import = false;
    window(tr("Plugins"), ctx).id(Id::new("Plugins")).open(&mut open).show(ctx, |ui| {
        ui.set_width(380.0);
        let plugins = &app.session.prefs.plugins;
        if plugins.is_empty() {
            widgets::label(ui, tr("No plugins yet. Import a development plugin from its figcraft-plugin.json."), t.text_secondary);
        }
        for (i, p) in plugins.iter().enumerate() {
            if i > 0 {
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(280.0);
                    ui.label(RichText::new(&p.manifest.name).font(semibold(12.0)).color(t.text));
                    widgets::label(ui, &summary(p), t.text_secondary);
                });
                if widgets::button(ui, tr("Remove"), false, true).clicked() {
                    remove = Some(p.manifest.id.clone());
                }
            });
        }
        ui.add_space(12.0);
        if widgets::button(ui, tr("Import plugin from manifest…"), true, true).clicked() {
            import = true;
        }
    });
    app.ui.plugins_window = open;
    if let Some(id) = remove {
        app.run("plugin.remove", json!({"id": id}));
    }
    if import {
        import_dialog(app);
    }
}

/// "1.0 · can edit · development: <path>" under a plugin's name.
fn summary(p: &InstalledPlugin) -> String {
    let m = &p.manifest;
    let access = match m.permissions.access {
        Access::Read => tr("reads the document"),
        Access::Write => tr("can edit the document"),
    };
    let mut parts: Vec<String> = Vec::new();
    if !m.version.is_empty() {
        parts.push(m.version.clone());
    }
    parts.push(access.to_string());
    parts.push(match &p.path {
        Some(path) => trf("development: {path}", &[("path", path)]),
        None => tr("this session only").into(),
    });
    parts.join(" · ")
}

fn form_window(app: &mut FigApp, ctx: &Context) {
    let Some(mut form) = app.ui.plugin_form.clone() else { return };
    let Some((name, command)) = app
        .session
        .prefs
        .plugins
        .iter()
        .find(|p| p.manifest.id == form.id)
        .and_then(|p| Some((p.manifest.name.clone(), p.manifest.command(Some(&form.command))?.clone())))
    else {
        app.ui.plugin_form = None;
        return;
    };
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut submit = false;
    let mut cancel = false;
    let label = command.label.trim_end_matches(['…', '.']).trim();
    let title = if label.contains(name.as_str()) { label.to_string() } else { format!("{name}: {label}") };
    window(&title, ctx).id(Id::new("plugin_form")).open(&mut open).show(ctx, |ui| {
        ui.set_width(320.0);
        egui::Grid::new("plugin_form_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            for p in &command.params {
                ui.label(RichText::new(p.title()).font(regular(11.0)).color(t.text_secondary));
                let current = form.values.get(&p.name).cloned().unwrap_or_else(|| p.default_value());
                if let Some(v) = field(ui, &form, p, &current) {
                    form.values.insert(p.name.clone(), v);
                }
                ui.end_row();
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            submit = widgets::button(ui, tr("Run"), true, true).clicked();
            cancel = widgets::button(ui, tr("Cancel"), false, true).clicked();
        });
    });
    app.ui.plugin_form = (open && !submit && !cancel).then_some(form.clone());
    if submit {
        run(app, &form.id, &form.command, Value::Object(form.values));
    }
}

/// One parameter's field; the new value when it changed.
fn field(ui: &mut Ui, form: &PluginForm, p: &Param, current: &Value) -> Option<Value> {
    let id = Id::new(("plugin_param", &form.id, &form.command, &p.name));
    match p.kind {
        ParamKind::Text => widgets::text_field(ui, id, current.as_str().unwrap_or_default(), FIELD_W, "").map(Value::String),
        ParamKind::Number => match widgets::number_field(ui, id, Lead::None, current.as_f64(), "", FIELD_W, 1.0)? {
            FieldEdit::Set(v) | FieldEdit::Scrub(v, _) => p.coerce(&json!(v)).ok(),
        },
        ParamKind::Boolean => {
            let on = current.as_bool().unwrap_or(false);
            widgets::checkbox(ui, on, "").then_some(Value::Bool(!on))
        }
        ParamKind::Select => {
            let resp = widgets::dropdown(ui, current.as_str().unwrap_or_default(), FIELD_W, None);
            let mut picked = None;
            crate::right::menu_popup(&resp).show(|ui| {
                for o in &p.options {
                    if crate::right::menu_item(ui, o, "") {
                        picked = Some(Value::String(o.clone()));
                        ui.close();
                    }
                }
            });
            picked
        }
    }
}
