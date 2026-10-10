//! The style picker: the popover a Fill, Stroke, Effects or Layout grid section's Styles button
//! opens, laid out like the variable picker. Search; styles grouped by the folders of their
//! `/`-separated names (folders collapse); a list or, for colour styles, a grid of swatches. Click
//! applies a style; its edit button (or a right-click) edits the name, description and values; `+`
//! creates a style from the selection's current values; the detach button unlinks it. Colour slots
//! also list colour variables to bind (this file's, then each enabled library's).

use egui::{Align2, Color32, CornerRadius, Id, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use figcraft_color::Paint;
use figcraft_doc::{Document, GridKind, Node, NodeId, Style, StyleKind, VariableType};
use serde_json::json;

use crate::FigApp;
use crate::colorpicker::PickerTarget;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets;

/// Content width of the popover.
const W: f32 = 240.0;
/// Height of a list row.
const ROW_H: f32 = 26.0;
/// Side of a swatch in the grid view.
const SWATCH: f32 = 24.0;

/// A style slot on a layer: what a style picker lists and what a style chip shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleSlot {
    Fill,
    Stroke,
    Effect,
    Grid,
}

impl StyleSlot {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fill" => Some(StyleSlot::Fill),
            "stroke" => Some(StyleSlot::Stroke),
            "effect" => Some(StyleSlot::Effect),
            "grid" => Some(StyleSlot::Grid),
            _ => None,
        }
    }

    /// The slot's name in `style.apply {target}`, `style.detach {which}` and `variable.bind {field}`.
    pub fn key(self) -> &'static str {
        match self {
            StyleSlot::Fill => "fill",
            StyleSlot::Stroke => "stroke",
            StyleSlot::Effect => "effect",
            StyleSlot::Grid => "grid",
        }
    }

    fn linked(self, n: &Node) -> Option<String> {
        match self {
            StyleSlot::Fill => n.styles.fill.clone(),
            StyleSlot::Stroke => n.styles.stroke.clone(),
            StyleSlot::Effect => n.styles.effect.clone(),
            StyleSlot::Grid => n.styles.grid.clone(),
        }
    }

    fn paints(self) -> bool {
        matches!(self, StyleSlot::Fill | StyleSlot::Stroke)
    }

    fn title(self) -> &'static str {
        match self {
            StyleSlot::Fill | StyleSlot::Stroke => "Color styles",
            StyleSlot::Effect => "Effect styles",
            StyleSlot::Grid => "Grid styles",
        }
    }

    /// `style.create {type}` for this slot (taken from the layer's fills, strokes, effects or grids).
    fn create_type(self) -> &'static str {
        match self {
            StyleSlot::Fill => "paint",
            StyleSlot::Stroke => "stroke",
            StyleSlot::Effect => "effect",
            StyleSlot::Grid => "grid",
        }
    }

    /// The base of a new style's name ("Color 3").
    fn default_name(self) -> &'static str {
        match self {
            StyleSlot::Fill | StyleSlot::Stroke => "Color",
            StyleSlot::Effect => "Effect",
            StyleSlot::Grid => "Grid",
        }
    }

    fn holds(self, k: &StyleKind) -> bool {
        matches!(
            (k, self),
            (StyleKind::Paint { .. }, StyleSlot::Fill | StyleSlot::Stroke)
                | (StyleKind::Effect { .. }, StyleSlot::Effect)
                | (StyleKind::Grid { .. }, StyleSlot::Grid)
        )
    }

    fn icon(self) -> Icon {
        if self == StyleSlot::Grid { Icon::Grid } else { Icon::Effects }
    }
}

/// The style being edited, with its name and description as typed.
#[derive(Clone, Debug)]
struct Editing {
    id: String,
    name: String,
    description: String,
    /// Focus the name (and select it) on the next frame: a style that was just created.
    focus: bool,
}

/// The open picker.
#[derive(Clone, Debug)]
pub struct Picker {
    slot: StyleSlot,
    ids: Vec<NodeId>,
    /// Screen y of the button that opened it.
    y: f32,
    query: String,
    edit: Option<Editing>,
}

impl Picker {
    pub fn slot(&self) -> StyleSlot {
        self.slot
    }
}

