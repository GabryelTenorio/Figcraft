//! The Variables editor: collections on the left; the selected collection's variables on the
//! right as a table with one value column per mode. Double-click a name to rename it, right-click
//! for more; a value's variable button turns it into an alias of another variable. Everything goes
//! through `variable.*` commands, so it's undoable and the same over MCP.

use egui::{Align2, Context, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
use figcraft_doc::{Collection, Variable, VariableType, VariableValue, Variables, scopes_for};
use serde_json::{Value, json};

use crate::FigApp;
use crate::colorpicker::PickerTarget;
use crate::icons::{self, Icon};
use crate::right::menu_item;
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, FieldEdit, Lead};

const SIDEBAR_W: f32 = 200.0;
const NAME_W: f32 = 240.0;
const MODE_W: f32 = 200.0;
const ROW_H: f32 = 36.0;
const TYPES: [VariableType; 4] = [VariableType::Color, VariableType::Number, VariableType::String, VariableType::Boolean];

/// What's being renamed inline.
#[derive(Clone, Debug, PartialEq, Hash)]
enum Rename {
    Collection(String),
    /// (collection, mode)
    Mode(String, String),
    Variable(String),
    /// (collection, group path)
    Group(String, String),
}

/// The open editor's state (`UiState::variables`; None = closed).
#[derive(Clone, Default)]
pub struct Editor {
    /// The selected collection's id.
    collection: Option<String>,
    query: String,
    rename: Option<Rename>,
    /// The variable whose details (name, description, scopes) are open.
    details: Option<String>,
    /// The group the table shows ("space" or "color/brand"); None = every variable.
    group: Option<String>,
}

impl Editor {
    /// An editor showing `collection` (id or name; the first one when None or unknown).
    pub fn on(collection: Option<String>) -> Self {
        Editor { collection, ..Default::default() }
    }
}

/// Show the editor while it's open (call once per frame).
pub fn show(app: &mut FigApp, ctx: &Context) {
    let Some(mut st) = app.ui.variables.take() else { return };
    let Ok(doc) = app.session.ds().map(|d| d.doc.clone()) else { return };
    let vars = &doc.variables;
    st.collection = st.collection.as_ref().and_then(|c| vars.collection(c)).or_else(|| vars.collections.first()).map(|c| c.id.clone());
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let size = vec2(880.0_f32.min(screen.width() - 80.0).max(480.0), 480.0_f32.min(screen.height() - 120.0).max(240.0));
    let mut open = true;
    egui::Window::new("Local variables")
        .id(Id::new("variables_window"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(size)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.panel).inner_margin(egui::Margin::same(0)))
        .show(ctx, |ui| {
            ui.set_min_size(size);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            header(ui, &mut open);
            let h = (size.y - 44.0).max(160.0);
            let top_down = egui::Layout::top_down(egui::Align::Min);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(SIDEBAR_W, h), top_down, |ui| sidebar(app, ui, vars, &mut st));
                let (line, _) = ui.allocate_exact_size(vec2(1.0, h), Sense::hover());
                ui.painter().rect_filled(line, CornerRadius::ZERO, t.border);
                ui.allocate_ui_with_layout(vec2(size.x - SIDEBAR_W - 1.0, h), top_down, |ui| table(app, ui, vars, &mut st));
            });
        });
    details(app, ctx, vars, &mut st);
    app.ui.variables = open.then_some(st);
}

fn header(ui: &mut Ui, open: &mut bool) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    ui.painter().text(pos2(r.min.x + 16.0, r.center().y), Align2::LEFT_CENTER, "Local variables", semibold(13.0), t.text);
    if small_button(ui, Id::new("vars_close"), Rect::from_center_size(pos2(r.max.x - 22.0, r.center().y), vec2(24.0, 24.0)), Icon::Close, "Close") {
        *open = false;
    }
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, t.border));
}

