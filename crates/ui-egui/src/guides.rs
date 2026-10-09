//! Ruler guides on the canvas: drag out of a ruler to add one, drag a guide to move it, drag it
//! back onto its ruler to remove it. Guides show while the rulers do; layers snap to them.

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Stroke, pos2, vec2};
use figcraft_doc::{Guide, NodeKind};
use serde_json::json;

use crate::FigApp;
use crate::theme::semibold;

/// Ruler thickness (matches the canvas rulers).
const RULER: f32 = 20.0;
const GUIDE: Color32 = Color32::from_rgb(0xF2, 0x48, 0x22);

#[derive(Clone, Copy, Debug)]
enum GuideDrag {
    /// Dragging a new guide out of a ruler.
    New { vertical: bool },
    /// Moving guide `index`.
    Move { index: usize, vertical: bool },
}

fn drag_id() -> Id {
    Id::new("guide_drag")
}

/// The dragged guide's live position (page units).
fn preview_id() -> Id {
    Id::new("guide_preview")
}

fn page_guides(app: &FigApp) -> Vec<Guide> {
    let Ok(ds) = app.session.ds() else { return vec![] };
    match ds.doc.node(ds.page).map(|n| &n.kind) {
        Some(NodeKind::Page { guides, .. }) => guides.clone(),
        _ => vec![],
    }
}

fn rulers_on(app: &FigApp) -> bool {
    app.session.ds().is_ok_and(|d| d.view.rulers)
}

/// Screen coordinate of a guide (x for vertical guides, y for horizontal).
fn screen_of(app: &FigApp, g: &Guide) -> f32 {
    if g.vertical { app.to_screen(g.offset, 0.0).x } else { app.to_screen(0.0, g.offset).y }
}

/// The guide within a few points of `pos` (top-most first).
fn guide_at(app: &FigApp, pos: Pos2) -> Option<(usize, bool)> {
    page_guides(app)
        .iter()
        .enumerate()
        .rev()
        .find(|(_, g)| (screen_of(app, g) - if g.vertical { pos.x } else { pos.y }).abs() <= 3.0)
        .map(|(i, g)| (i, g.vertical))
}

fn cursor(vertical: bool) -> egui::CursorIcon {
    if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical }
}

/// Pointer handling; returns true when the event belongs to a ruler or guide.
pub fn input(app: &mut FigApp, ctx: &egui::Context, pos: Pos2) -> bool {
    if !rulers_on(app) {
        return false;
    }
    let r = app.canvas.rect;
    let (pressed, released) =
        ctx.input(|i| (i.pointer.button_pressed(egui::PointerButton::Primary), i.pointer.button_released(egui::PointerButton::Primary)));
    if let Some(drag) = ctx.data(|d| d.get_temp::<GuideDrag>(drag_id())) {
        let vertical = match drag {
            GuideDrag::New { vertical } | GuideDrag::Move { vertical, .. } => vertical,
        };
        let p = app.to_page(pos);
        let snap = app.session.ds().map(|d| d.view.snap_pixel).unwrap_or(true);
        let raw = if vertical { p.x } else { p.y };
        let offset = if snap { raw.round() } else { (raw * 100.0).round() / 100.0 };
        ctx.data_mut(|d| d.insert_temp(preview_id(), (vertical, offset)));
        ctx.set_cursor_icon(cursor(vertical));
        if released {
            ctx.data_mut(|d| {
                d.remove::<GuideDrag>(drag_id());
                d.remove::<(bool, f64)>(preview_id());
            });
            // Released over its own ruler: no guide (a moved one is removed).
            let on_ruler = if vertical { pos.x < r.min.x + RULER } else { pos.y < r.min.y + RULER };
            match (drag, on_ruler) {
                (GuideDrag::New { .. }, false) => {
                    app.run("guide.add", json!({"vertical": vertical, "offset": offset}));
                }
                (GuideDrag::Move { index, .. }, true) => {
                    app.run("guide.remove", json!({"index": index}));
                }
                (GuideDrag::Move { index, .. }, false) => {
                    app.run("guide.move", json!({"index": index, "offset": offset}));
                }
                (GuideDrag::New { .. }, true) => {}
            }
        }
        ctx.request_repaint();
        return true;
    }
    let in_top = pos.y < r.min.y + RULER && pos.x > r.min.x + RULER;
    let in_left = pos.x < r.min.x + RULER && pos.y > r.min.y + RULER;
    if in_top || in_left {
        ctx.set_cursor_icon(cursor(in_left));
        if pressed {
            ctx.data_mut(|d| d.insert_temp(drag_id(), GuideDrag::New { vertical: in_left }));
        }
        return true;
    }
    if pos.x < r.min.x + RULER || pos.y < r.min.y + RULER {
        return true;
    }
    // Existing guides move with the Move tool (not while editing text or a vector).
    let idle =
        app.session.tool.tool == figcraft_engine::ToolId::Move && app.session.ds().is_ok_and(|d| d.text_edit.is_none() && d.vector_edit.is_none());
    if idle && let Some((index, vertical)) = guide_at(app, pos) {
        ctx.set_cursor_icon(cursor(vertical));
        if pressed {
            ctx.data_mut(|d| d.insert_temp(drag_id(), GuideDrag::Move { index, vertical }));
        }
        return true;
    }
    false
}

