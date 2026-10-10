//! Dev Mode extras in the Inspect tab: annotations (notes with pinned properties and a category)
//! drawn as callouts in their category's colour on the canvas and edited in the panel, the
//! categories and the filter of the callouts, the handoff status of the selected frames (with the
//! focus view and Compare changes), the list of frames and sections that are ready for dev, and
//! the focus view's bar.

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::{Annotation, AnnotationCategory, DevStatus, Document, NodeId};
use figcraft_engine::annotations::{self, PINNABLE, Pinnable};
use figcraft_engine::codegen::{CodeLang, code_for};
use figcraft_engine::devinspect::{self, DevItem, Prop};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::right::{menu_check, menu_item_with, menu_popup, prop_row, row};
use crate::theme::{ACCENT, Tokens, doc_c32, regular, semibold};
use crate::widgets::{self, DropLead, PAD};
use crate::{FigApp, RightTab};

/// Width of an annotation callout on the canvas.
const CALLOUT_W: f32 = 220.0;
/// Gap between a layer and its callouts.
const CALLOUT_GAP: f32 = 24.0;

/// The status filters of the ready-for-dev list.
const FILTERS: [(Option<DevStatus>, &str); 3] =
    [(Some(DevStatus::ReadyForDev), "Ready for dev"), (Some(DevStatus::Completed), "Completed"), (None, "Any status")];

/// Which annotations the canvas shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CategoryFilter {
    #[default]
    All,
    Uncategorized,
    /// One category, by id.
    Only(String),
}

impl CategoryFilter {
    /// From `all`, `none` or a category id or name.
    pub fn parse(doc: &Document, s: &str) -> Option<Self> {
        match s.trim() {
            k if k.eq_ignore_ascii_case("all") => Some(Self::All),
            k if k.eq_ignore_ascii_case("none") => Some(Self::Uncategorized),
            k => annotations::find_category(annotations::categories(doc), k).map(|c| Self::Only(c.id.clone())),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Uncategorized => "none",
            Self::Only(id) => id,
        }
    }

    fn shows(&self, doc: &Document, a: &Annotation) -> bool {
        let cat = annotations::category_of(doc, a);
        match self {
            Self::All => true,
            Self::Uncategorized => cat.is_none(),
            Self::Only(id) => cat.is_some_and(|c| c.id == *id),
        }
    }

    fn label(&self, doc: &Document) -> String {
        match self {
            Self::All => "All categories".into(),
            Self::Uncategorized => "No category".into(),
            Self::Only(id) => {
                annotations::categories(doc).iter().find(|c| c.id == *id).map(|c| c.name.clone()).unwrap_or_else(|| "All categories".into())
            }
        }
    }
}

/// Dev Mode UI state: the filters of the ready list and of the annotations, the compare view, and
/// what the canvas, the lists and the code show, recomputed only when the document (its
/// revision), the page or the choice change.
#[derive(Default)]
pub struct DevUi {
    pub filter: usize,
    pub categories: CategoryFilter,
    pub compare: Option<crate::dev_compare::CompareView>,
    annotated: Vec<NodeId>,
    annotated_key: Option<(u64, u64, NodeId)>,
    ready: Vec<DevItem>,
    ready_key: Option<(u64, u64, usize)>,
    /// Every ready-for-dev frame (the focus view's previous / next).
    focus_list: Vec<NodeId>,
    focus_key: Option<(u64, u64)>,
    code: Option<((u64, u64, NodeId, CodeLang), String)>,
}

/// Whether Dev Mode extras show: the Inspect tab is open and nothing is presented.
fn active(app: &FigApp) -> bool {
    app.ui.right_tab == RightTab::Inspect && app.ui.present.is_none()
}

// ------------------------------------------------------------------ canvas