/// A square icon button painted at `rect`; true when clicked.
fn small_button(ui: &mut Ui, id: Id, rect: Rect, icon: Icon, tip: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, id, Sense::click()).on_hover_text(tip);
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), rect.shrink(5.0), icon, t.icon);
    resp.clicked()
}

fn rename_id(target: &Rename) -> Id {
    Id::new(("var_rename", target))
}

fn start_rename(ui: &Ui, st: &mut Editor, target: Rename) {
    ui.memory_mut(|m| m.request_focus(rename_id(&target).with("te")));
    st.rename = Some(target);
}

/// The inline editor of a rename in progress: the new name once committed. The rename ends when
/// the editor closes.
fn rename_field(ui: &mut Ui, st: &mut Editor, target: &Rename, value: &str, rect: Rect, font: egui::FontId) -> Option<String> {
    let id = rename_id(target);
    let out = widgets::text_field_inline(ui, id, value, rect, font);
    if out.is_some() || !ui.memory(|m| m.has_focus(id.with("te"))) {
        st.rename = None;
    }
    out
}

/// The first of `base`, `base 2`, `base 3`… that `taken` doesn't hold.
fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..10_000).map(|k| if k == 1 { base.to_string() } else { format!("{base} {k}") }).find(|n| !taken(n)).unwrap_or_else(|| base.to_string())
}

fn sidebar(app: &mut FigApp, ui: &mut Ui, vars: &Variables, st: &mut Editor) {
    let t = Tokens::get(ui.ctx());
    let (hr, _) = ui.allocate_exact_size(vec2(SIDEBAR_W, 40.0), Sense::hover());
    ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Collections", semibold(11.0), t.text_secondary);
    if small_button(
        ui,
        Id::new("vars_add_coll"),
        Rect::from_center_size(pos2(hr.max.x - 20.0, hr.center().y), vec2(24.0, 24.0)),
        Icon::Plus,
        "Create collection",
    ) {
        create_collection(app, ui, vars, st);
    }
    egui::ScrollArea::vertical().id_salt("vars_collections").auto_shrink([false, false]).show(ui, |ui| {
        for c in &vars.collections {
            let (r, resp) = ui.allocate_exact_size(vec2(SIDEBAR_W, 30.0), Sense::click());
            let r = r.shrink2(vec2(8.0, 1.0));
            let selected = st.collection.as_deref() == Some(c.id.as_str());
            if selected {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.selected_row);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
            }
            let target = Rename::Collection(c.id.clone());
            let count = vars.variables.iter().filter(|v| v.collection == c.id).count();
            ui.painter().text(pos2(r.max.x - 8.0, r.center().y), Align2::RIGHT_CENTER, count.to_string(), regular(11.0), t.text_tertiary);
            let name_r = Rect::from_min_max(pos2(r.min.x + 8.0, r.min.y + 4.0), pos2(r.max.x - 32.0, r.max.y - 4.0));
            if st.rename.as_ref() == Some(&target) {
                if let Some(name) = rename_field(ui, st, &target, &c.name, name_r, regular(11.0)) {
                    app.run("variable.renameCollection", json!({"collection": c.id, "name": name}));
                }
            } else {
                let font = if selected { semibold(11.0) } else { regular(11.0) };
                ui.painter().with_clip_rect(name_r).text(name_r.left_center(), Align2::LEFT_CENTER, &c.name, font, t.text);
            }
            if resp.clicked() {
                if !selected {
                    st.group = None;
                }
                st.collection = Some(c.id.clone());
            }
            if resp.double_clicked() {
                start_rename(ui, st, target.clone());
            }
            resp.context_menu(|ui| {
                if menu_item(ui, "Rename", "") {
                    start_rename(ui, st, target.clone());
                    ui.close();
                }
                if menu_item(ui, "Delete collection", "") {
                    app.run("variable.deleteCollection", json!({"collection": c.id}));
                    ui.close();
                }
            });
            if selected {
                group_rows(app, ui, vars, c, st);
            }
        }
    });
}

