//! Exporting: the Design panel's Export section (a row per export setting with size, suffix and
//! format, a preview, and "Export N layers"), the page's Export button, and the export actions
//! the Export dialog shares. Every change runs an `export.*` or `file.export*` command.

use std::collections::BTreeSet;

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::*;
use figcraft_engine::cmd::file::export_jobs;
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::right::{common, menu_item, menu_popup, row};
use crate::theme::{Tokens, regular};
use crate::widgets::{self, FIELD_H};

/// Re-render the preview at most this often (seconds) while the document changes.
const PREVIEW_REFRESH: f64 = 0.3;
/// Height of the preview box.
const PREVIEW_H: f32 = 160.0;

/// The Export section for the selected layers.
pub fn section(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId]) {
    let t = Tokens::get(ui.ctx());
    let ids = sel.to_vec();
    let shared = common(doc, sel, |n| n.exports.clone());
    let mixed = shared.is_none() && sel.len() > 1;
    let list = shared.unwrap_or_default();
    let tip = if mixed { "Replace mixed export settings" } else { "Add export setting" };
    if widgets::section_header(ui, "Export", &[(Icon::Plus, tip)], list.is_empty()).is_some() {
        if mixed {
            app.set_props(&ids, json!({"exports": [ExportSetting::default()]}));
        } else {
            app.run("export.add", json!({"ids": ids}));
        }
    }
    if mixed {
        row(ui, 28.0, |ui| widgets::label(ui, "Click + to replace mixed content", t.text_secondary));
    }
    for (i, e) in list.iter().enumerate() {
        setting_row(app, ui, &ids, i, e);
    }
    let jobs = export_jobs(doc, sel);
    if jobs.is_empty() {
        return;
    }
    if let Some(first) = jobs.first().map(|j| j.id) {
        preview(app, ui, doc, first);
    }
    let layers: BTreeSet<NodeId> = jobs.iter().map(|j| j.id).collect();
    let label = match (layers.len(), layers.first().and_then(|i| doc.node(*i))) {
        (1, Some(n)) => format!("Export {}", n.name),
        (k, _) => format!("Export {k} layers"),
    };
    if wide_button(ui, &label) {
        export_layers(app, sel);
    }
}

/// One export setting: size (typed or picked), suffix, format and remove.
fn setting_row(app: &mut FigApp, ui: &mut Ui, ids: &[NodeId], i: usize, e: &ExportSetting) {
    row(ui, 32.0, |ui| {
        let (size_w, format_w, gap) = (56.0, 60.0, ui.spacing().item_spacing.x);
        let suffix_w = (ui.available_width() - size_w - format_w - 24.0 - gap * 3.0).max(32.0);
        let presets: Vec<String> =
            ExportSize::SCALES.iter().map(|value| ExportSize::Scale { value: *value }.label()).chain(["512w".into(), "512h".into()]).collect();
        if let Some(s) = widgets::combo_field(ui, Id::new(("export_size", i)), &e.size.label(), size_w, &presets)
            && ExportSize::parse(&s) != Some(e.size)
        {
            app.run("export.update", json!({"ids": ids, "index": i, "size": s}));
        }
        if let Some(sfx) = widgets::text_field(ui, Id::new(("export_suffix", i)), &e.suffix, suffix_w, "Suffix")
            && sfx != e.suffix
        {
            app.run("export.update", json!({"ids": ids, "index": i, "suffix": sfx}));
        }
        let resp = widgets::dropdown(ui, e.format.label(), format_w, None);
        menu_popup(&resp).show(|ui| {
            for f in ExportFormat::ALL {
                if menu_item(ui, f.label(), "") {
                    app.run("export.update", json!({"ids": ids, "index": i, "format": f.ext()}));
                    ui.close();
                }
            }
        });
        if icons::button(ui, Icon::Minus, 24.0, false, "Remove export setting").clicked() {
            app.run("export.remove", json!({"ids": ids, "index": i}));
        }
    });
}

/// The rendered preview, cached per layer and document revision.
#[derive(Clone)]
struct PreviewTex {
    id: NodeId,
    revision: u64,
    at: f64,
    tex: Option<egui::TextureHandle>,
}