/// Draw the page's guides (under the rulers) and the one being dragged with its position.
pub fn draw(app: &FigApp, painter: &egui::Painter) {
    if !rulers_on(app) {
        return;
    }
    let r = app.canvas.rect;
    let ctx = painter.ctx();
    let drag = ctx.data(|d| d.get_temp::<GuideDrag>(drag_id()));
    let moving = match drag {
        Some(GuideDrag::Move { index, .. }) => Some(index),
        _ => None,
    };
    let hover = ctx.input(|i| i.pointer.hover_pos()).filter(|p| r.contains(*p)).and_then(|p| guide_at(app, p)).map(|(i, _)| i);
    let line = |painter: &egui::Painter, vertical: bool, s: f32, width: f32| {
        if vertical {
            painter.line_segment([pos2(s, r.min.y), pos2(s, r.max.y)], Stroke::new(width, GUIDE));
        } else {
            painter.line_segment([pos2(r.min.x, s), pos2(r.max.x, s)], Stroke::new(width, GUIDE));
        }
    };
    for (i, g) in page_guides(app).iter().enumerate() {
        if Some(i) == moving {
            continue;
        }
        let s = screen_of(app, g);
        line(painter, g.vertical, s, if hover == Some(i) && drag.is_none() { 2.0 } else { 1.0 });
        if hover == Some(i) && drag.is_none() {
            label(painter, r, g.vertical, s, g.offset);
        }
    }
    if let Some((vertical, offset)) = ctx.data(|d| d.get_temp::<(bool, f64)>(preview_id())) {
        let g = Guide { vertical, offset };
        let s = screen_of(app, &g);
        line(painter, vertical, s, 1.0);
        label(painter, r, vertical, s, offset);
    }
}

/// The guide's position, in a tag next to its ruler.
fn label(painter: &egui::Painter, r: Rect, vertical: bool, s: f32, offset: f64) {
    let text = crate::canvas::fmt(offset);
    let galley = painter.layout_no_wrap(text, semibold(10.0), Color32::WHITE);
    let size = galley.size() + vec2(8.0, 4.0);
    let at = if vertical { pos2(s + 4.0, r.min.y + RULER + 4.0) } else { pos2(r.min.x + RULER + 4.0, s + 4.0) };
    let tag = Rect::from_min_size(at, size);
    painter.rect_filled(tag, CornerRadius::same(3), GUIDE);
    painter.galley(tag.min + vec2(4.0, 2.0), galley, Color32::WHITE);
}