/// Every group path in `names` with its ancestors ("a/b/c" → "a", "a/b"), sorted.
fn groups<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let mut path = group_of(name);
        while !path.is_empty() {
            if !out.iter().any(|g| g == path) {
                out.push(path.to_string());
            }
            path = group_of(path);
        }
    }
    out.sort();
    out
}

/// "All variables" and the collection's groups (indented by depth): picking one filters the
/// table; groups can be renamed (every variable in them follows) or deleted.
fn group_rows(app: &mut FigApp, ui: &mut Ui, vars: &Variables, coll: &Collection, st: &mut Editor) {
    let t = Tokens::get(ui.ctx());
    let names: Vec<&str> = vars.variables.iter().filter(|v| v.collection == coll.id).map(|v| v.name.as_str()).collect();
    let list = groups(names.iter().copied());
    if list.is_empty() {
        return;
    }
    let rows = std::iter::once(None).chain(list.iter().map(Some));
    for g in rows {
        let (r, resp) = ui.allocate_exact_size(vec2(SIDEBAR_W, 26.0), Sense::click());
        let r = r.shrink2(vec2(8.0, 0.0));
        let on = st.group.as_ref() == g;
        if on {
            ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
        } else if resp.hovered() {
            ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, t.border), StrokeKind::Inside);
        }
        let depth = g.map_or(0, |g| g.matches('/').count());
        let x = r.min.x + 20.0 + depth as f32 * 12.0;
        let label = g.map_or("All variables", |g| g.rsplit('/').next().unwrap_or(g));
        let count = names.iter().filter(|n| g.is_none_or(|g| n.starts_with(&format!("{g}/")))).count();
        ui.painter().text(pos2(r.max.x - 8.0, r.center().y), Align2::RIGHT_CENTER, count.to_string(), regular(10.0), t.text_tertiary);
        let name_r = Rect::from_min_max(pos2(x, r.min.y + 3.0), pos2(r.max.x - 28.0, r.max.y - 3.0));
        let Some(g) = g else {
            ui.painter().text(name_r.left_center(), Align2::LEFT_CENTER, label, regular(11.0), t.text_secondary);
            if resp.clicked() {
                st.group = None;
            }
            continue;
        };
        let target = Rename::Group(coll.id.clone(), g.clone());
        if st.rename.as_ref() == Some(&target) {
            if let Some(new) = rename_field(ui, st, &target, label, name_r, regular(11.0)) {
                rename_group(app, vars, coll, g, &new, st);
            }
        } else {
            ui.painter().with_clip_rect(name_r).text(name_r.left_center(), Align2::LEFT_CENTER, label, regular(11.0), t.text);
        }
        if resp.clicked() {
            st.group = Some(g.clone());
        }
        if resp.double_clicked() {
            start_rename(ui, st, target.clone());
        }
        resp.context_menu(|ui| {
            if menu_item(ui, "Rename group", "") {
                start_rename(ui, st, target.clone());
                ui.close();
            }
            if menu_item(ui, "Delete group", "") {
                let steps: Vec<serde_json::Value> = vars
                    .variables
                    .iter()
                    .filter(|v| v.collection == coll.id && v.name.starts_with(&format!("{g}/")))
                    .map(|v| json!({"command": "variable.delete", "params": {"variable": v.id}}))
                    .collect();
                app.run("command.batch", json!({"label": "Delete group", "commands": steps}));
                if st.group.as_ref().is_some_and(|s| s == g || s.starts_with(&format!("{g}/"))) {
                    st.group = None;
                }
                ui.close();
            }
        });
    }
}