/// Open the picker for `slot` of `ids`, level with the pointer.
pub fn open(app: &mut FigApp, ui: &Ui, ids: &[NodeId], slot: StyleSlot) {
    let y = ui.input(|i| i.pointer.interact_pos()).map_or(140.0, |p| p.y - 12.0);
    app.ui.style_picker = Some(Picker { slot, ids: ids.to_vec(), y, query: String::new(), edit: None });
}

/// Open the picker for `slot` of the selection (the control channel), optionally editing `style`.
pub fn open_for_selection(app: &mut FigApp, slot: StyleSlot, style: Option<&str>) {
    let ids = app.session.selection();
    let edit = style.and_then(|s| app.session.doc().ok()?.style(s).map(editing));
    app.ui.style_picker = Some(Picker { slot, ids, y: 140.0, query: String::new(), edit });
}

fn editing(s: &Style) -> Editing {
    Editing { id: s.id.clone(), name: s.name.clone(), description: s.description.clone(), focus: false }
}

/// The style `slot` of every one of `ids` is linked to, if they agree.
fn linked_style(doc: &Document, ids: &[NodeId], slot: StyleSlot) -> Option<String> {
    crate::right::common(doc, ids, |n| slot.linked(n)).flatten()
}

/// The applied style's name (click opens the picker) with a detach button.
pub fn chip(app: &mut FigApp, ui: &mut Ui, doc: &Document, ids: &[NodeId], slot: StyleSlot) {
    let t = Tokens::get(ui.ctx());
    let Some(name) = linked_style(doc, ids, slot).and_then(|s| doc.style(&s).map(|st| st.name.clone())) else { return };
    let mut open_it = false;
    crate::right::row(ui, 32.0, |ui| {
        let w = (ui.available_width() - 32.0).max(40.0);
        let (r, resp) = ui.allocate_exact_size(vec2(w, widgets::FIELD_H), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(14.0, 14.0)), Icon::Styles, t.icon_secondary);
        let galley = ui.painter().layout(name.clone(), regular(11.0), t.text, r.width() - 32.0);
        ui.painter().galley(pos2(r.min.x + 26.0, r.center().y - galley.size().y / 2.0), galley, t.text);
        open_it = resp.on_hover_text("Change style").clicked();
        if icons::button(ui, Icon::Detach, 24.0, false, "Detach style").clicked() {
            app.run("style.detach", json!({"ids": ids, "which": slot.key()}));
        }
    });
    if open_it {
        open(app, ui, ids, slot);
    }
}

/// What the user asked the picker to do this frame.
enum Act {
    Apply(String),
    Bind(crate::variable_ui::Pick),
    Create,
    Detach,
    Edit(String),
    Back,
    EditPaint(String, usize),
    UpdateFromSelection(String),
    Delete(String),
    ToggleFolder(String),
    ToggleView,
}

/// An icon button on the dark popover.
fn menu_icon(ui: &mut Ui, rect: Rect, icon: Icon, tip: &str, on: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, ui.id().with(("style_picker_btn", tip)), Sense::click()).on_hover_text(tip);
    if on || resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(5), t.menu_hover);
    }
    icons::paint(ui.painter(), rect.shrink(5.0), icon, t.menu_text);
    resp
}

/// A text button on the dark popover.
fn menu_button(ui: &mut Ui, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), regular(11.0), t.menu_text);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, widgets::FIELD_H), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(5), if resp.hovered() { t.menu_hover } else { Color32::TRANSPARENT });
    ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, t.menu_dim), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, regular(11.0), t.menu_text);
    resp
}

/// A dim caption on the popover.
fn caption(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(text).font(regular(10.0)).color(t.menu_dim));
}

/// The folder part of a style name ("Brand/Blue/500" → "Brand/Blue"), empty at the top level.
fn folder(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(f, _)| f.trim())
}

/// The last part of a style name ("Brand/Blue/500" → "500").
fn leaf(name: &str) -> &str {
    name.rsplit_once('/').map_or(name, |(_, l)| l.trim())
}

/// `base N` with the smallest N no style of the document is named.
fn next_name(doc: &Document, base: &str) -> String {
    (1..).map(|n| format!("{base} {n}")).find(|n| doc.styles.iter().all(|s| &s.name != n)).unwrap_or_else(|| base.to_string())
}

