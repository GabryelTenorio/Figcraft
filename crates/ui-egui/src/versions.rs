//! The Version history panel, shown in the left sidebar in place of Pages and Layers: save a
//! version (title and description), the history newest first under the current version, and per
//! version Restore, Rename, Duplicate and Delete. Clicking a version shows it on the canvas
//! (read-only, with a banner to restore it or go back); clicking Current version goes back.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::FigApp;
use crate::a11y::Tip;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets;

/// Version panel state.
#[derive(Default)]
pub struct VersionUi {
    pub open: bool,
    /// The save form (new version) or the edit form of a version.
    pub form: Option<Form>,
    /// The version waiting for delete confirmation.
    pub confirm_delete: Option<String>,
    /// Runs of autosaves shown unfolded (keyed by their newest version's id).
    pub open_groups: std::collections::BTreeSet<String>,
}

/// Title and description being written; `id` is the version being edited (`None` = a new one).
pub struct Form {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    focus: bool,
}

impl Form {
    fn new(id: Option<String>, name: &str, description: &str) -> Self {
        Self { id, name: name.to_string(), description: description.to_string(), focus: true }
    }
}

/// Open the panel (File ▸ Show version history).
pub fn open(app: &mut FigApp) {
    app.ui.versions.open = true;
    app.ui.left_collapsed = false;
    app.ui.left_tab = crate::LeftTab::File;
}

/// Open the panel with the save form (File ▸ Save to version history…).
pub fn start_save(app: &mut FigApp) {
    open(app);
    app.ui.versions.confirm_delete = None;
    app.ui.versions.form = Some(Form::new(None, "", ""));
}

/// One version as listed.
struct Row {
    id: String,
    name: String,
    description: String,
    time: u64,
    auto: bool,
}

enum Action {
    Close,
    Preview(String),
    ExitPreview,
    ToggleGroup(String),
    New,
    Submit,
    CancelForm,
    Restore(String),
    Edit(String),
    Duplicate(String),
    AskDelete(String),
    Delete(String),
    CancelDelete,
}

const DOT_X: f32 = 20.0;
const TEXT_X: f32 = 36.0;

pub fn panel(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let mut action = None;

    // Header: title, save, close.
    let (head, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
    ui.painter().text(pos2(head.min.x + 16.0, head.center().y), Align2::LEFT_CENTER, "Version history", semibold(13.0), t.text);
    let mut bar =
        ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(12.0, 12.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    if icons::button(&mut bar, Icon::Close, 24.0, false, "Close version history").clicked() {
        action = Some(Action::Close);
    }
    let new_open = app.ui.versions.form.as_ref().is_some_and(|f| f.id.is_none());
    let tip = format!("Save version  {}", crate::toolbar::pretty_shortcut("Alt+Cmd+S"));
    if icons::button(&mut bar, Icon::Plus, 24.0, new_open, &tip).clicked() {
        action = Some(Action::New);
    }
    ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, t.border));

    if let Some(f) = app.ui.versions.form.as_mut().filter(|f| f.id.is_none()) {
        ui.add_space(12.0);
        if let Some(a) = form(ui, f, "Save", 16.0) {
            action = Some(a);
        }
        ui.add_space(12.0);
        widgets::separator(ui);
    }

    let rows: Vec<Row> = app
        .session
        .ds()
        .map(|ds| {
            ds.versions
                .iter()
                .rev()
                .map(|v| Row { id: v.id.clone(), name: v.name.clone(), description: v.description.clone(), time: v.time, auto: v.auto })
                .collect()
        })
        .unwrap_or_default();
    let previewing = app.session.ds().ok().and_then(|ds| ds.preview.as_ref().map(|p| p.id.clone()));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Rows touch so the timeline is continuous.
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(4.0);
        if current_row(ui, !rows.is_empty(), previewing.is_none()) {
            action = Some(Action::ExitPreview);
        }
        let last = rows.len().saturating_sub(1);
        let mut i = 0;
        let mut group_end = 0;
        while let Some(row) = rows.get(i) {
            // Two or more autosaves in a row fold into one line (open while one of them is shown).
            if i >= group_end {
                let run = rows.iter().skip(i).take_while(|r| r.auto).count();
                if run >= 2 {
                    group_end = i + run;
                    let open = app.ui.versions.open_groups.contains(&row.id)
                        || rows.iter().skip(i).take(run).any(|r| previewing.as_deref() == Some(r.id.as_str()));
                    if autosave_group(ui, run, open, group_end > last && !open) {
                        action = Some(Action::ToggleGroup(row.id.clone()));
                    }
                    if !open {
                        i = group_end;
                        continue;
                    }
                }
            }
            let a = if app.ui.versions.confirm_delete.as_deref() == Some(row.id.as_str()) {
                inline_row(ui, i == last, |ui| confirm_row(ui, row))
            } else if let Some(f) = app.ui.versions.form.as_mut().filter(|f| f.id.as_deref() == Some(row.id.as_str())) {
                inline_row(ui, i == last, |ui| form(ui, f, "Save", TEXT_X))
            } else {
                version_row(ui, row, i == last, previewing.as_deref() == Some(row.id.as_str()))
            };
            if a.is_some() {
                action = a;
            }
            i += 1;
        }
        if rows.is_empty() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("No saved versions yet. Save a version to mark a point in your work you can come back to.")
                            .font(regular(11.0))
                            .color(t.text_secondary),
                    )
                    .wrap(),
                );
            });
        }
        ui.add_space(16.0);
    });
    if let Some(a) = action {
        act(app, a, &rows);
    }
}