/// Rename the last part of group `g` to `new`: every variable under it moves along (one undo step).
fn rename_group(app: &mut FigApp, vars: &Variables, coll: &Collection, g: &str, new: &str, st: &mut Editor) {
    let new = new.trim().replace('/', "-");
    if new.is_empty() {
        return;
    }
    let renamed = match g.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{new}"),
        None => new,
    };
    let prefix = format!("{g}/");
    let steps: Vec<serde_json::Value> = vars
        .variables
        .iter()
        .filter(|v| v.collection == coll.id && v.name.starts_with(&prefix))
        .map(|v| json!({"command": "variable.update", "params": {"variable": v.id, "name": format!("{renamed}/{}", &v.name[prefix.len()..])}}))
        .collect();
    if app.run("command.batch", json!({"label": "Rename group", "commands": steps})).is_some()
        && st.group.as_deref().is_some_and(|s| s == g || s.starts_with(&prefix))
    {
        st.group = st.group.as_ref().map(|s| format!("{renamed}{}", &s[g.len()..]));
    }
}

fn create_collection(app: &mut FigApp, ui: &Ui, vars: &Variables, st: &mut Editor) {
    let name = unique("Collection", |n| vars.collections.iter().any(|c| c.name == n));
    if let Some(id) =
        app.run("variable.createCollection", json!({"name": name, "modes": ["Mode 1"]})).and_then(|r| r["id"].as_str().map(str::to_string))
    {
        st.collection = Some(id.clone());
        start_rename(ui, st, Rename::Collection(id));
    }
}

/// The group part of a variable name ("space/md" → "space"; "" when ungrouped).
fn group_of(name: &str) -> &str {
    name.rsplit_once('/').map(|(g, _)| g).unwrap_or("")
}

fn table(app: &mut FigApp, ui: &mut Ui, vars: &Variables, st: &mut Editor) {
    let t = Tokens::get(ui.ctx());
    let Some(coll) = st.collection.as_ref().and_then(|c| vars.collection(c)).cloned() else {
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.vertical(|ui| {
                widgets::label(
                    ui,
                    "Variables store reusable values (colours, numbers, strings, booleans) with modes such as Light and Dark.",
                    t.text_secondary,
                );
                ui.add_space(12.0);
                if widgets::button(ui, "Create collection", true, true).clicked() {
                    create_collection(app, ui, vars, st);
                }
            });
        });
        return;
    };
    // Search.
    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let field = Rect::from_min_size(pos2(sr.min.x + 16.0, sr.center().y - 12.0), vec2(220.0, 24.0));
    ui.painter().rect_filled(field, CornerRadius::same(5), t.field);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(field.min.x + 13.0, field.center().y), vec2(12.0, 12.0)), Icon::Search, t.icon_secondary);
    let mut child = ui.new_child(
        UiBuilder::new().max_rect(Rect::from_min_max(pos2(field.min.x + 24.0, field.min.y + 4.0), pos2(field.max.x - 4.0, field.max.y - 4.0))),
    );
    child.add(
        egui::TextEdit::singleline(&mut st.query)
            .frame(egui::Frame::NONE)
            .hint_text("Search")
            .font(regular(11.0))
            .vertical_align(egui::Align::Center),
    );
    let q = st.query.trim().to_lowercase();
    let in_group = |v: &Variable| st.group.as_ref().is_none_or(|g| v.name.starts_with(&format!("{g}/")));
    let mut list: Vec<&Variable> =
        vars.variables.iter().filter(|v| v.collection == coll.id && in_group(v) && (q.is_empty() || v.name.to_lowercase().contains(&q))).collect();
    list.sort_by(|a, b| group_of(&a.name).cmp(group_of(&b.name)));
    let width = (NAME_W + coll.modes.len() as f32 * MODE_W + 40.0).max(ui.available_width());
    egui::ScrollArea::both().id_salt("vars_table").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        header_row(app, ui, &coll, st, width);
        let mut group: Option<&str> = None;
        for v in &list {
            let g = group_of(&v.name);
            if group != Some(g) {
                group = Some(g);
                if !g.is_empty() {
                    let (r, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
                    ui.painter().text(pos2(r.min.x + 16.0, r.center().y + 2.0), Align2::LEFT_CENTER, g, semibold(11.0), t.text_secondary);
                }
            }
            var_row(app, ui, vars, &coll, v, st, width);
        }
        if list.is_empty() && !q.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
            ui.painter().text(pos2(r.min.x + 16.0, r.center().y), Align2::LEFT_CENTER, "No matching variables", regular(11.0), t.text_secondary);
        }
        create_row(app, ui, vars, &coll, st);
    });
}

