//! The Assets tab: local components as thumbnail tiles (search, click to place, drag onto the
//! canvas to drop an instance into the frame or auto layout slot under the pointer) and local
//! styles.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{COMPONENT, Tokens, regular, semibold};

const TILE_LABEL: f32 = 22.0;
/// Re-check thumbnails at most this often (seconds) while the document changes.
const THUMB_REFRESH: f64 = 0.4;
const MAX_THUMBS: usize = 200;

/// Rendered thumbnails keyed by component, with the content fingerprint they were made from.
#[derive(Clone, Default)]
struct Thumbs {
    map: HashMap<NodeId, (u64, Option<egui::TextureHandle>)>,
    checked_at: f64,
    revision: u64,
}

/// A component being dragged out of the panel.
#[derive(Clone, Copy)]
struct AssetDrag {
    component: NodeId,
}

fn drag_id() -> Id {
    Id::new("asset_drag")
}

/// Content fingerprint of a layer subtree (re-render its thumbnail when it changes).
fn fingerprint(doc: &Document, id: NodeId) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut stack = vec![id];
    let mut n = 0;
    while let Some(i) = stack.pop() {
        n += 1;
        if n > 5000 {
            break;
        }
        let Some(node) = doc.node(i) else { continue };
        if let Ok(bytes) = serde_json::to_vec(node) {
            bytes.hash(&mut h);
        }
        stack.extend(node.children.iter().copied());
    }
    h.finish()
}

fn thumbnail(ctx: &egui::Context, doc: &Document, id: NodeId, px: f32) -> Option<egui::TextureHandle> {
    let b = figcraft_render::export_bounds(doc, id)?;
    let side = b.width().max(b.height());
    if !(side.is_finite() && side > 0.0) {
        return None;
    }
    let scale = (f64::from(px) / side).min(2.0);
    let pix = figcraft_render::render_node(doc, id, scale)?;
    let size = [pix.width() as usize, pix.height() as usize];
    let img = egui::ColorImage::from_rgba_unmultiplied(size, &figcraft_render::to_rgba8(&pix));
    Some(ctx.load_texture(format!("asset-thumb-{}", id.0), img, egui::TextureOptions::LINEAR))
}

/// Local components (sets as one entry) matching `filter`.
fn components(doc: &Document, filter: &str) -> Vec<(NodeId, String, bool)> {
    let in_set = |n: &figcraft_doc::Node| n.parent.and_then(|p| doc.node(p)).is_some_and(|p| matches!(p.kind, NodeKind::ComponentSet { .. }));
    let mut out: Vec<(NodeId, String, bool)> = doc
        .nodes()
        .filter(|n| matches!(n.kind, NodeKind::Component { .. }) && !in_set(n) || matches!(n.kind, NodeKind::ComponentSet { .. }))
        .filter(|n| filter.is_empty() || n.name.to_lowercase().contains(filter))
        .map(|n| (n.id, n.name.clone(), matches!(n.kind, NodeKind::ComponentSet { .. })))
        .collect();
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.0.cmp(&b.0)));
    out
}