/// Draw a style's preview (its first paint, or the slot's icon) centred in `r`.
fn preview(ui: &Ui, r: Rect, s: &Style, slot: StyleSlot) {
    let t = Tokens::get(ui.ctx());
    match &s.kind {
        StyleKind::Paint { paints } => {
            let p = paints.last().cloned().unwrap_or_else(|| Paint::solid(figcraft_color::Color::TRANSPARENT));
            widgets::paint_swatch(ui.painter(), r, &p);
        }
        _ => icons::paint(ui.painter(), r, slot.icon(), t.menu_text),
    }
}

/// One style in the list: preview, name, a check when applied and an edit button on hover.
fn style_row(ui: &mut Ui, s: &Style, slot: StyleSlot, applied: bool, editable: bool, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(W, ROW_H), Sense::click());
    let edit_r = Rect::from_center_size(pos2(r.max.x - 14.0, r.center().y), vec2(22.0, 22.0));
    let hovered = resp.hovered() || ui.rect_contains_pointer(edit_r);
    if hovered {
        ui.painter().rect_filled(r, CornerRadius::same(5), t.menu_hover);
    }
    preview(ui, Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(14.0, 14.0)), s, slot);
    let galley = ui.painter().layout(leaf(&s.name).to_string(), regular(11.0), t.menu_text, W - 64.0);
    ui.painter().galley(pos2(r.min.x + 26.0, r.center().y - galley.size().y / 2.0), galley, t.menu_text);
    if hovered && editable {
        if menu_icon(ui, edit_r, Icon::Adjust, "Edit style", false).clicked() {
            *act = Some(Act::Edit(s.id.clone()));
            return;
        }
    } else if applied {
        icons::paint(ui.painter(), edit_r.shrink(5.0), Icon::Check, t.menu_text);
    }
    let resp = if s.description.is_empty() { resp } else { resp.on_hover_text(&s.description) };
    if resp.secondary_clicked() && editable {
        *act = Some(Act::Edit(s.id.clone()));
    } else if resp.clicked() {
        *act = Some(Act::Apply(s.id.clone()));
    }
}

/// Colour styles as a grid of swatches (name on hover).
fn swatch_grid(ui: &mut Ui, list: &[&Style], applied: Option<&str>, editable: impl Fn(&str) -> bool, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let per_row = ((W - 8.0 + 6.0) / (SWATCH + 6.0)).floor().max(1.0) as usize;
    for chunk in list.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            for s in chunk {
                let (r, resp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
                preview(ui, r, s, StyleSlot::Fill);
                if applied == Some(s.id.as_str()) {
                    ui.painter().rect_stroke(r.expand(2.0), CornerRadius::same(4), Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_stroke(r.expand(2.0), CornerRadius::same(4), Stroke::new(1.0, t.menu_text), egui::StrokeKind::Inside);
                }
                let resp = resp.on_hover_text(&s.name);
                if resp.secondary_clicked() && editable(&s.id) {
                    *act = Some(Act::Edit(s.id.clone()));
                } else if resp.clicked() {
                    *act = Some(Act::Apply(s.id.clone()));
                }
            }
        });
        ui.add_space(2.0);
    }
}