/// The annotations on the current page as callouts beside their layers (Inspect tab only).
pub fn draw(app: &mut FigApp, painter: &egui::Painter) {
    if !active(app) {
        return;
    }
    let Ok(ds) = app.session.ds() else { return };
    let key = (ds.uid, ds.revision, ds.page);
    if app.ui.dev.annotated_key != Some(key) {
        app.ui.dev.annotated = annotations::on_page(&ds.doc, ds.page);
        app.ui.dev.annotated_key = Some(key);
    }
    // A filter on a category the file no longer has shows everything again.
    if let CategoryFilter::Only(id) = &app.ui.dev.categories
        && !annotations::categories(&ds.doc).iter().any(|c| c.id == *id)
    {
        app.ui.dev.categories = CategoryFilter::All;
    }
    let t = Tokens::get(painter.ctx());
    let clip = painter.clip_rect();
    let focus = ds.focused();
    let filter = &app.ui.dev.categories;
    // Callouts already drawn: a later one that would overlap moves down below them.
    let mut placed: Vec<Rect> = Vec::new();
    for id in &app.ui.dev.annotated {
        // The focus view shows only its frame's.
        if focus.is_some_and(|f| !ds.doc.is_ancestor(f, *id)) {
            continue;
        }
        let Some(n) = ds.doc.node(*id).filter(|n| n.visible) else { continue };
        let Some(b) = ds.doc.world_bounds(*id) else { continue };
        let r = Rect::from_two_pos(app.to_screen(b.x0, b.y0), app.to_screen(b.x1, b.y1));
        let x = r.max.x + CALLOUT_GAP;
        if x > clip.max.x || x + CALLOUT_W < clip.min.x || r.min.y > clip.max.y {
            continue;
        }
        let selected = ds.selection.contains(id);
        let anchor = pos2(r.max.x, r.min.y);
        let mut y = r.min.y;
        let mut first = true;
        for a in n.annotations.iter().filter(|a| filter.shows(&ds.doc, a)) {
            if y > clip.max.y {
                break;
            }
            let pinned = annotations::pinned(&ds.doc, *id, a);
            let category = annotations::category_of(&ds.doc, a);
            let color = category.map_or(ACCENT, |c| doc_c32(painter.ctx(), c.color));
            let card = Callout::new(painter, &t, a, category, &pinned);
            let mut rect = Rect::from_min_size(pos2(x, y), vec2(CALLOUT_W, card.height));
            while let Some(below) = placed.iter().filter(|p| p.intersects(rect.expand(3.0))).map(|p| p.max.y).reduce(f32::max) {
                rect = rect.translate(vec2(0.0, below + 6.0 - rect.min.y));
            }
            if first {
                painter.line_segment([anchor, pos2(rect.min.x, rect.min.y + 14.0)], Stroke::new(1.0, color));
                painter.circle_filled(anchor, 3.5, color);
                first = false;
            }
            card.paint(painter, &t, rect, color, &pinned, selected);
            placed.push(rect);
            y = rect.max.y + 6.0;
        }
    }
}

/// One annotation as a card: its category, its note (up to six lines) and its pinned properties
/// with their values.
struct Callout {
    category: Option<std::sync::Arc<egui::Galley>>,
    note: Option<std::sync::Arc<egui::Galley>>,
    height: f32,
}

/// Height of a callout's category line.
const CATEGORY_H: f32 = 18.0;

impl Callout {
    fn new(painter: &egui::Painter, t: &Tokens, a: &Annotation, category: Option<&AnnotationCategory>, pinned: &[(&Pinnable, Option<Prop>)]) -> Self {
        let note = (!a.label.trim().is_empty()).then(|| {
            let mut job = egui::text::LayoutJob::simple(a.label.clone(), regular(11.0), t.text, CALLOUT_W - 22.0);
            job.wrap.max_rows = 6;
            painter.layout_job(job)
        });
        let category = category.map(|c| painter.layout_no_wrap(c.name.clone(), semibold(9.0), Color32::WHITE));
        let note_h = note.as_ref().map_or(0.0, |g| g.size().y);
        let gap = if note.is_some() && !pinned.is_empty() { 6.0 } else { 0.0 };
        let cat_h = if category.is_some() { CATEGORY_H } else { 0.0 };
        Callout { category, note, height: cat_h + note_h + gap + pinned.len() as f32 * 18.0 + 16.0 }
    }