fn act(app: &mut FigApp, a: Action, rows: &[Row]) {
    let st = &mut app.ui.versions;
    let name_of = |id: &str| rows.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default();
    match a {
        Action::Close => {
            *st = VersionUi::default();
            app.run("version.exitPreview", json!({}));
        }
        Action::Preview(id) => {
            app.run("version.preview", json!({"id": id}));
        }
        Action::ExitPreview => {
            app.run("version.exitPreview", json!({}));
        }
        Action::ToggleGroup(id) => {
            if !st.open_groups.remove(&id) {
                st.open_groups.insert(id);
            }
        }
        Action::New => {
            st.confirm_delete = None;
            st.form = if st.form.as_ref().is_some_and(|f| f.id.is_none()) { None } else { Some(Form::new(None, "", "")) };
        }
        Action::CancelForm => st.form = None,
        Action::CancelDelete => st.confirm_delete = None,
        Action::Edit(id) => {
            st.confirm_delete = None;
            st.form = rows.iter().find(|r| r.id == id).map(|r| Form::new(Some(id.clone()), &r.name, &r.description));
        }
        Action::AskDelete(id) => {
            st.form = None;
            st.confirm_delete = Some(id);
        }
        Action::Submit => {
            let Some(f) = st.form.take() else { return };
            let (cmd, params) = match &f.id {
                Some(id) => ("version.rename", json!({"id": id, "name": f.name.trim(), "description": f.description.trim()})),
                None => ("file.saveVersion", json!({"name": f.name.trim(), "description": f.description.trim()})),
            };
            if app.run(cmd, params).is_none() {
                app.ui.versions.form = Some(f);
            }
        }
        Action::Delete(id) => {
            st.confirm_delete = None;
            app.run("version.delete", json!({"id": id}));
        }
        Action::Restore(id) => {
            if app.run("version.restore", json!({"id": id})).is_some() {
                app.toast(format!("Restored “{}”", name_of(&id)));
            }
        }
        Action::Duplicate(id) => {
            if app.run("version.duplicate", json!({"id": id})).is_some() {
                app.toast(format!("Opened “{}” in a new tab", name_of(&id)));
            }
        }
    }
}

/// Title and description fields with Cancel / `submit` buttons, `indent` from the left.
fn form(ui: &mut Ui, f: &mut Form, submit: &str, indent: f32) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    let field = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui) -> egui::Response| {
        egui::Frame::NONE.fill(t.field).corner_radius(CornerRadius::same(5)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, add).inner
    };
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 16.0);
            let w = ui.available_width() - 16.0;
            let title = field(ui, &mut |ui| {
                ui.add(egui::TextEdit::singleline(&mut f.name).hint_text("Title").font(regular(11.0)).frame(egui::Frame::NONE).desired_width(w))
            });
            if std::mem::take(&mut f.focus) {
                title.request_focus();
            }
            ui.add_space(8.0);
            let desc = field(ui, &mut |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut f.description)
                        .hint_text("Describe what changed")
                        .font(regular(11.0))
                        .frame(egui::Frame::NONE)
                        .desired_rows(3)
                        .desired_width(w),
                )
            });
            let (enter, cmd_enter, esc) = ui.input(|i| {
                (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Enter) && i.modifiers.command, i.key_pressed(egui::Key::Escape))
            });
            let editing_existing = f.id.is_some();
            let can_submit = !editing_existing || !f.name.trim().is_empty();
            if esc && (title.has_focus() || title.lost_focus() || desc.has_focus() || desc.lost_focus()) {
                out = Some(Action::CancelForm);
            } else if can_submit && ((title.lost_focus() && enter) || (desc.has_focus() && cmd_enter)) {
                out = Some(Action::Submit);
            }
            ui.add_space(8.0);
            match buttons(ui, submit, can_submit) {
                Some(true) => out = Some(Action::Submit),
                Some(false) => out = Some(Action::CancelForm),
                None => {}
            }
        });
    });
    out
}