/// The style list: header, search, folders of styles and (colour slots) colour variables.
fn listing(app: &FigApp, ui: &mut Ui, doc: &Document, p: &mut Picker, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let slot = p.slot;
    let applied = linked_style(doc, &p.ids, slot);
    let grid = slot.paints() && app.ui.style_grid;
    // Header: title, then view toggle, detach and create.
    let (hr, _) = ui.allocate_exact_size(vec2(W, 24.0), Sense::hover());
    ui.painter().text(pos2(hr.min.x + 2.0, hr.center().y), Align2::LEFT_CENTER, slot.title(), semibold(11.0), t.menu_text);
    let mut x = hr.max.x - 24.0;
    let mut slot_rect = || {
        let r = Rect::from_min_size(pos2(x, hr.min.y), vec2(24.0, 24.0));
        x -= 26.0;
        r
    };
    if menu_icon(ui, slot_rect(), Icon::Plus, "Create style", false).clicked() {
        *act = Some(Act::Create);
    }
    if applied.is_some() && menu_icon(ui, slot_rect(), Icon::Detach, "Detach style", false).clicked() {
        *act = Some(Act::Detach);
    }
    if slot.paints() {
        let (icon, tip) = if grid { (Icon::ViewList, "Show as list") } else { (Icon::ViewGrid, "Show as grid") };
        if menu_icon(ui, slot_rect(), icon, tip, false).clicked() {
            *act = Some(Act::ToggleView);
        }
    }
    ui.add_space(4.0);
    let r = ui.add(egui::TextEdit::singleline(&mut p.query).hint_text("Search").font(regular(11.0)).desired_width(W));
    if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    ui.add_space(4.0);
    let q = p.query.trim().to_lowercase();
    let matches = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);
    // Styles by folder, folders in order of first appearance.
    let mut groups: Vec<(&str, Vec<&Style>)> = Vec::new();
    for s in doc.styles.iter().filter(|s| slot.holds(&s.kind) && matches(&s.name)) {
        let f = folder(&s.name);
        match groups.iter_mut().find(|(g, _)| *g == f) {
            Some((_, list)) => list.push(s),
            None => groups.push((f, vec![s])),
        }
    }
    groups.sort_by_key(|(f, _)| !f.is_empty());
    let editable = |id: &str| !figcraft_engine::library::is_remote_style(doc, id);
    let libs = if slot.paints() { crate::variable_ui::libraries(app, doc) } else { Vec::new() };
    let colors =
        if slot.paints() { crate::variable_ui::groups(doc, &libs, |_, v| v.kind == VariableType::Color && matches(&v.name)) } else { Vec::new() };
    egui::ScrollArea::vertical().id_salt("style_picker_scroll").max_height(320.0).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if groups.is_empty() {
            let any = doc.styles.iter().any(|s| slot.holds(&s.kind));
            caption(ui, if any { "No matching styles" } else { "No styles yet. Click + to create one from the selection." });
        }
        for (f, list) in &groups {
            if !f.is_empty() {
                // Searching shows every match, folders open.
                let closed = q.is_empty() && app.ui.style_folds.contains(*f);
                let (r, resp) = ui.allocate_exact_size(vec2(W, ROW_H), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(5), t.menu_hover);
                }
                let chevron = Rect::from_center_size(pos2(r.min.x + 10.0, r.center().y), vec2(10.0, 10.0));
                icons::paint(ui.painter(), chevron, if closed { Icon::ChevronRight } else { Icon::Chevron }, t.menu_dim);
                let label = f.split('/').map(str::trim).collect::<Vec<_>>().join(" / ");
                ui.painter().text(pos2(r.min.x + 22.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.0), t.menu_dim);
                if resp.clicked() && q.is_empty() {
                    *act = Some(Act::ToggleFolder(f.to_string()));
                }
                if closed {
                    continue;
                }
            }
            if grid {
                ui.add_space(4.0);
                swatch_grid(ui, list, applied.as_deref(), editable, act);
            } else {
                for s in list {
                    style_row(ui, s, slot, applied.as_deref() == Some(s.id.as_str()), editable(&s.id), act);
                }
            }
        }
        if !colors.is_empty() {
            ui.add_space(6.0);
            caption(ui, "Color variables");
        }
        for (lib, c, vars) in &colors {
            crate::variable_ui::group_caption(ui, *lib, c);
            let all = lib.map_or(&doc.variables, |l| &l.doc.variables);
            for v in vars {
                if crate::variable_ui::variable_row(ui, all, v, W) {
                    *act = Some(Act::Bind(crate::variable_ui::Pick { library: lib.map(|l| l.manifest.id.clone()), id: v.id.clone() }));
                }
            }
        }
    });
}

/// One property row in the editor (a paint, an effect, a grid); paints open the colour picker.
fn property_row(ui: &mut Ui, label: &str, paint: Option<&Paint>, slot: StyleSlot) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(W, ROW_H), if paint.is_some() { Sense::click() } else { Sense::hover() });
    if paint.is_some() && resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(5), t.menu_hover);
    }
    let pr = Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(14.0, 14.0));
    match paint {
        Some(p) => widgets::paint_swatch(ui.painter(), pr, p),
        None => icons::paint(ui.painter(), pr, slot.icon(), t.menu_text),
    }
    ui.painter().text(pos2(r.min.x + 26.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.0), t.menu_text);
    paint.is_some() && resp.clicked()
}