    fn paint(self, painter: &egui::Painter, t: &Tokens, rect: Rect, color: Color32, pinned: &[(&Pinnable, Option<Prop>)], selected: bool) {
        painter.add(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: t.shadow }.as_shape(rect, CornerRadius::same(6)));
        painter.rect_filled(rect, CornerRadius::same(6), t.panel);
        painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, if selected { ACCENT } else { t.border_strong }), StrokeKind::Inside);
        painter.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, rect.height() - 12.0)), CornerRadius::same(2), color);
        let x = rect.min.x + 12.0;
        let mut y = rect.min.y + 8.0;
        // The category as a pill in its colour.
        if let Some(g) = self.category {
            let w = g.size().x.min(CALLOUT_W - 30.0);
            let pill = Rect::from_min_size(pos2(x, y - 1.0), vec2(w + 10.0, 14.0));
            painter.rect_filled(pill, CornerRadius::same(4), color);
            painter.with_clip_rect(pill.shrink2(vec2(5.0, 0.0))).galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
            y += CATEGORY_H;
        }
        if let Some(g) = self.note {
            let top = y;
            y += g.size().y + if pinned.is_empty() { 0.0 } else { 6.0 };
            painter.galley(pos2(x, top), g, t.text);
        }
        for (p, v) in pinned {
            let mid = y + 9.0;
            painter.text(pos2(x, mid), Align2::LEFT_CENTER, p.label, regular(10.0), t.text_secondary);
            let mut vx = x + 84.0;
            if let Some(c) = v.as_ref().and_then(|v| v.color.as_deref()).and_then(figcraft_color::Color::from_hex) {
                widgets::paint_swatch(painter, Rect::from_center_size(pos2(vx + 5.0, mid), vec2(10.0, 10.0)), &figcraft_color::Paint::solid(c));
                vx += 15.0;
            }
            // The style or variable reads better than its value.
            let value = v.as_ref().map(|v| v.token.clone().unwrap_or_else(|| v.value.clone())).unwrap_or_else(|| "—".into());
            let value_r = Rect::from_min_max(pos2(vx, y), pos2(rect.max.x - 8.0, y + 18.0));
            painter.with_clip_rect(value_r).text(pos2(vx, mid), Align2::LEFT_CENTER, value, regular(10.0), t.text);
            y += 18.0;
        }
    }
}

// ------------------------------------------------------------------ Inspect panel

/// Layer `id`'s code in `lang`, generated again only when the document or the choice change (a
/// component set's code compares every variant).
pub fn code(app: &mut FigApp, id: NodeId, lang: CodeLang) -> String {
    let Ok(ds) = app.session.ds() else { return String::new() };
    let key = (ds.uid, ds.revision, id, lang);
    match &app.ui.dev.code {
        Some((k, c)) if *k == key => c.clone(),
        _ => {
            let c = code_for(&ds.doc, id, lang);
            app.ui.dev.code = Some((key, c.clone()));
            c
        }
    }
}

/// The handoff status of the selected frames or sections (nothing for other layers).
pub fn status(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId]) {
    if sel.is_empty() || !sel.iter().all(|i| crate::section_panel::has_status(doc, *i)) {
        return;
    }
    widgets::separator(ui);
    widgets::sublabels(ui, "Status", None);
    let w = ui.available_width() - PAD * 2.0;
    row(ui, 32.0, |ui| crate::section_panel::status_dropdown(app, ui, doc, sel, w));
    if let [id] = sel {
        let changed = doc.node(*id).is_some_and(|n| n.dev_status == DevStatus::ReadyForDev && n.dev_changed);
        let focused = app.session.ds().ok().and_then(figcraft_engine::DocState::focused) == Some(*id);
        row(ui, 32.0, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if !focused && widgets::button(ui, "Focus view", false, true).on_hover_text("Show only this on the canvas").clicked() {
                app.run("dev.focus", json!({"id": id}));
            }
            if changed && widgets::button(ui, "Compare changes", true, true).on_hover_text("What changed since it was marked ready for dev").clicked()
            {
                app.run("dev.compare", json!({"id": id, "show": true}));
            }
        });
    }
    ui.add_space(4.0);
}