pub fn show(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let Ok(ds) = app.session.ds() else { return };
    let doc = ds.doc.clone();
    let revision = ds.revision;
    // Search.
    let filter_id = Id::new("asset_filter");
    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    let (fr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let field = Rect::from_min_max(pos2(fr.min.x + 12.0, fr.min.y + 8.0), pos2(fr.max.x - 12.0, fr.max.y - 4.0));
    ui.painter().rect_filled(field, CornerRadius::same(5), t.field);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(field.min.x + 12.0, field.center().y), vec2(12.0, 12.0)), Icon::Search, t.icon_secondary);
    let te_rect = Rect::from_min_max(pos2(field.min.x + 24.0, field.center().y - 7.0), pos2(field.max.x - 6.0, field.center().y + 7.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(te_rect));
    let r = child.add(
        egui::TextEdit::singleline(&mut filter)
            .id(Id::new("asset_filter_field"))
            .frame(egui::Frame::NONE)
            .font(regular(11.0))
            .hint_text("Search assets")
            .desired_width(te_rect.width()),
    );
    if r.has_focus() {
        ui.painter().rect_stroke(field, CornerRadius::same(5), Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            filter.clear();
        }
    }
    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    let q = filter.trim().to_lowercase();
    let comps = components(&doc, &q);
    let styles: Vec<(String, String)> = doc
        .styles
        .iter()
        .filter(|s| q.is_empty() || s.name.to_lowercase().contains(&q))
        .map(|s| (s.name.clone(), s.kind.type_name().to_string()))
        .collect();
    // Thumbnails: refresh changed ones (throttled while the document changes).
    let now = ui.input(|i| i.time);
    let mut thumbs: Thumbs = ui.data(|d| d.get_temp(Id::new("asset_thumbs"))).unwrap_or_default();
    if thumbs.revision != revision && now - thumbs.checked_at >= THUMB_REFRESH || thumbs.map.is_empty() && !comps.is_empty() {
        let tile_px = ((w - 40.0) / 2.0).max(32.0) * ui.ctx().pixels_per_point();
        let live: Vec<NodeId> = comps.iter().take(MAX_THUMBS).map(|c| c.0).collect();
        thumbs.map.retain(|id, _| live.contains(id));
        for id in live {
            let f = fingerprint(&doc, id);
            if thumbs.map.get(&id).is_none_or(|(old, _)| *old != f) {
                thumbs.map.insert(id, (f, thumbnail(ui.ctx(), &doc, id, tile_px)));
            }
        }
        thumbs.checked_at = now;
        thumbs.revision = revision;
    } else if thumbs.revision != revision {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(THUMB_REFRESH));
    }
    let mut place = None;
    let mut drag_start = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let (hr, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Local components", semibold(11.0), t.text);
        if comps.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                let hint = if q.is_empty() { "No components yet. Select layers and press ⌥⌘K." } else { "No matching components" };
                crate::widgets::label(ui, hint, t.text_secondary);
            });
            ui.add_space(8.0);
        }
        // Two-column tiles.
        let gap = 8.0;
        let tile = ((w - 32.0 - gap) / 2.0).max(40.0);
        for pair in comps.chunks(2) {
            let (row, _) = ui.allocate_exact_size(vec2(w, tile + TILE_LABEL + gap), Sense::hover());
            for (k, (id, name, set)) in pair.iter().enumerate() {
                let x = row.min.x + 16.0 + k as f32 * (tile + gap);
                let r = Rect::from_min_size(pos2(x, row.min.y), vec2(tile, tile));
                let resp = ui.interact(
                    r.union(Rect::from_min_size(r.left_bottom(), vec2(tile, TILE_LABEL))),
                    Id::new(("asset", id.0)),
                    Sense::click_and_drag(),
                );
                ui.painter().rect_filled(r, CornerRadius::same(6), t.field);
                if resp.hovered() {
                    ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, COMPONENT), egui::StrokeKind::Inside);
                }
                match thumbs.map.get(id).and_then(|(_, tex)| tex.as_ref()) {
                    Some(tex) => {
                        let size = tex.size_vec2();
                        // Fit the tile, never past 1:1 physical pixels (small components stay crisp).
                        let inner = r.shrink(14.0);
                        let fit = (inner.width() / size.x).min(inner.height() / size.y).min(1.0 / ui.ctx().pixels_per_point());
                        let img = Rect::from_center_size(r.center(), size * fit);
                        ui.painter().image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => icons::paint(
                        ui.painter(),
                        Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
                        if *set { Icon::ComponentSet } else { Icon::Component },
                        COMPONENT,
                    ),
                }
                let label = Rect::from_min_size(pos2(x, r.max.y + 2.0), vec2(tile, TILE_LABEL - 4.0));
                let galley = ui.painter().layout(name.clone(), regular(11.0), t.text, tile);
                ui.painter().with_clip_rect(label).galley(pos2(x, label.center().y - galley.size().y / 2.0), galley, t.text);
                let resp = resp.on_hover_text(format!("{name}\nDrag onto the canvas, or click to place in the middle of the view"));
                if resp.drag_started() {
                    drag_start = Some(*id);
                }
                if resp.clicked() {
                    place = Some(*id);
                }
            }
        }
        crate::widgets::separator(ui);
        let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Local styles", semibold(11.0), t.text);
        for (name, ty) in &styles {
            let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            let icon = match ty.as_str() {
                "text" => Icon::Text,
                "effect" => Icon::Effects,
                "grid" => Icon::Grid,
                _ => Icon::Fill,
            };
            icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 22.0, r.center().y), vec2(14.0, 14.0)), icon, t.icon_secondary);
            ui.painter().text(pos2(r.min.x + 40.0, r.center().y), Align2::LEFT_CENTER, name, regular(11.0), t.text);
        }
    });
    if let Some(component) = drag_start {
        ui.data_mut(|d| d.insert_temp(drag_id(), AssetDrag { component }));
    }
    let tex = |id: NodeId| thumbs.map.get(&id).and_then(|(_, t)| t.clone());
    drag_feedback(app, ui, &doc, tex);
    ui.data_mut(|d| d.insert_temp(Id::new("asset_thumbs"), thumbs));
    if let Some(id) = place {
        let (cx, cy) = app.session.ds().map(|d| (d.view.cx, d.view.cy)).unwrap_or((0.0, 0.0));
        app.run("instance.place", json!({"component": id, "x": cx, "y": cy}));
    }
}