/// A layout grid's summary ("Columns 12", "Grid 8").
fn grid_label(g: &figcraft_doc::LayoutGrid) -> String {
    match g.kind {
        GridKind::Grid => format!("Grid {}", g.size),
        GridKind::Columns => format!("Columns {}", g.count),
        GridKind::Rows => format!("Rows {}", g.count),
    }
}

/// Editing one style: name, description, its values (paints open the colour picker), update from
/// the selection, delete.
fn editor(ui: &mut Ui, doc: &Document, p: &mut Picker, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let slot = p.slot;
    let has_selection = !p.ids.is_empty();
    let Some(e) = p.edit.as_mut() else { return };
    let Some(style) = doc.style(&e.id).cloned() else {
        *act = Some(Act::Back);
        return;
    };
    let (hr, _) = ui.allocate_exact_size(vec2(W, 24.0), Sense::hover());
    if menu_icon(ui, Rect::from_min_size(hr.min, vec2(24.0, 24.0)), Icon::ChevronLeft, "Back to styles", false).clicked() {
        *act = Some(Act::Back);
    }
    ui.painter().text(pos2(hr.min.x + 30.0, hr.center().y), Align2::LEFT_CENTER, "Edit style", semibold(11.0), t.menu_text);
    ui.add_space(6.0);
    caption(ui, "Name");
    let r = ui.add(egui::TextEdit::singleline(&mut e.name).font(regular(11.0)).desired_width(W));
    if e.focus {
        r.request_focus();
        let mut state = egui::TextEdit::load_state(ui.ctx(), r.id).unwrap_or_default();
        let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(e.name.chars().count()));
        state.cursor.set_char_range(Some(all));
        egui::TextEdit::store_state(ui.ctx(), r.id, state);
        e.focus = false;
    }
    ui.add_space(6.0);
    caption(ui, "Description");
    ui.add(egui::TextEdit::multiline(&mut e.description).font(regular(11.0)).desired_width(W).desired_rows(2));
    ui.add_space(8.0);
    caption(ui, "Properties");
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        match &style.kind {
            StyleKind::Paint { paints } if paints.is_empty() => caption(ui, "No paints"),
            StyleKind::Paint { paints } => {
                // Top of the list paints on top: show in reverse like the Fill section.
                for (i, paint) in paints.iter().enumerate().rev() {
                    let label = format!("{}  {:.0}%", paint.label(), paint.opacity * 100.0);
                    if property_row(ui, &label, Some(paint), slot) {
                        *act = Some(Act::EditPaint(style.id.clone(), i));
                    }
                }
            }
            StyleKind::Effect { effects } => {
                for fx in effects.iter().rev() {
                    property_row(ui, fx.label(), None, slot);
                }
            }
            StyleKind::Grid { grids } => {
                for g in grids {
                    property_row(ui, &grid_label(g), None, slot);
                }
            }
            StyleKind::Text { .. } => {}
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if has_selection && menu_button(ui, "Update from selection").on_hover_text("Take the values from the first selected layer").clicked() {
            *act = Some(Act::UpdateFromSelection(style.id.clone()));
        }
        if menu_button(ui, "Delete").on_hover_text("Delete the style (layers keep their values)").clicked() {
            *act = Some(Act::Delete(style.id.clone()));
        }
    });
}

/// Write the typed name and description of the style being edited, when they changed.
fn commit_edit(app: &mut FigApp, doc: &Document, e: &Editing) {
    let Some(s) = doc.style(&e.id) else { return };
    let name = e.name.trim();
    let mut q = json!({"style": e.id});
    if !name.is_empty() && name != s.name {
        q["name"] = json!(name);
    }
    if e.description != s.description {
        q["description"] = json!(e.description);
    }
    if q.as_object().is_some_and(|o| o.len() > 1) {
        app.run("style.update", q);
    }
}