/// The selected layer's annotations: each note editable in place with its pinned properties (click
/// to copy), a menu to pin more, and a field to add one.
pub fn annotations_section(app: &mut FigApp, ui: &mut Ui, doc: &Document, id: NodeId) {
    let t = Tokens::get(ui.ctx());
    let Some(n) = doc.node(id) else { return };
    let sections = devinspect::properties(doc, id);
    widgets::separator(ui);
    widgets::section_header(ui, "Annotations", &[], n.annotations.is_empty());
    for (k, a) in n.annotations.iter().enumerate() {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        ui.painter().text(pos2(r.min.x + PAD, r.center().y), Align2::LEFT_CENTER, format!("Note {}", k + 1), semibold(10.0), t.text_secondary);
        let del = Rect::from_center_size(pos2(r.max.x - PAD - 4.0, r.center().y), vec2(22.0, 22.0));
        if widgets::icon_button_at(ui, Id::new(("annot_del", id, k)), del, Icon::Minus, "Remove annotation") {
            app.run("annotation.remove", json!({"id": id, "index": k}));
        }
        if let Some(cat) = category_menu(ui, doc, Id::new(("annot_cat", id, k)), a) {
            app.run("annotation.edit", json!({"id": id, "index": k, "category": cat}));
        }
        if let Some(label) = widgets::multiline_field(ui, Id::new(("annot_note", id, k)), &a.label, "Add a note for developers", 2) {
            if label.trim().is_empty() && a.properties.is_empty() {
                app.run("annotation.remove", json!({"id": id, "index": k}));
            } else {
                app.run("annotation.edit", json!({"id": id, "index": k, "label": label}));
            }
        }
        ui.add_space(2.0);
        for (p, v) in annotations::pinned(doc, id, a) {
            prop_row(app, ui, &v.unwrap_or_else(|| Prop { label: p.label.to_string(), value: "—".into(), color: None, token: None }));
        }
        if let Some(p) = pin_menu(ui, Id::new(("annot_pin", id, k)), &sections, &a.properties) {
            let mut props = a.properties.clone();
            match props.iter().position(|q| q == p.id) {
                Some(i) => {
                    props.remove(i);
                }
                None => props.push(p.id.to_string()),
            }
            if props.is_empty() && a.label.trim().is_empty() {
                app.run("annotation.remove", json!({"id": id, "index": k}));
            } else {
                app.run("annotation.edit", json!({"id": id, "index": k, "properties": props}));
            }
        }
        ui.add_space(6.0);
    }
    // A new annotation starts as a note; properties are pinned to it afterwards.
    if n.annotations.len() < annotations::MAX_PER_LAYER
        && let Some(label) = widgets::multiline_field(ui, Id::new(("annot_new", id)), "", "Add an annotation for developers", 1)
        && !label.trim().is_empty()
    {
        app.run("annotation.add", json!({"id": id, "label": label}));
    }
    ui.add_space(4.0);
}

/// The category of annotation `a` as a dropdown: the one picked (`Value::Null` for none).
fn category_menu(ui: &mut Ui, doc: &Document, id: Id, a: &Annotation) -> Option<serde_json::Value> {
    let current = annotations::category_of(doc, a);
    let mut out = None;
    let w = ui.available_width() - PAD * 2.0;
    row(ui, 30.0, |ui| {
        let lead = current.map(|c| DropLead::Dot(doc_c32(ui.ctx(), c.color)));
        let label = current.map_or("No category", |c| c.name.as_str());
        let resp = ui.push_id(id, |ui| widgets::dropdown_lead(ui, label, w, lead)).inner;
        menu_popup(&resp).show(|ui| {
            if menu_check(ui, current.is_none(), "No category", "") {
                out = Some(serde_json::Value::Null);
                ui.close();
            }
            for c in annotations::categories(doc) {
                if category_item(ui, c, current.is_some_and(|x| x.id == c.id)) {
                    out = Some(json!(c.id));
                    ui.close();
                }
            }
        });
    });
    out
}

/// A menu row for category `c` with its colour dot; true when clicked.
fn category_item(ui: &mut Ui, c: &AnnotationCategory, on: bool) -> bool {
    let col = doc_c32(ui.ctx(), c.color);
    let label = format!("{}{}", if on { "✓ " } else { "    " }, c.name);
    menu_item_with(ui, &label, "", 14.0, |p, r, _| {
        p.circle_filled(pos2(r.max.x - 7.0, r.center().y), 4.5, col);
    })
}

