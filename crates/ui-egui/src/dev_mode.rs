//! Dev Mode extras in the Inspect tab: annotations (notes with pinned properties) drawn as
//! callouts on the canvas and edited in the panel, the handoff status of the selected frames, and
//! the list of frames and sections that are ready for dev.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::{Annotation, DevStatus, Document, NodeId};
use figcraft_engine::annotations::{self, PINNABLE, Pinnable};
use figcraft_engine::codegen::{CodeLang, code_for};
use figcraft_engine::devinspect::{self, DevItem, Prop};
use serde_json::json;

use crate::a11y::Tip;
use crate::icons::{self, Icon};
use crate::right::{menu_check, menu_popup, prop_row, row};
use crate::theme::{ACCENT, Tokens, regular, semibold};
use crate::widgets::{self, PAD};
use crate::{FigApp, RightTab};

/// Width of an annotation callout on the canvas.
const CALLOUT_W: f32 = 220.0;
/// Gap between a layer and its callouts.
const CALLOUT_GAP: f32 = 24.0;

/// The status filters of the ready-for-dev list.
const FILTERS: [(Option<DevStatus>, &str); 3] =
    [(Some(DevStatus::ReadyForDev), "Ready for dev"), (Some(DevStatus::Completed), "Completed"), (None, "Any status")];

/// Dev Mode UI state: the filter of the ready list, and what the canvas, the list and the code
/// show, recomputed only when the document (its revision), the page or the choice change.
#[derive(Default)]
pub struct DevUi {
    pub filter: usize,
    annotated: Vec<NodeId>,
    annotated_key: Option<(u64, u64, NodeId)>,
    ready: Vec<DevItem>,
    ready_key: Option<(u64, u64, usize)>,
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
    let t = Tokens::get(painter.ctx());
    let clip = painter.clip_rect();
    // Callouts already drawn: a later one that would overlap moves down below them.
    let mut placed: Vec<Rect> = Vec::new();
    for id in &app.ui.dev.annotated {
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
        for (k, a) in n.annotations.iter().enumerate() {
            if y > clip.max.y {
                break;
            }
            let pinned = annotations::pinned(&ds.doc, *id, a);
            let card = Callout::new(painter, &t, a, &pinned);
            let mut rect = Rect::from_min_size(pos2(x, y), vec2(CALLOUT_W, card.height));
            while let Some(below) = placed.iter().filter(|p| p.intersects(rect.expand(3.0))).map(|p| p.max.y).reduce(f32::max) {
                rect = rect.translate(vec2(0.0, below + 6.0 - rect.min.y));
            }
            if k == 0 {
                painter.line_segment([anchor, pos2(rect.min.x, rect.min.y + 14.0)], Stroke::new(1.0, ACCENT));
                painter.circle_filled(anchor, 3.5, ACCENT);
            }
            card.paint(painter, &t, rect, &pinned, selected);
            placed.push(rect);
            y = rect.max.y + 6.0;
        }
    }
}

/// One annotation as a card: its note (up to six lines) and its pinned properties with their
/// values.
struct Callout {
    note: Option<std::sync::Arc<egui::Galley>>,
    height: f32,
}

impl Callout {
    fn new(painter: &egui::Painter, t: &Tokens, a: &Annotation, pinned: &[(&Pinnable, Option<Prop>)]) -> Self {
        let note = (!a.label.trim().is_empty()).then(|| {
            let mut job = egui::text::LayoutJob::simple(a.label.clone(), regular(11.0), t.text, CALLOUT_W - 22.0);
            job.wrap.max_rows = 6;
            painter.layout_job(job)
        });
        let note_h = note.as_ref().map_or(0.0, |g| g.size().y);
        let gap = if note.is_some() && !pinned.is_empty() { 6.0 } else { 0.0 };
        Callout { note, height: note_h + gap + pinned.len() as f32 * 18.0 + 16.0 }
    }

    fn paint(self, painter: &egui::Painter, t: &Tokens, rect: Rect, pinned: &[(&Pinnable, Option<Prop>)], selected: bool) {
        painter.add(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: t.shadow }.as_shape(rect, CornerRadius::same(6)));
        painter.rect_filled(rect, CornerRadius::same(6), t.panel);
        painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, if selected { ACCENT } else { t.border_strong }), StrokeKind::Inside);
        painter.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, rect.height() - 12.0)), CornerRadius::same(2), ACCENT);
        let x = rect.min.x + 12.0;
        let mut y = rect.min.y + 8.0;
        if let Some(g) = self.note {
            y += g.size().y + if pinned.is_empty() { 0.0 } else { 6.0 };
            painter.galley(pos2(x, rect.min.y + 8.0), g, t.text);
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
        let resp = ui.interact(del, Id::new(("annot_del", id, k)), Sense::click()).tip("Remove annotation");
        if resp.hovered() {
            ui.painter().rect_filled(del, CornerRadius::same(5), t.hover);
        }
        icons::paint(ui.painter(), del.shrink(5.0), Icon::Minus, t.icon);
        if resp.clicked() {
            app.run("annotation.remove", json!({"id": id, "index": k}));
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
    let mut go = None;
    for item in &app.ui.dev.ready {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        let resp = resp.tip(format!("Go to {}", item.name));
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
        if resp.clicked() {
            go = Some(item.id);
        }
    }
    if let Some(id) = go {
        app.run("select.set", json!({"ids": [id]}));
        app.run("view.zoomSelection", json!({}));
    }
    ui.add_space(4.0);
}
