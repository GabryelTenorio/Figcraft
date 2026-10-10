//! Ruler guides on the canvas: drag out of a ruler to add one, drag a guide to move it, drag it
//! back onto its ruler to remove it. A guide dropped on a frame belongs to that frame: it's kept in
//! the frame's coordinates, moves with it and spans only the frame. Guides show while the rulers
//! do; layers snap to them.

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Stroke, pos2, vec2};
use figcraft_doc::{Document, NodeId};
use figcraft_geom::Point;
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
    /// Moving guide `index` of `owner` (a page or frame).
    Move { owner: NodeId, index: usize, vertical: bool },
}

/// A guide where it lands on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    /// The page or frame it belongs to.
    owner: NodeId,
    index: usize,
    vertical: bool,
    /// Its offset in the owner's coordinates (what its label shows).
    offset: f64,
    /// Its page position (x for vertical guides, y for horizontal).
    at: f64,
    /// A frame guide's extent across it on the page (y span for vertical guides); None: the page.
    span: Option<(f64, f64)>,
}

fn drag_id() -> Id {
    Id::new("guide_drag")
}

/// The dragged guide as it would land.
fn preview_id() -> Id {
    Id::new("guide_preview")
}

/// The page's guides and those of the frames on it (frames last, so they're found first).
fn placed_guides(app: &FigApp) -> Vec<Placed> {
    let Ok(ds) = app.session.ds() else { return vec![] };
    let doc = &ds.doc;
    let mut out: Vec<Placed> = doc
        .node(ds.page)
        .map(|n| n.guides())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, g)| Placed { owner: ds.page, index, vertical: g.vertical, offset: g.offset, at: g.offset, span: None })
        .collect();
    let mut stack: Vec<(NodeId, usize)> = doc.node(ds.page).map(|n| n.children.iter().map(|c| (*c, 0)).collect()).unwrap_or_default();
    while let Some((id, depth)) = stack.pop() {
        let Some(n) = doc.node(id).filter(|n| n.visible && depth < 64) else { continue };
        if !n.guides.is_empty() {
            for (index, g) in n.guides.iter().enumerate() {
                if let Some(p) = place(doc, id, index, g.vertical, g.offset) {
                    out.push(p);
                }
            }
        }
        stack.extend(n.children.iter().map(|c| (*c, depth + 1)));
    }
    out
}

/// Where guide `index` of frame `owner` at `offset` (frame coordinates) lands on the page.
fn place(doc: &Document, owner: NodeId, index: usize, vertical: bool, offset: f64) -> Option<Placed> {
    let b = doc.world_bounds(owner)?;
    let xf = doc.world_transform(owner);
    let (at, span) = if vertical { ((xf * Point::new(offset, 0.0)).x, (b.y0, b.y1)) } else { ((xf * Point::new(0.0, offset)).y, (b.x0, b.x1)) };
    Some(Placed { owner, index, vertical, offset, at, span: Some(span) })
}

/// A guide dropped at page point `p`: on the frame under it (in the frame's coordinates) or on the
/// page.
fn drop_target(app: &FigApp, p: Point, vertical: bool, snap: bool) -> Option<Placed> {
    let ds = app.session.ds().ok()?;
    let round = |v: f64| if snap { v.round() } else { (v * 100.0).round() / 100.0 };
    let frame = figcraft_engine::cmd::create::frame_at(&ds.doc, ds.page, p, &[]).filter(|f| ds.doc.node(*f).is_some_and(|n| n.kind.is_frame_like()));
    match frame {
        Some(f) => {
            let local = ds.doc.world_transform(f).inverse() * p;
            place(&ds.doc, f, usize::MAX, vertical, round(if vertical { local.x } else { local.y }))
        }
        None => {
            let at = round(if vertical { p.x } else { p.y });
            Some(Placed { owner: ds.page, index: usize::MAX, vertical, offset: at, at, span: None })
        }
    }
}

/// Where the moved guide of `owner` lands with the pointer at page point `p`.
fn moved(app: &FigApp, owner: NodeId, index: usize, vertical: bool, p: Point, snap: bool) -> Option<Placed> {
    let ds = app.session.ds().ok()?;
    let round = |v: f64| if snap { v.round() } else { (v * 100.0).round() / 100.0 };
    if owner == ds.page {
        let at = round(if vertical { p.x } else { p.y });
        return Some(Placed { owner, index, vertical, offset: at, at, span: None });
    }
    let local = ds.doc.world_transform(owner).inverse() * p;
    place(&ds.doc, owner, index, vertical, round(if vertical { local.x } else { local.y }))
}

/// A guide command's params: frame guides name their frame, page guides need no owner.
fn owned(owner: NodeId, page: Option<NodeId>, mut params: serde_json::Value) -> serde_json::Value {
    if Some(owner) != page
        && let Some(o) = params.as_object_mut()
    {
        o.insert("frame".into(), json!(owner));
    }
    params
}

fn rulers_on(app: &FigApp) -> bool {
    app.session.ds().is_ok_and(|d| d.view.rulers)
}

/// Screen coordinate of a guide (x for vertical guides, y for horizontal).
fn screen_of(app: &FigApp, g: &Placed) -> f32 {
    if g.vertical { app.to_screen(g.at, 0.0).x } else { app.to_screen(0.0, g.at).y }
}