/// With nothing selected: which annotations the canvas shows, and the file's categories (rename,
/// recolour, delete, add).
pub fn categories_section(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Ok(doc) = app.session.doc().cloned() else { return };
    widgets::separator(ui);
    let add = widgets::section_header(ui, "Annotations", &[(Icon::Plus, "Add a category")], false).is_some();
    let w = ui.available_width() - PAD * 2.0;
    let filter = app.ui.dev.categories.clone();
    row(ui, 32.0, |ui| {
        let lead = match &filter {
            CategoryFilter::Only(id) => annotations::categories(&doc).iter().find(|c| c.id == *id).map(|c| DropLead::Dot(doc_c32(ui.ctx(), c.color))),
            _ => Some(DropLead::Icon(Icon::Comment)),
        };
        let resp = widgets::dropdown_lead(ui, &filter.label(&doc), w, lead).on_hover_text("Annotations shown on the canvas");
        menu_popup(&resp).show(|ui| {
            for (f, label) in [(CategoryFilter::All, "All categories"), (CategoryFilter::Uncategorized, "No category")] {
                if menu_check(ui, filter == f, label, "") {
                    app.ui.dev.categories = f;
                    ui.close();
                }
            }
            for c in annotations::categories(&doc) {
                if category_item(ui, c, filter == CategoryFilter::Only(c.id.clone())) {
                    app.ui.dev.categories = CategoryFilter::Only(c.id.clone());
                    ui.close();
                }
            }
        });
    });
    widgets::sublabels(ui, "Categories", None);
    let mut action: Option<(&str, serde_json::Value)> = None;
    for c in annotations::categories(&doc) {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        // The colour: click for the palette.
        let sw = Rect::from_center_size(pos2(r.min.x + PAD + 8.0, r.center().y), vec2(16.0, 16.0));
        let sw_resp = ui.interact(sw, Id::new(("cat_color", &c.id)), Sense::click()).on_hover_text("Colour");
        ui.painter().rect_filled(sw, CornerRadius::same(4), doc_c32(ui.ctx(), c.color));
        if sw_resp.hovered() {
            ui.painter().rect_stroke(sw.expand(2.0), CornerRadius::same(5), Stroke::new(1.0, t.border_strong), StrokeKind::Inside);
        }
        egui::Popup::menu(&sw_resp).style(crate::right::menu_style).show(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for col in AnnotationCategory::COLORS {
                    let (cr, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                    ui.painter().rect_filled(cr, CornerRadius::same(4), doc_c32(ui.ctx(), col));
                    if col.to_hex() == c.color.to_hex() {
                        ui.painter().rect_stroke(cr.expand(2.0), CornerRadius::same(5), Stroke::new(1.5, ACCENT), StrokeKind::Inside);
                    }
                    if resp.clicked() {
                        action = Some(("annotation.editCategory", json!({"id": c.id, "color": col.to_css_hex()})));
                        ui.close();
                    }
                }
            });
        });
        let name_r = Rect::from_min_max(pos2(sw.max.x + 8.0, r.min.y + 3.0), pos2(r.max.x - PAD - 28.0, r.max.y - 3.0));
        if let Some(name) = widgets::text_field_inline(ui, Id::new(("cat_name", &c.id)), &c.name, name_r, regular(11.0))
            && name.trim() != c.name
            && !name.trim().is_empty()
        {
            action = Some(("annotation.editCategory", json!({"id": c.id, "name": name.trim()})));
        }
        let del = Rect::from_center_size(pos2(r.max.x - PAD - 8.0, r.center().y), vec2(22.0, 22.0));
        if widgets::icon_button_at(ui, Id::new(("cat_del", &c.id)), del, Icon::Minus, "Delete category (its annotations stay)") {
            action = Some(("annotation.removeCategory", json!({"id": c.id})));
        }
    }
    if add {
        let list = annotations::categories(&doc);
        let name = (1..=list.len() + 1).map(|k| format!("Category {k}")).find(|n| annotations::find_category(list, n).is_none()).unwrap_or_default();
        action = Some(("annotation.addCategory", json!({"name": name})));
    }
    if let Some((cmd, params)) = action {
        app.run(cmd, params);
    }
    ui.add_space(4.0);
}