fn header_row(app: &mut FigApp, ui: &mut Ui, coll: &Collection, st: &mut Editor, width: f32) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
    ui.painter().text(pos2(r.min.x + 16.0, r.center().y), Align2::LEFT_CENTER, "Name", regular(11.0), t.text_secondary);
    for (i, m) in coll.modes.iter().enumerate() {
        let x = r.min.x + NAME_W + i as f32 * MODE_W;
        let cell = Rect::from_min_max(pos2(x + 4.0, r.min.y + 4.0), pos2(x + MODE_W - 8.0, r.max.y - 4.0));
        let target = Rename::Mode(coll.id.clone(), m.id.clone());
        if st.rename.as_ref() == Some(&target) {
            if let Some(name) = rename_field(ui, st, &target, &m.name, cell.shrink2(vec2(4.0, 2.0)), regular(11.0)) {
                app.run("variable.renameMode", json!({"collection": coll.id, "mode": m.id, "name": name}));
            }
            continue;
        }
        let resp = ui.interact(cell, Id::new(("vars_mode", &m.id)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(cell, CornerRadius::same(4), t.hover);
        }
        ui.painter().with_clip_rect(cell).text(
            pos2(cell.min.x + 4.0, cell.center().y),
            Align2::LEFT_CENTER,
            &m.name,
            regular(11.0),
            t.text_secondary,
        );
        if resp.double_clicked() {
            start_rename(ui, st, target.clone());
        }
        resp.context_menu(|ui| {
            if menu_item(ui, "Rename mode", "") {
                start_rename(ui, st, target.clone());
                ui.close();
            }
            if coll.modes.len() > 1 && menu_item(ui, "Delete mode", "") {
                app.run("variable.removeMode", json!({"collection": coll.id, "mode": m.id}));
                ui.close();
            }
        });
    }
    let x = r.min.x + NAME_W + coll.modes.len() as f32 * MODE_W;
    if small_button(ui, Id::new("vars_add_mode"), Rect::from_center_size(pos2(x + 14.0, r.center().y), vec2(24.0, 24.0)), Icon::Plus, "Add mode") {
        let name = unique("Mode", |n| coll.modes.iter().any(|m| m.name == n));
        if let Some(id) = app.run("variable.addMode", json!({"collection": coll.id, "name": name})).and_then(|r| r["id"].as_str().map(str::to_string))
        {
            start_rename(ui, st, Rename::Mode(coll.id.clone(), id));
        }
    }
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, t.border));
}