/// The timeline's line and dot for a row (`top`/`bottom`: the line continues above/below).
fn timeline(ui: &Ui, r: Rect, dot_y: f32, top: bool, bottom: bool, current: bool) {
    let t = Tokens::get(ui.ctx());
    let x = r.min.x + DOT_X;
    let p = ui.painter();
    let line = Stroke::new(1.0, t.border_strong);
    if top {
        p.line_segment([pos2(x, r.min.y), pos2(x, dot_y)], line);
    }
    if bottom {
        p.line_segment([pos2(x, dot_y), pos2(x, r.max.y)], line);
    }
    if current {
        p.circle_filled(pos2(x, dot_y), 4.0, t.accent);
    } else {
        p.circle(pos2(x, dot_y), 3.5, t.panel, line);
    }
}

/// The current version's row (`active` when it's what the canvas shows); true when clicked.
fn current_row(ui: &mut Ui, more: bool, active: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    if active {
        ui.painter().rect_filled(r, CornerRadius::ZERO, t.selected_row);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::ZERO, t.hover);
    }
    timeline(ui, r, r.center().y, false, more, true);
    ui.painter().text(pos2(r.min.x + TEXT_X, r.center().y), Align2::LEFT_CENTER, "Current version", semibold(11.0), t.text);
    !active && resp.on_hover_text("Back to the current version").clicked()
}

fn version_row(ui: &mut Ui, row: &Row, last: bool, previewing: bool) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let text_w = (w - TEXT_X - 36.0).max(40.0);
    // Up to three lines of description, then an ellipsis.
    let desc = (!row.description.is_empty()).then(|| {
        let mut job = egui::text::LayoutJob::simple(row.description.clone(), regular(11.0), t.text, text_w);
        job.wrap.max_rows = 3;
        ui.painter().layout_job(job)
    });
    let desc_h = desc.as_ref().map(|g| g.size().y + 4.0).unwrap_or(0.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 50.0 + desc_h), Sense::click());
    let hovered = ui.rect_contains_pointer(r);
    if previewing {
        ui.painter().rect_filled(r, CornerRadius::ZERO, t.selected_row);
    } else if hovered {
        ui.painter().rect_filled(r, CornerRadius::ZERO, t.hover);
    }
    let name_y = r.min.y + 17.0;
    timeline(ui, r, name_y, true, !last, false);
    let clip = Rect::from_min_max(pos2(r.min.x + TEXT_X, r.min.y), pos2(r.min.x + TEXT_X + text_w, r.max.y));
    let p = ui.painter().with_clip_rect(clip);
    p.text(pos2(clip.min.x, name_y), Align2::LEFT_CENTER, &row.name, semibold(11.0), t.text);
    p.text(pos2(clip.min.x, name_y + 16.0), Align2::LEFT_CENTER, crate::comments::ago_long(row.time), regular(11.0), t.text_secondary);
    if let Some(g) = desc {
        p.galley(pos2(clip.min.x, name_y + 26.0), g, t.text);
    }
    let mut out = None;
    // More menu (on hover), also on right-click.
    let br = Rect::from_center_size(pos2(r.max.x - 22.0, name_y), vec2(24.0, 24.0));
    let more = ui.interact(br, Id::new(("version_more", &row.id)), Sense::click());
    let menu_open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&more));
    if hovered || menu_open {
        if more.hovered() || menu_open {
            ui.painter().rect_filled(br, CornerRadius::same(5), t.pressed);
        }
        icons::paint(ui.painter(), br.shrink(5.0), Icon::Dots, t.icon);
    }
    let more = more.tip("More");
    let mut menu = |ui: &mut Ui| {
        for (label, a) in [
            ("Restore this version", Action::Restore(row.id.clone())),
            ("Rename", Action::Edit(row.id.clone())),
            ("Duplicate", Action::Duplicate(row.id.clone())),
            ("Delete", Action::AskDelete(row.id.clone())),
        ] {
            if crate::right::menu_item(ui, label, "") {
                out = Some(a);
                ui.close();
            }
        }
    };
    crate::right::menu_popup(&more).show(&mut menu);
    crate::right::context_popup(&resp).show(&mut menu);
    if resp.double_clicked() {
        out = Some(Action::Edit(row.id.clone()));
    } else if resp.clicked() && !previewing && out.is_none() {
        out = Some(Action::Preview(row.id.clone()));
    }
    out
}