/// "Pin a property": a menu of the properties the layer has (those already `pinned` checked);
/// returns the one picked.
fn pin_menu(ui: &mut Ui, id: Id, sections: &[devinspect::Section], pinned: &[String]) -> Option<&'static Pinnable> {
    let mut out = None;
    let w = ui.available_width() - PAD * 2.0;
    row(ui, 30.0, |ui| {
        let resp = ui.push_id(id, |ui| widgets::dropdown(ui, "Pin a property", w, Some(Icon::Plus))).inner;
        menu_popup(&resp).show(|ui| {
            for p in PINNABLE {
                let on = pinned.iter().any(|q| q == p.id);
                if (on || annotations::applies(sections, p)) && menu_check(ui, on, p.label, "") {
                    out = Some(p);
                    ui.close();
                }
            }
        });
    });
    out
}

/// The frames and sections with a handoff status, filtered by status, across every page: click one
/// to go to it.
pub fn ready_list(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Ok(ds) = app.session.ds() else { return };
    let filter = app.ui.dev.filter.min(FILTERS.len() - 1);
    let key = (ds.uid, ds.revision, filter);
    if app.ui.dev.ready_key != Some(key) {
        app.ui.dev.ready = devinspect::dev_items(&ds.doc, FILTERS.get(filter).and_then(|f| f.0), None);
        app.ui.dev.ready_key = Some(key);
    }
    let doc = ds.doc.clone();
    widgets::separator(ui);
    widgets::section_header(ui, "Ready for dev", &[], app.ui.dev.ready.is_empty());
    let w = ui.available_width() - PAD * 2.0;
    row(ui, 32.0, |ui| {
        let resp = widgets::dropdown(ui, FILTERS.get(filter).map_or("", |f| f.1), w, Some(Icon::Code));
        menu_popup(&resp).show(|ui| {
            for (i, (_, label)) in FILTERS.iter().enumerate() {
                if menu_check(ui, i == filter, label, "") {
                    app.ui.dev.filter = i;
                    ui.close();
                }
            }
        });
    });
    if app.ui.dev.ready.is_empty() {
        widgets::note(ui, "Nothing here yet. Set a frame's status to list it.", t.text_secondary);
        return;
    }
    let (mut go, mut focus) = (None, None);
    for item in &app.ui.dev.ready {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        let resp = resp.on_hover_text(format!("Go to {}", item.name));
        if resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(8.0, 0.0)), CornerRadius::same(5), t.hover);
        }
        let dot = if item.status == DevStatus::ReadyForDev { t.success } else { t.text_tertiary };
        ui.painter().circle_filled(pos2(r.min.x + PAD + 4.0, r.center().y), 4.0, dot);
        let page = doc.node(item.page).map(|p| p.name.clone()).unwrap_or_default();
        let page_w = ui.painter().layout_no_wrap(page.clone(), regular(10.0), t.text_secondary).size().x.min(90.0);
        let page_r = Rect::from_min_max(pos2(r.max.x - PAD - page_w, r.min.y), pos2(r.max.x - PAD, r.max.y));
        ui.painter().with_clip_rect(page_r).text(page_r.right_center(), Align2::RIGHT_CENTER, page, regular(10.0), t.text_secondary);
        let name_r = Rect::from_min_max(pos2(r.min.x + PAD + 14.0, r.min.y), pos2(page_r.min.x - 8.0, r.max.y));
        let name = widgets::elide(ui, &item.name, name_r.width());
        ui.painter().text(name_r.left_center(), Align2::LEFT_CENTER, name, regular(11.0), t.text);
        // Hovered: a button to open it in the focus view, over the page name.
        let focus_r = Rect::from_center_size(pos2(r.max.x - PAD - 10.0, r.center().y), vec2(22.0, 22.0));
        let over_focus = ui.rect_contains_pointer(focus_r);
        if resp.hovered() || over_focus {
            ui.painter().rect_filled(
                Rect::from_min_max(pos2(focus_r.min.x - 4.0, r.min.y + 2.0), pos2(r.max.x - 8.0, r.max.y - 2.0)),
                CornerRadius::same(5),
                t.hover,
            );
            if widgets::icon_button_at(ui, Id::new(("ready_focus", item.id)), focus_r, Icon::Frame, "Open in focus view") {
                focus = Some(item.id);
            }
        }
        if resp.clicked() && !over_focus {
            go = Some(item.id);
        }
    }
    if let Some(id) = focus {
        app.run("dev.focus", json!({"id": id}));
    } else if let Some(id) = go {
        app.run("select.set", json!({"ids": [id]}));
        app.run("view.zoomSelection", json!({}));
    }
    ui.add_space(4.0);
}