/// While a component is dragged: a ghost at the pointer and, over the canvas, the frame or auto
/// layout slot it would drop into. Releasing over the canvas places the instance.
fn drag_feedback(app: &mut FigApp, ui: &mut Ui, doc: &Document, tex: impl Fn(NodeId) -> Option<egui::TextureHandle>) {
    let Some(drag) = ui.data(|d| d.get_temp::<AssetDrag>(drag_id())) else { return };
    let (pointer, released) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.any_released()));
    let over_canvas = pointer.filter(|p| app.canvas.rect.contains(*p));
    let page = app.session.ds().map(|d| d.page).ok();
    let slot = over_canvas.zip(page).map(|(p, page)| {
        let pt = app.to_page(p);
        (pt, figcraft_engine::drop::slot(doc, page, pt))
    });
    let ov = &mut app.session.tool.overlay;
    ov.drop_target = slot.as_ref().and_then(|(_, s)| (Some(s.parent) != page).then_some(s.parent));
    ov.insert_line = slot.as_ref().and_then(|(_, s)| s.line);
    if let Some(p) = pointer {
        ghost(ui, p, doc, drag.component, tex(drag.component));
    }
    if released {
        ui.data_mut(|d| d.remove::<AssetDrag>(drag_id()));
        let ov = &mut app.session.tool.overlay;
        ov.drop_target = None;
        ov.insert_line = None;
        if let Some((pt, _)) = slot {
            app.run("instance.place", json!({"component": drag.component, "x": pt.x, "y": pt.y}));
        }
    }
    ui.ctx().request_repaint();
}

fn ghost(ui: &Ui, p: Pos2, doc: &Document, id: NodeId, tex: Option<egui::TextureHandle>) {
    let t = Tokens::get(ui.ctx());
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("asset_ghost")));
    let r = Rect::from_center_size(p + vec2(0.0, 0.0), vec2(64.0, 64.0));
    painter.rect_filled(r, CornerRadius::same(6), t.panel.gamma_multiply(0.9));
    painter.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, COMPONENT), egui::StrokeKind::Inside);
    match tex {
        Some(tex) => {
            let size = tex.size_vec2();
            let fit = (56.0 / size.x).min(56.0 / size.y);
            painter.image(
                tex.id(),
                Rect::from_center_size(r.center(), size * fit),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE.gamma_multiply(0.85),
            );
        }
        None => {
            let set = doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::ComponentSet { .. }));
            icons::paint(
                &painter,
                Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
                if set { Icon::ComponentSet } else { Icon::Component },
                COMPONENT,
            );
        }
    }
}