/// The picker popover (call once per frame).
pub fn show(app: &mut FigApp, ctx: &egui::Context) {
    let Some(mut p) = app.ui.style_picker.take() else { return };
    let Ok(doc) = app.session.ds().map(|d| d.doc.clone()) else { return };
    // A new selection closes the picker (it acts on the layers it was opened for).
    if app.session.selection() != p.ids {
        if let Some(e) = &p.edit {
            commit_edit(app, &doc, e);
        }
        return;
    }
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    // Left of the Design panel; further left while the colour picker (which sits there) is open.
    let mut x = screen.max.x - app.ui.right_width - W - 32.0;
    if app.ui.picker.is_some() {
        x -= 260.0;
    }
    let y = p.y.clamp(screen.min.y + 48.0, (screen.max.y - 460.0).max(screen.min.y + 48.0));
    let mut act = None;
    let area = egui::Area::new(Id::new("style_picker_area")).order(egui::Order::Foreground).fixed_pos(pos2(x, y)).show(ctx, |ui| {
        egui::Frame::NONE.fill(t.menu).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_width(W);
            if p.edit.is_some() {
                editor(ui, &doc, &mut p, &mut act);
            } else {
                listing(app, ui, &doc, &mut p, &mut act);
            }
        });
    });
    let rect = area.response.rect;
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("style_picker_border"))).rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, Color32::from_black_alpha(40)),
        egui::StrokeKind::Inside,
    );
    // Clicks in the colour picker editing this picker's style keep it open.
    let picker_rect =
        app.ui.picker.as_ref().filter(|t| matches!(t, PickerTarget::Style { .. })).and(ctx.memory(|m| m.area_rect(Id::new("color_picker"))));
    let outside = ctx.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|pos| !rect.contains(pos) && !picker_rect.is_some_and(|r| r.contains(pos)))
    });
    let mut close = outside || ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let ids = p.ids.clone();
    let slot = p.slot;
    match act {
        Some(Act::Apply(style)) => {
            let mut q = json!({"ids": ids, "style": style});
            if slot.paints() {
                q["target"] = json!(slot.key());
            }
            app.run("style.apply", q);
            close = true;
        }
        Some(Act::Bind(pick)) => {
            crate::variable_ui::bind(app, &ids, slot.key(), &pick);
            close = true;
        }
        Some(Act::Detach) => {
            app.run("style.detach", json!({"ids": ids, "which": slot.key()}));
        }
        Some(Act::Create) => {
            let name = next_name(&doc, slot.default_name());
            let made = app.run("style.create", json!({"type": slot.create_type(), "name": name, "from": ids.first()}));
            if let Some(id) = made.as_ref().and_then(|v| v["id"].as_str()) {
                p.query.clear();
                p.edit = Some(Editing { id: id.to_string(), name, description: String::new(), focus: true });
            }
        }
        Some(Act::Edit(id)) => p.edit = doc.style(&id).map(editing),
        Some(Act::Back) => {
            if let Some(e) = p.edit.take() {
                commit_edit(app, &doc, &e);
            }
        }
        Some(Act::EditPaint(id, index)) => {
            if let Some(e) = &p.edit {
                commit_edit(app, &doc, e);
            }
            app.ui.picker = Some(PickerTarget::Style { id, index });
        }
        Some(Act::UpdateFromSelection(id)) => {
            app.run("style.update", json!({"style": id, "from": ids.first()}));
        }
        Some(Act::Delete(id)) => {
            app.run("style.delete", json!({"style": id}));
            p.edit = None;
        }
        Some(Act::ToggleFolder(f)) => {
            if !app.ui.style_folds.remove(&f) {
                app.ui.style_folds.insert(f);
            }
        }
        Some(Act::ToggleView) => app.ui.style_grid = !app.ui.style_grid,
        None => {}
    }
    if close {
        if let Some(e) = &p.edit {
            commit_edit(app, &doc, e);
        }
        if matches!(app.ui.picker, Some(PickerTarget::Style { .. })) {
            app.ui.picker = None;
        }
    } else {
        app.ui.style_picker = Some(p);
    }
}

#[cfg(test)]
#[path = "style_picker_tests.rs"]
mod tests;