// ------------------------------------------------------------------ focus view

enum FocusAction {
    Previous,
    Next,
    Compare,
    Exit,
}

/// The focus view's bar at the top of the canvas: the frame's name and status, Compare changes,
/// previous / next ready-for-dev frame and exit.
pub fn focus_bar(app: &mut FigApp, ctx: &egui::Context, canvas: Rect) {
    let Ok(ds) = app.session.ds() else { return };
    // A version preview has the top of the canvas.
    let (Some(id), None) = (ds.focused(), ds.preview.as_ref()) else { return };
    let doc = ds.doc.clone();
    let key = (ds.uid, ds.revision);
    if app.ui.dev.focus_key != Some(key) {
        app.ui.dev.focus_list = devinspect::dev_items(&doc, Some(DevStatus::ReadyForDev), None).into_iter().map(|i| i.id).collect();
        app.ui.dev.focus_key = Some(key);
    }
    let Some(n) = doc.node(id) else { return };
    let list = &app.ui.dev.focus_list;
    let at = list.iter().position(|i| *i == id);
    // Somewhere to go: another ready frame, or into the list from outside it.
    let can_step = list.len() > usize::from(at.is_some());
    let t = Tokens::get(ctx);
    let action = widgets::canvas_banner(ctx, Id::new("dev_focus_bar"), canvas, |ui| {
        let mut out = None;
        let icon = crate::layers::node_icon(&doc, id);
        icons::paint(ui.painter(), Rect::from_center_size(ui.cursor().min + vec2(7.0, 12.0), vec2(14.0, 14.0)), icon, t.icon_secondary);
        ui.add_space(20.0);
        let name = widgets::elide(ui, &n.name, 260.0);
        ui.label(egui::RichText::new(name).font(semibold(11.0)).color(t.text));
        if let Some(b) = crate::canvas::status_badge(ui.painter(), n, ui.cursor().min + vec2(0.0, 12.0)) {
            ui.add_space(b.width());
        }
        if n.dev_status == DevStatus::ReadyForDev && n.dev_changed && widgets::button(ui, "Compare changes", true, true).clicked() {
            out = Some(FocusAction::Compare);
        }
        if can_step {
            ui.add_space(4.0);
            if widgets::icon_button(ui, Icon::ChevronLeft, "Previous ready for dev") {
                out = Some(FocusAction::Previous);
            }
            let pos = at.map_or_else(|| format!("{} ready", list.len()), |k| format!("{} of {}", k + 1, list.len()));
            ui.label(egui::RichText::new(pos).font(regular(11.0)).color(t.text_secondary));
            if widgets::icon_button(ui, Icon::ChevronRight, "Next ready for dev") {
                out = Some(FocusAction::Next);
            }
        }
        if widgets::icon_button(ui, Icon::Close, "Exit focus view (Esc)") {
            out = Some(FocusAction::Exit);
        }
        out
    });
    match action {
        Some(FocusAction::Previous) => {
            app.run("dev.focus", json!({"direction": "previous"}));
        }
        Some(FocusAction::Next) => {
            app.run("dev.focus", json!({"direction": "next"}));
        }
        Some(FocusAction::Compare) => {
            app.run("dev.compare", json!({"id": id, "show": true}));
        }
        Some(FocusAction::Exit) => {
            app.run("dev.unfocus", json!({}));
        }
        None => {}
    }
}