/// A "Preview" disclosure row; open, it shows the layer as it exports.
fn preview(app: &mut FigApp, ui: &mut Ui, doc: &Document, id: NodeId) {
    let t = Tokens::get(ui.ctx());
    let mut open = app.ui.export_thumb;
    row(ui, 28.0, |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), FIELD_H), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
        }
        let icon = if open { Icon::Chevron } else { Icon::ChevronRight };
        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 8.0, r.center().y), vec2(12.0, 12.0)), icon, t.icon_secondary);
        ui.painter().text(pos2(r.min.x + 20.0, r.center().y), Align2::LEFT_CENTER, "Preview", regular(11.0), t.text);
        if resp.clicked() {
            open = !open;
        }
    });
    app.ui.export_thumb = open;
    if !open {
        return;
    }
    let revision = app.session.ds().map(|d| d.revision).unwrap_or(0);
    let now = ui.input(|i| i.time);
    let cache_id = Id::new("export_preview_tex");
    let cached: Option<PreviewTex> = ui.data(|d| d.get_temp(cache_id));
    let stale = cached.as_ref().is_none_or(|c| c.id != id || c.revision != revision);
    let due = cached.as_ref().is_none_or(|c| c.id != id || now - c.at >= PREVIEW_REFRESH);
    let tex = match cached {
        Some(c) if !stale || !due => {
            if stale {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(PREVIEW_REFRESH));
            }
            c.tex
        }
        _ => {
            let px = ui.available_width() * ui.ctx().pixels_per_point();
            let tex = crate::assets_panel::thumbnail(ui.ctx(), doc, id, px);
            ui.data_mut(|d| d.insert_temp(cache_id, PreviewTex { id, revision, at: now, tex: tex.clone() }));
            tex
        }
    };
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, PREVIEW_H + 8.0), Sense::hover());
    let frame = Rect::from_min_max(pos2(r.min.x + widgets::PAD, r.min.y), pos2(r.max.x - widgets::PAD, r.max.y - 8.0));
    widgets::checker(&ui.painter().with_clip_rect(frame), frame, 8.0);
    ui.painter().rect_stroke(frame, CornerRadius::same(4), Stroke::new(1.0, t.border), StrokeKind::Inside);
    if let Some(tex) = tex {
        let [tw, th] = tex.size().map(|v| v.max(1) as f32);
        // Fit the box, never past the texture's own pixels.
        let k = ((frame.width() - 16.0) / tw).min((frame.height() - 16.0) / th).min(1.0 / ui.ctx().pixels_per_point().max(0.5)).max(0.0);
        let img = Rect::from_center_size(frame.center(), vec2(tw * k, th * k));
        ui.painter().image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
}

/// A full-width outlined button row; true when clicked.
pub(crate) fn wide_button(ui: &mut Ui, label: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut clicked = false;
    row(ui, 40.0, |ui| {
        let w = ui.available_width();
        let (r, resp) = ui.allocate_exact_size(vec2(w, FIELD_H), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
        }
        ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, t.border_strong), StrokeKind::Inside);
        let galley = ui.painter().layout(label.to_string(), regular(11.0), t.text, w - 16.0);
        ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text);
        clicked = resp.clicked();
    });
    clicked
}

/// Export `ids` with their export settings: one file through a save dialog, several into a
/// folder.
pub(crate) fn export_layers(app: &mut FigApp, ids: &[NodeId]) {
    let jobs = match app.session.doc() {
        Ok(doc) => export_jobs(doc, ids),
        Err(_) => return,
    };
    match jobs.as_slice() {
        [] => {}
        [one] => save_one(app, &[one.id], &one.setting, &one.file),
        _ => {
            let Some(dir) = app.services.pick_folder("Export") else { return };
            if let Some(r) = app.run("file.exportLayers", json!({"ids": ids, "dir": dir.to_string_lossy()})) {
                let n = r["files"].as_array().map_or(0, Vec::len);
                app.toast(format!("Exported {n} file{}", if n == 1 { "" } else { "s" }));
            }
        }
    }
}

/// Export `ids` together as one file with `setting`, through a save dialog suggesting `file`.
pub(crate) fn save_one(app: &mut FigApp, ids: &[NodeId], setting: &ExportSetting, file: &str) {
    let ext = setting.format.ext();
    let Some(p) = app.services.pick_save("Export", file, &[ext]) else { return };
    let path = p.to_string_lossy().to_string();
    if app.run("file.export", json!({"ids": ids, "format": ext, "size": setting.size, "path": path})).is_some() {
        app.toast(format!("Exported {}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()));
    }
}

/// The file name a one-off export of `ids` suggests: the first layer's name (or `fallback`) with
/// the setting's suffix.
pub(crate) fn suggested_name(doc: &Document, ids: &[NodeId], setting: &ExportSetting, fallback: &str) -> String {
    let name = ids.first().and_then(|i| doc.node(*i)).map_or(fallback, |n| n.name.as_str());
    format!("{name}{}.{}", setting.file_suffix(), setting.format.ext())
}

/// The page's Export section: the whole page as one PNG.
pub fn page_section(app: &mut FigApp, ui: &mut Ui, doc: &Document, page: NodeId) {
    widgets::section_header(ui, "Export", &[], false);
    if wide_button(ui, "Export page as PNG") {
        let all: Vec<NodeId> = doc.node(page).map(|p| p.children.clone()).unwrap_or_default();
        let setting = ExportSetting::default();
        let page_name = doc.node(page).map_or("Page", |p| p.name.as_str());
        let file = format!("{page_name}.{}", setting.format.ext());
        save_one(app, &all, &setting, &file);
    }
}