fn var_row(app: &mut FigApp, ui: &mut Ui, vars: &Variables, coll: &Collection, v: &Variable, st: &mut Editor, width: f32) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    if resp.hovered() || st.details.as_deref() == Some(v.id.as_str()) {
        ui.painter().rect_filled(r, CornerRadius::ZERO, t.hover);
    }
    kind_glyph(ui, vars, v, coll, Rect::from_center_size(pos2(r.min.x + 24.0, r.center().y), vec2(14.0, 14.0)));
    let name_r = Rect::from_min_max(pos2(r.min.x + 40.0, r.min.y + 8.0), pos2(r.min.x + NAME_W - 8.0, r.max.y - 8.0));
    let target = Rename::Variable(v.id.clone());
    if st.rename.as_ref() == Some(&target) {
        if let Some(name) = rename_field(ui, st, &target, &v.name, name_r, regular(11.0)) {
            app.run("variable.update", json!({"variable": v.id, "name": name}));
        }
    } else {
        // Grouped names show their last part under the group's header.
        let short = v.name.rsplit('/').next().unwrap_or(&v.name);
        ui.painter().with_clip_rect(name_r).text(name_r.left_center(), Align2::LEFT_CENTER, short, regular(11.0), t.text);
    }
    if resp.double_clicked() && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < name_r.max.x) {
        start_rename(ui, st, target.clone());
    }
    for (i, m) in coll.modes.iter().enumerate() {
        let x = r.min.x + NAME_W + i as f32 * MODE_W;
        value_cell(
            app,
            ui,
            vars,
            v,
            coll,
            &m.id,
            Rect::from_min_max(pos2(x + 4.0, r.center().y - 12.0), pos2(x + MODE_W - 8.0, r.center().y + 12.0)),
        );
    }
    resp.context_menu(|ui| {
        if menu_item(ui, "Edit variable", "") {
            st.details = Some(v.id.clone());
            ui.close();
        }
        if menu_item(ui, "Rename", "") {
            start_rename(ui, st, target.clone());
            ui.close();
        }
        if menu_item(ui, "Duplicate", "") {
            app.run("variable.duplicate", json!({"variable": v.id}));
            ui.close();
        }
        if menu_item(ui, "Delete variable", "") {
            app.run("variable.delete", json!({"variable": v.id}));
            ui.close();
        }
    });
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, t.border));
}

/// The type marker before a variable's name: its colour (default mode), or a type glyph.
fn kind_glyph(ui: &Ui, vars: &Variables, v: &Variable, coll: &Collection, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    match v.kind {
        VariableType::Color => {
            let modes = coll.modes.first().map(|m| [(coll.id.clone(), m.id.clone())].into_iter().collect()).unwrap_or_default();
            if let Some(VariableValue::Color(c)) = vars.resolve(&v.id, &modes) {
                widgets::paint_swatch(p, rect, &figcraft_color::Paint::solid(c));
            }
        }
        VariableType::Number => {
            p.text(rect.center(), Align2::CENTER_CENTER, "#", semibold(12.0), t.icon_secondary);
        }
        VariableType::String => icons::paint(p, rect, Icon::Text, t.icon_secondary),
        VariableType::Boolean => {
            let track = Rect::from_center_size(rect.center(), vec2(14.0, 8.0));
            p.rect_stroke(track, CornerRadius::same(4), Stroke::new(1.2, t.icon_secondary), StrokeKind::Inside);
            p.circle_filled(pos2(track.max.x - 4.0, track.center().y), 2.0, t.icon_secondary);
        }
    }
}

/// `val` as a `variable.set` value.
fn value_json(val: &VariableValue) -> Value {
    match val {
        VariableValue::Color(c) => json!(c.to_css_hex()),
        VariableValue::Number(x) => json!(x),
        VariableValue::String(s) => json!(s),
        VariableValue::Boolean(b) => json!(b),
        VariableValue::Alias(a) => json!({"alias": a}),
    }
}

fn set(app: &mut FigApp, v: &Variable, mode: &str, value: Value) {
    app.run("variable.set", json!({"variable": v.id, "mode": mode, "value": value}));
}