/// While a version is shown: a banner over the canvas naming it, with Restore and Back.
pub fn preview_banner(app: &mut FigApp, ctx: &egui::Context, canvas: Rect) {
    let Some((id, name, time)) = app.session.ds().ok().and_then(|ds| ds.preview.as_ref().map(|p| (p.id.clone(), p.name.clone(), p.time))) else {
        return;
    };
    let t = Tokens::get(ctx);
    let mut action = None;
    egui::Area::new(Id::new("version_preview_banner"))
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(pos2(canvas.center().x, canvas.min.y + 12.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(t.panel)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(8))
                .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 12, spread: 0, color: t.shadow })
                .inner_margin(egui::Margin { left: 14, right: 8, top: 8, bottom: 8 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        icons::paint(
                            ui.painter(),
                            Rect::from_center_size(ui.cursor().min + vec2(7.0, 12.0), vec2(14.0, 14.0)),
                            Icon::Clock,
                            t.icon_secondary,
                        );
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new(format!("Viewing “{name}”")).font(semibold(11.0)).color(t.text));
                        ui.label(egui::RichText::new(crate::comments::ago_long(time)).font(regular(11.0)).color(t.text_secondary));
                        ui.add_space(4.0);
                        if widgets::button(ui, "Back to current", false, true).clicked() {
                            action = Some(false);
                        }
                        if widgets::button(ui, "Restore this version", true, true).clicked() {
                            action = Some(true);
                        }
                    });
                });
        });
    match action {
        Some(true) => {
            if app.run("version.restore", json!({"id": id})).is_some() {
                app.toast(format!("Restored “{name}”"));
            }
        }
        Some(false) => {
            app.run("version.exitPreview", json!({}));
        }
        None => {}
    }
}

/// The folded line for `n` autosaves in a row; true when clicked (fold / unfold).
fn autosave_group(ui: &mut Ui, n: usize, open: bool, last: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::ZERO, t.hover);
    }
    timeline(ui, r, r.center().y, true, !last, false);
    let chevron = Rect::from_center_size(pos2(r.min.x + TEXT_X + 6.0, r.center().y), vec2(12.0, 12.0));
    icons::paint(ui.painter(), chevron, if open { Icon::Chevron } else { Icon::ChevronRight }, t.icon_secondary);
    let label = format!("{n} autosave versions");
    ui.painter().text(pos2(chevron.max.x + 6.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.0), t.text_secondary);
    resp.on_hover_text(if open { "Fold autosaves" } else { "Show autosaves" }).clicked()
}

/// A row holding a form (rename, delete confirmation), with the timeline running past it.
fn inline_row(ui: &mut Ui, last: bool, add: impl FnOnce(&mut Ui) -> Option<Action>) -> Option<Action> {
    let top = ui.cursor().min.y;
    ui.add_space(8.0);
    let a = add(ui);
    ui.add_space(8.0);
    let r = Rect::from_min_max(pos2(ui.min_rect().min.x, top), pos2(ui.min_rect().max.x, ui.cursor().min.y));
    timeline(ui, r, top + 20.0, true, !last, false);
    a
}

fn confirm_row(ui: &mut Ui, row: &Row) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    egui::Frame::NONE.inner_margin(egui::Margin { left: TEXT_X as i8, right: 16, top: 4, bottom: 0 }).show(ui, |ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(format!("Delete “{}”? This can't be undone.", row.name)).font(regular(11.0)).color(t.text)).wrap(),
        );
        ui.add_space(8.0);
        out = buttons(ui, "Delete", true).map(|delete| if delete { Action::Delete(row.id.clone()) } else { Action::CancelDelete });
    });
    out
}

/// Right-aligned Cancel and `primary` buttons: `Some(true)` for the primary one, `Some(false)` for
/// Cancel.
fn buttons(ui: &mut Ui, primary: &str, enabled: bool) -> Option<bool> {
    let size = vec2(ui.available_width(), widgets::FIELD_H);
    ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let ok = widgets::button(ui, primary, true, enabled).clicked();
        let cancel = widgets::button(ui, "Cancel", false, true).clicked();
        if ok { Some(true) } else { cancel.then_some(false) }
    })
    .inner
}