/// The guide's line on screen, clipped to the canvas `r`.
fn segment(app: &FigApp, g: &Placed, r: Rect) -> [Pos2; 2] {
    let s = screen_of(app, g);
    let (a, b) = match g.span {
        Some((a, b)) if g.vertical => (app.to_screen(0.0, a).y.max(r.min.y), app.to_screen(0.0, b).y.min(r.max.y)),
        Some((a, b)) => (app.to_screen(a, 0.0).x.max(r.min.x), app.to_screen(b, 0.0).x.min(r.max.x)),
        None if g.vertical => (r.min.y, r.max.y),
        None => (r.min.x, r.max.x),
    };
    if g.vertical { [pos2(s, a), pos2(s, b)] } else { [pos2(a, s), pos2(b, s)] }
}

/// The guide within a few points of `pos` (frame guides and later ones first).
fn guide_at(app: &FigApp, pos: Pos2) -> Option<Placed> {
    let r = app.canvas.rect;
    placed_guides(app).into_iter().rev().find(|g| {
        let [a, b] = segment(app, g, r);
        let (across, along, lo, hi) = if g.vertical { (pos.x - a.x, pos.y, a.y, b.y) } else { (pos.y - a.y, pos.x, a.x, b.x) };
        across.abs() <= 3.0 && along >= lo - 3.0 && along <= hi + 3.0
    })
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
        let at = match drag {
            GuideDrag::New { vertical } => drop_target(app, p, vertical, snap),
            GuideDrag::Move { owner, index, vertical } => moved(app, owner, index, vertical, p, snap),
        };
        ctx.data_mut(|d| match at {
            Some(g) => {
                d.insert_temp(preview_id(), g);
            }
            None => d.remove::<Placed>(preview_id()),
        });
        ctx.set_cursor_icon(cursor(vertical));
        if released {
            ctx.data_mut(|d| {
                d.remove::<GuideDrag>(drag_id());
                d.remove::<Placed>(preview_id());
            });
            // Released over its own ruler: no guide (a moved one is removed).
            let on_ruler = if vertical { pos.x < r.min.x + RULER } else { pos.y < r.min.y + RULER };
            let page = app.session.ds().map(|d| d.page).ok();
            match (drag, on_ruler, at) {
                (GuideDrag::New { .. }, false, Some(g)) => {
                    app.run("guide.add", owned(g.owner, page, json!({"vertical": vertical, "offset": g.offset})));
                }
                (GuideDrag::Move { owner, index, .. }, true, _) => {
                    app.run("guide.remove", owned(owner, page, json!({"index": index})));
                }
                (GuideDrag::Move { owner, index, .. }, false, Some(g)) => {
                    app.run("guide.move", owned(owner, page, json!({"index": index, "offset": g.offset})));
                }
                _ => {}
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
    if idle && let Some(g) = guide_at(app, pos) {
        ctx.set_cursor_icon(cursor(g.vertical));
        if pressed {
            ctx.data_mut(|d| d.insert_temp(drag_id(), GuideDrag::Move { owner: g.owner, index: g.index, vertical: g.vertical }));
        }
        return true;
    }
    false
}

/// Draw the page's and its frames' guides (under the rulers) and the one being dragged with its
/// position.
pub fn draw(app: &FigApp, painter: &egui::Painter) {
    if !rulers_on(app) {
        return;
    }
    let r = app.canvas.rect;
    let ctx = painter.ctx();
    let drag = ctx.data(|d| d.get_temp::<GuideDrag>(drag_id()));
    let moving = match drag {
        Some(GuideDrag::Move { owner, index, .. }) => Some((owner, index)),
        _ => None,
    };
    let hover = if drag.is_none() { ctx.input(|i| i.pointer.hover_pos()).filter(|p| r.contains(*p)).and_then(|p| guide_at(app, p)) } else { None };
    for g in placed_guides(app) {
        if Some((g.owner, g.index)) == moving {
            continue;
        }
        let hovered = hover == Some(g);
        painter.line_segment(segment(app, &g, r), Stroke::new(if hovered { 2.0 } else { 1.0 }, GUIDE));
        if hovered {
            label(app, painter, r, &g);
        }
    }
    if let Some(g) = ctx.data(|d| d.get_temp::<Placed>(preview_id())) {
        painter.line_segment(segment(app, &g, r), Stroke::new(1.0, GUIDE));
        label(app, painter, r, &g);
    }
}

/// The guide's offset (in its page's or frame's coordinates), in a tag next to its ruler or at the
/// start of a frame guide.
fn label(app: &FigApp, painter: &egui::Painter, r: Rect, g: &Placed) {
    let galley = painter.layout_no_wrap(crate::canvas::fmt(g.offset), semibold(10.0), Color32::WHITE);
    let size = galley.size() + vec2(8.0, 4.0);
    let [a, _] = segment(app, g, r);
    let at = if g.vertical { pos2(a.x + 4.0, a.y.max(r.min.y + RULER) + 4.0) } else { pos2(a.x.max(r.min.x + RULER) + 4.0, a.y + 4.0) };
    let tag = Rect::from_min_size(at, size);
    painter.rect_filled(tag, CornerRadius::same(3), GUIDE);
    painter.galley(tag.min + vec2(4.0, 2.0), galley, Color32::WHITE);
}