/// One value cell: an editor for the value, or the aliased variable's chip; the variable button
/// on the right makes it an alias.
fn value_cell(app: &mut FigApp, ui: &mut Ui, vars: &Variables, v: &Variable, coll: &Collection, mode: &str, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    let Some(val) = v.values.get(mode) else { return };
    let id = Id::new(("var_cell", &v.id, mode));
    // The value editor leaves room for the alias button.
    let body = Rect::from_min_max(rect.min, pos2(rect.max.x - 26.0, rect.max.y));
    let alias_btn = Rect::from_center_size(pos2(rect.max.x - 11.0, rect.center().y), vec2(22.0, 22.0));
    let hovered = ui.rect_contains_pointer(rect.expand2(vec2(4.0, 6.0)));
    match val {
        VariableValue::Alias(target) => {
            let full = vars.get(target).map(|x| x.name.as_str()).unwrap_or("Missing variable");
            let resp = ui.interact(body, id.with("alias"), Sense::click()).on_hover_text("Change alias");
            ui.painter().rect_filled(body, CornerRadius::same(5), t.field);
            let chip = body.shrink2(vec2(3.0, 3.0));
            ui.painter().rect_filled(chip, CornerRadius::same(4), t.hover);
            ui.painter().rect_stroke(
                chip,
                CornerRadius::same(4),
                Stroke::new(1.0, if resp.hovered() { t.border_strong } else { t.border }),
                StrokeKind::Inside,
            );
            icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(chip.min.x + 11.0, chip.center().y), vec2(12.0, 12.0)),
                Icon::Variable,
                t.icon_secondary,
            );
            let text_r = Rect::from_min_max(pos2(chip.min.x + 21.0, chip.min.y), pos2(chip.max.x - 4.0, chip.max.y));
            ui.painter().with_clip_rect(text_r).text(text_r.left_center(), Align2::LEFT_CENTER, full, regular(11.0), t.text);
            if resp.clicked() {
                crate::variable_ui::open_alias(ui, &v.id, mode, v.kind, body.left_bottom());
            }
            // Detach: keep the value it resolves to now.
            if hovered && small_button(ui, id.with("detach"), alias_btn, Icon::Detach, "Detach alias") {
                let modes = [(coll.id.clone(), mode.to_string())].into_iter().collect();
                if let Some(resolved) = vars.resolve(&v.id, &modes) {
                    set(app, v, mode, value_json(&resolved));
                }
            }
            return;
        }
        VariableValue::Color(c) => {
            ui.painter().rect_filled(body, CornerRadius::same(5), t.field);
            let sw = Rect::from_center_size(pos2(body.min.x + 13.0, body.center().y), vec2(14.0, 14.0));
            widgets::paint_swatch(ui.painter(), sw, &figcraft_color::Paint::solid(*c));
            if ui.interact(sw.expand(3.0), id.with("swatch"), Sense::click()).on_hover_text("Edit colour").clicked() {
                app.ui.picker = Some(PickerTarget::Variable { id: v.id.clone(), mode: mode.to_string() });
            }
            let hex_r = Rect::from_min_max(pos2(sw.max.x + 8.0, body.min.y + 4.0), pos2(body.max.x - 44.0, body.max.y - 4.0));
            if let Some(h) = widgets::text_field_inline(ui, id.with("hex"), &c.to_hex(), hex_r, regular(11.0))
                && let Some(nc) = figcraft_color::Color::from_hex(&h)
            {
                // Six digits keep the alpha; eight set it.
                let nc = if h.trim().trim_start_matches('#').len() == 8 { nc } else { nc.with_alpha(c.a) };
                set(app, v, mode, json!(nc.to_css_hex()));
            }
            let pct = format!("{}%", (c.a * 100.0).round());
            ui.painter().text(pos2(body.max.x - 8.0, body.center().y), Align2::RIGHT_CENTER, pct, regular(11.0), t.text_secondary);
        }
        VariableValue::Number(x) => {
            let mut child = ui.new_child(UiBuilder::new().max_rect(body));
            match widgets::number_field(&mut child, id.with("num"), Lead::None, Some(*x), "", body.width(), 1.0) {
                Some(FieldEdit::Set(n)) => set(app, v, mode, json!(n)),
                Some(FieldEdit::Scrub(n, done)) => {
                    crate::typography::live(app, "Edit variable", "variable.set", json!({"variable": v.id, "mode": mode, "value": n}), done)
                }
                None => {}
            }
        }
        VariableValue::String(s) => {
            let mut child = ui.new_child(UiBuilder::new().max_rect(body));
            if let Some(n) = widgets::text_field(&mut child, id.with("str"), s, body.width(), "") {
                set(app, v, mode, json!(n));
            }
        }
        VariableValue::Boolean(b) => {
            let mut child = ui.new_child(UiBuilder::new().max_rect(body.shrink2(vec2(6.0, 0.0))));
            if widgets::checkbox(&mut child, *b, if *b { "True" } else { "False" }) {
                set(app, v, mode, json!(!b));
            }
        }
    }
    if hovered && small_button(ui, id.with("mkalias"), alias_btn, Icon::Variable, "Create alias") {
        crate::variable_ui::open_alias(ui, &v.id, mode, v.kind, body.left_bottom());
    }
}

fn create_row(app: &mut FigApp, ui: &mut Ui, vars: &Variables, coll: &Collection, st: &mut Editor) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let resp = widgets::button(ui, "+ Create variable", false, true);
        egui::Popup::menu(&resp).show(|ui| {
            for kind in TYPES {
                let label = match kind {
                    VariableType::Color => "Color",
                    VariableType::Number => "Number",
                    VariableType::String => "String",
                    VariableType::Boolean => "Boolean",
                };
                if menu_item(ui, label, "") {
                    let name = unique(label, |n| vars.variables.iter().any(|v| v.collection == coll.id && v.name == n));
                    let params = json!({"collection": coll.id, "name": name, "type": kind.id()});
                    if let Some(id) = app.run("variable.create", params).and_then(|r| r["id"].as_str().map(str::to_string)) {
                        st.query.clear();
                        start_rename(ui, st, Rename::Variable(id));
                    }
                    ui.close();
                }
            }
        });
    });
    ui.add_space(12.0);
}

/// The details of one variable: name, description and where pickers offer it.
fn details(app: &mut FigApp, ctx: &Context, vars: &Variables, st: &mut Editor) {
    let Some(v) = st.details.as_ref().and_then(|id| vars.get(id)).cloned() else {
        st.details = None;
        return;
    };
    let t = Tokens::get(ctx);
    let mut open = true;
    egui::Window::new("Edit variable")
        .id(Id::new("variable_details"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.panel).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_width(260.0);
            widgets::label(ui, "Name", t.text_secondary);
            if let Some(n) = widgets::text_field(ui, Id::new(("var_detail_name", &v.id)), &v.name, 260.0, "") {
                app.run("variable.update", json!({"variable": v.id, "name": n}));
            }
            ui.add_space(6.0);
            widgets::label(ui, "Description", t.text_secondary);
            if let Some(d) = widgets::text_field(ui, Id::new(("var_detail_desc", &v.id)), &v.description, 260.0, "What it's for") {
                app.run("variable.update", json!({"variable": v.id, "description": d}));
            }
            let scopes = scopes_for(v.kind);
            if scopes.is_empty() {
                return;
            }
            ui.add_space(10.0);
            widgets::label(ui, "Scoping: where pickers offer it", t.text_secondary);
            // "All" off hides it everywhere; on, from everywhere it fits.
            if widgets::checkbox(ui, v.scopes.is_none(), "All supported properties") {
                app.run("variable.update", json!({"variable": v.id, "scopes": if v.scopes.is_none() { json!([]) } else { Value::Null }}));
            }
            let on: Vec<&str> = scopes.iter().map(|(id, _)| *id).filter(|id| v.in_scope(id)).collect();
            for (sid, label) in scopes {
                let checked = on.contains(sid);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    if widgets::checkbox(ui, checked, label) {
                        let mut list = on.clone();
                        if checked {
                            list.retain(|x| x != sid);
                        } else {
                            list.push(sid);
                        }
                        app.run("variable.update", json!({"variable": v.id, "scopes": list}));
                    }
                });
            }
        });
    if !open {
        st.details = None;
    }
}

#[cfg(test)]
mod tests {
    use super::groups;

    #[test]
    fn groups_include_parents_once_and_sorted() {
        let names = ["space/sm", "space/md", "color/brand/blue", "color/text", "plain", "color/brand/red"];
        assert_eq!(groups(names.into_iter()), ["color", "color/brand", "space"]);
        assert!(groups(["a", "b"].into_iter()).is_empty());
    }
}
