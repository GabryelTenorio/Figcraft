//! Pointer tools. The UI (or an agent through the control channel) feeds [`PointerEvent`]s in page
//! coordinates; tools turn gestures into document edits through interactions (one undo step per
//! gesture) and publish overlay hints (marquee, guides, size label, cursor) for the canvas.

use std::collections::BTreeMap;

use figcraft_doc::geometry::select_target;
use figcraft_doc::*;
use figcraft_geom::{Affine, Point, Rect, Vec2};
use serde::Serialize;
use serde_json::json;

use crate::cmd::arrange::move_world;
use crate::cmd::create::frame_at;
use crate::cmd::edit::roots;
use crate::snap::{Guide, Targets};
use crate::{Result, Session, textedit};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
pub enum ToolId {
    #[default]
    Move,
    Scale,
    Hand,
    Frame,
    Section,
    Slice,
    Rectangle,
    Line,
    Arrow,
    Ellipse,
    Polygon,
    Star,
    Pen,
    Pencil,
    Text,
    Comment,
    Eyedropper,
}

impl ToolId {
    pub const ALL: [ToolId; 17] = [
        ToolId::Move,
        ToolId::Scale,
        ToolId::Hand,
        ToolId::Frame,
        ToolId::Section,
        ToolId::Slice,
        ToolId::Rectangle,
        ToolId::Line,
        ToolId::Arrow,
        ToolId::Ellipse,
        ToolId::Polygon,
        ToolId::Star,
        ToolId::Pen,
        ToolId::Pencil,
        ToolId::Text,
        ToolId::Comment,
        ToolId::Eyedropper,
    ];
    pub fn id(self) -> &'static str {
        match self {
            ToolId::Move => "move",
            ToolId::Scale => "scale",
            ToolId::Hand => "hand",
            ToolId::Frame => "frame",
            ToolId::Section => "section",
            ToolId::Slice => "slice",
            ToolId::Rectangle => "rectangle",
            ToolId::Line => "line",
            ToolId::Arrow => "arrow",
            ToolId::Ellipse => "ellipse",
            ToolId::Polygon => "polygon",
            ToolId::Star => "star",
            ToolId::Pen => "pen",
            ToolId::Pencil => "pencil",
            ToolId::Text => "text",
            ToolId::Comment => "comment",
            ToolId::Eyedropper => "eyedropper",
        }
    }
    pub fn from_id(s: &str) -> Option<ToolId> {
        ToolId::ALL.into_iter().find(|t| t.id().eq_ignore_ascii_case(s))
    }
    pub fn label(self) -> &'static str {
        match self {
            ToolId::Move => "Move",
            ToolId::Scale => "Scale",
            ToolId::Hand => "Hand tool",
            ToolId::Frame => "Frame",
            ToolId::Section => "Section",
            ToolId::Slice => "Slice",
            ToolId::Rectangle => "Rectangle",
            ToolId::Line => "Line",
            ToolId::Arrow => "Arrow",
            ToolId::Ellipse => "Ellipse",
            ToolId::Polygon => "Polygon",
            ToolId::Star => "Star",
            ToolId::Pen => "Pen",
            ToolId::Pencil => "Pencil",
            ToolId::Text => "Text",
            ToolId::Comment => "Comment",
            ToolId::Eyedropper => "Pick color",
        }
    }
    pub fn shortcut(self) -> &'static str {
        match self {
            ToolId::Move => "V",
            ToolId::Scale => "K",
            ToolId::Hand => "H",
            ToolId::Frame => "F",
            ToolId::Section => "Shift+S",
            ToolId::Slice => "S",
            ToolId::Rectangle => "R",
            ToolId::Line => "L",
            ToolId::Arrow => "Shift+L",
            ToolId::Ellipse => "O",
            ToolId::Pen => "P",
            ToolId::Pencil => "Shift+P",
            ToolId::Text => "T",
            ToolId::Comment => "C",
            ToolId::Eyedropper => "I",
            _ => "",
        }
    }
    fn draws_shape(self) -> bool {
        matches!(
            self,
            ToolId::Frame
                | ToolId::Section
                | ToolId::Slice
                | ToolId::Rectangle
                | ToolId::Ellipse
                | ToolId::Polygon
                | ToolId::Star
                | ToolId::Line
                | ToolId::Arrow
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    /// Pointer moved without a button.
    Hover,
    DoubleClick,
}

/// A pointer event in page coordinates (and screen coordinates for panning).
#[derive(Clone, Debug, Serialize)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub x: f64,
    pub y: f64,
    /// Screen position (logical pixels).
    pub sx: f64,
    pub sy: f64,
    pub shift: bool,
    pub alt: bool,
    /// Cmd on macOS, Ctrl elsewhere.
    pub cmd: bool,
    pub ctrl: bool,
    /// Screen pixels per page unit.
    pub zoom: f64,
}

impl PointerEvent {
    pub fn at(kind: PointerKind, x: f64, y: f64, zoom: f64) -> Self {
        Self { kind, x, y, sx: x * zoom, sy: y * zoom, shift: false, alt: false, cmd: false, ctrl: false, zoom }
    }
    pub(crate) fn p(&self) -> Point {
        Point::new(self.x, self.y)
    }
    fn tol(&self) -> f64 {
        1.0 / self.zoom.max(1e-6)
    }
}

/// Cursor hint for the UI.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Default)]
pub enum Cursor {
    #[default]
    Default,
    Crosshair,
    Move,
    /// Resize along an angle (degrees, 0 = horizontal).
    Resize(f64),
    Rotate,
    Grab,
    Grabbing,
    Text,
    Pen,
    Copy,
}

/// Selection handle kinds: corners (0 tl, 1 tr, 2 br, 3 bl), edges (0 t, 1 r, 2 b, 3 l) and the
/// rotation zones outside the corners.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Handle {
    Corner(u8),
    Edge(u8),
    Rotate(u8),
}

/// What the canvas should draw on top of the scene.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Overlay {
    pub marquee: Option<[f64; 4]>,
    pub guides: Vec<Guide>,
    pub cursor: Cursor,
    /// Pen tool: the in-progress path's anchor points and the rubber band to the pointer.
    pub pen: Vec<[f64; 2]>,
    pub rubber: Option<[f64; 4]>,
    /// Frame/section the pointer is drawing or dropping into (highlighted).
    pub drop_target: Option<NodeId>,
    /// Auto layout insertion indicator (x0, y0, x1, y1).
    pub insert_line: Option<[f64; 4]>,
    /// Vector edit mode: what the pointer is over, the lasso being drawn and the closed area the
    /// paint bucket would fill (page-space polygons).
    pub vector_hover: Option<crate::vectoredit::VHit>,
    pub lasso: Vec<[f64; 2]>,
    /// The eraser brush under the pointer (x, y, radius in page units).
    pub brush: Option<[f64; 3]>,
    pub face: Vec<Vec<[f64; 2]>>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResizeStart {
    /// Node id → (world transform, w, h) at the start.
    nodes: Vec<(NodeId, Affine, f64, f64)>,
    /// Single node: its world transform and size; else the AABB.
    single: Option<(Affine, f64, f64)>,
    bounds: Rect,
    handle: Handle,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum Gesture {
    #[default]
    None,
    Marquee {
        start: Point,
        #[allow(dead_code)]
        additive: bool,
        base: Vec<NodeId>,
        within: Option<NodeId>,
    },
    Move {
        start: Point,
        ids: Vec<NodeId>,
        bounds: Rect,
        targets: Targets,
        duplicate: bool,
        moved: bool,
    },
    Resize {
        start: Point,
        r: Box<ResizeStart>,
        scale_tool: bool,
    },
    Rotate {
        center: Point,
        start_angle: f64,
        ids: Vec<NodeId>,
    },
    Draw {
        start: Point,
        id: Option<NodeId>,
        parent: NodeId,
        targets: Targets,
    },
    Pan {
        sx: f64,
        sy: f64,
        cx: f64,
        cy: f64,
    },
    Pencil {
        points: Vec<Point>,
    },
    TextSelect {
        id: NodeId,
    },
    PenDrag {
        anchor: Point,
    },
    /// Vector edit mode gestures (handled in `vectoredit`).
    Vector(crate::vectoredit::VGesture),
}

/// The pen tool's in-progress path (page coordinates).
#[derive(Clone, Debug, Default)]
pub struct PenPath {
    pub id: Option<NodeId>,
    pub parent: Option<NodeId>,
    /// Anchors with (in, out) handle offsets.
    pub points: Vec<(Point, Vec2, Vec2)>,
    pub closed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ToolState {
    pub tool: ToolId,
    pub(crate) gesture: Gesture,
    pub overlay: Overlay,
    pub pen: Option<PenPath>,
    /// Tool to return to after a temporary tool (space → hand).
    pub temp_from: Option<ToolId>,
    /// Last click (for double-click detection by agents that only send downs).
    pub last_hover: Option<NodeId>,
}

impl ToolState {
    pub fn busy(&self) -> bool {
        !matches!(self.gesture, Gesture::None)
    }
}

const HANDLE_PX: f64 = 5.0;
const ROTATE_PX: f64 = 18.0;
const SNAP_PX: f64 = 5.0;

/// Corners of the selection's handle box in page coordinates (tl, tr, br, bl).
pub fn selection_box(doc: &Document, sel: &[NodeId]) -> Option<[Point; 4]> {
    let ids = roots(doc, sel);
    if ids.len() == 1 {
        let id = ids.first().copied()?;
        let n = doc.node(id)?;
        let w = doc.world_transform(id);
        let (x0, y0, x1, y1) = (0.0, 0.0, n.width, n.height);
        return Some([w * Point::new(x0, y0), w * Point::new(x1, y0), w * Point::new(x1, y1), w * Point::new(x0, y1)]);
    }
    let b = doc.bounds_of(&ids)?;
    Some([Point::new(b.x0, b.y0), Point::new(b.x1, b.y0), Point::new(b.x1, b.y1), Point::new(b.x0, b.y1)])
}

/// Which handle (if any) is under `p`.
pub fn handle_at(doc: &Document, sel: &[NodeId], p: Point, zoom: f64) -> Option<Handle> {
    if sel.is_empty() {
        return None;
    }
    let corners = selection_box(doc, sel)?;
    let tol = HANDLE_PX / zoom.max(1e-6);
    // Lines (zero height) only get end handles.
    let is_line = sel.len() == 1 && sel.first().and_then(|i| doc.node(*i)).is_some_and(|n| matches!(n.kind, NodeKind::Line));
    for (i, c) in corners.iter().enumerate() {
        if is_line && (i == 3 || i == 2) {
            continue;
        }
        if (*c - p).hypot() <= tol * 1.6 {
            return Some(Handle::Corner(i as u8));
        }
    }
    if !is_line {
        for i in 0..4 {
            let (Some(a), Some(b)) = (corners.get(i), corners.get((i + 1) % 4)) else { continue };
            let seg = figcraft_geom::kurbo::Line::new(*a, *b);
            use figcraft_geom::kurbo::ParamCurveNearest;
            let near = seg.nearest(p, 0.001);
            if near.distance_sq.sqrt() <= tol && near.t > 0.05 && near.t < 0.95 {
                return Some(Handle::Edge(i as u8));
            }
        }
    }
    // Rotation zones: just outside each corner.
    let center = Point::new(corners.iter().map(|c| c.x).sum::<f64>() / 4.0, corners.iter().map(|c| c.y).sum::<f64>() / 4.0);
    let rot = ROTATE_PX / zoom.max(1e-6);
    for (i, c) in corners.iter().enumerate() {
        let d = (*c - p).hypot();
        let outward = (p - *c).dot(*c - center) > 0.0;
        if d <= rot && d > tol && outward {
            return Some(Handle::Rotate(i as u8));
        }
    }
    None
}

fn resize_cursor(doc: &Document, sel: &[NodeId], h: Handle) -> Cursor {
    let Some(c) = selection_box(doc, sel) else { return Cursor::Default };
    let angle = |a: Point, b: Point| (b - a).atan2().to_degrees();
    match h {
        Handle::Corner(i) => {
            let a = c.get(i as usize).copied().unwrap_or_default();
            let b = c.get((i as usize + 2) % 4).copied().unwrap_or_default();
            Cursor::Resize(angle(a, b))
        }
        Handle::Edge(i) => {
            let a = c.get(i as usize).copied().unwrap_or_default();
            let b = c.get((i as usize + 1) % 4).copied().unwrap_or_default();
            Cursor::Resize(angle(a, b) + 90.0)
        }
        Handle::Rotate(_) => Cursor::Rotate,
    }
}

impl Session {
    pub fn set_tool(&mut self, t: ToolId) {
        if self.tool.tool == ToolId::Pen && t != ToolId::Pen {
            let _ = self.pen_finish();
        }
        if !matches!(t, ToolId::Move | ToolId::Scale | ToolId::Hand) && self.ds().is_ok_and(|d| d.vector_edit.is_some()) {
            let _ = crate::vectoredit::end(self);
        }
        self.tool.tool = t;
        self.tool.gesture = Gesture::None;
        self.tool.overlay = Overlay::default();
        if let Ok(ds) = self.ds_mut() {
            if t != ToolId::Text && t != ToolId::Move {
                ds.text_edit = None;
            }
            ds.revision += 1;
        }
    }

    /// Feed one pointer event to the current tool.
    pub fn pointer(&mut self, ev: &PointerEvent) -> Result<()> {
        let tool = self.tool.tool;
        if matches!(tool, ToolId::Move | ToolId::Scale) && self.ds().is_ok_and(|d| d.vector_edit.is_some()) && self.vector_pointer(ev)? {
            return Ok(());
        }
        match ev.kind {
            PointerKind::Hover => self.hover(ev),
            PointerKind::Down => match tool {
                ToolId::Move | ToolId::Scale => self.move_down(ev),
                ToolId::Hand => {
                    let v = &self.ds()?.view;
                    self.tool.gesture = Gesture::Pan { sx: ev.sx, sy: ev.sy, cx: v.cx, cy: v.cy };
                    self.tool.overlay.cursor = Cursor::Grabbing;
                    Ok(())
                }
                ToolId::Text => self.text_down(ev),
                ToolId::Pen => self.pen_down(ev),
                ToolId::Pencil => {
                    self.tool.gesture = Gesture::Pencil { points: vec![ev.p()] };
                    Ok(())
                }
                ToolId::Comment => {
                    self.request_ui("comment.new", json!({"x": ev.x, "y": ev.y}));
                    Ok(())
                }
                ToolId::Eyedropper => self.eyedrop(ev),
                t if t.draws_shape() => self.draw_down(ev),
                _ => Ok(()),
            },
            PointerKind::Move => self.drag(ev),
            PointerKind::Up => self.up(ev),
            PointerKind::DoubleClick => self.double_click(ev),
        }
    }

    fn hover(&mut self, ev: &PointerEvent) -> Result<()> {
        let tool = self.tool.tool;
        let ds = self.ds()?;
        let (doc, sel, page) = (ds.doc.clone(), ds.selection.clone(), ds.page);
        let mut cursor = match tool {
            ToolId::Hand => Cursor::Grab,
            ToolId::Text => Cursor::Text,
            ToolId::Pen | ToolId::Pencil => Cursor::Pen,
            t if t.draws_shape() => Cursor::Crosshair,
            _ => Cursor::Default,
        };
        let mut hover = None;
        let mut guides = Vec::new();
        if matches!(tool, ToolId::Move | ToolId::Scale) {
            if let Some(h) = handle_at(&doc, &sel, ev.p(), ev.zoom) {
                cursor = resize_cursor(&doc, &sel, h);
            } else if let Some(leaf) = doc.hit_test(page, ev.p(), ev.tol() * 2.0) {
                let t = select_target(&doc, leaf, &sel, ev.cmd);
                hover = Some(t);
                if ev.alt && !sel.is_empty() && !sel.contains(&t) {
                    // Alt: measure from the selection to the hovered layer.
                    if let (Some(a), Some(b)) = (doc.bounds_of(&sel), doc.world_bounds(t)) {
                        guides = crate::snap::measure(a, b);
                    }
                }
            } else if ev.alt && !sel.is_empty() {
                // Alt over empty canvas inside a frame: distances to the parent.
                if let Some(a) = doc.bounds_of(&sel)
                    && let Some(p) = sel.first().and_then(|i| doc.node(*i)).and_then(|n| n.parent).filter(|p| *p != page)
                    && let Some(b) = doc.world_bounds(p)
                {
                    guides = crate::snap::measure(a, b);
                }
            }
            if ev.alt && hover.is_some() && sel.is_empty() {
                cursor = Cursor::Default;
            }
        }
        self.tool.overlay.cursor = cursor;
        self.tool.overlay.guides = guides;
        if tool == ToolId::Pen
            && let Some(pen) = &self.tool.pen
            && let Some((last, _, _)) = pen.points.last()
        {
            self.tool.overlay.rubber = Some([last.x, last.y, ev.x, ev.y]);
        }
        let ds = self.ds_mut()?;
        if ds.hover != hover {
            ds.hover = hover;
            ds.revision += 1;
        }
        Ok(())
    }

    fn move_down(&mut self, ev: &PointerEvent) -> Result<()> {
        let ds = self.ds()?;
        let (doc, sel, page) = (ds.doc.clone(), ds.selection.clone(), ds.page);
        // Clicking outside the text being edited ends editing.
        if let Some(te) = ds.text_edit.clone() {
            let inside = doc.world_bounds(te.id).is_some_and(|b| b.inflate(ev.tol() * 4.0, ev.tol() * 4.0).contains(ev.p()));
            if inside {
                return self.text_click(te.id, ev, ev.shift);
            }
            textedit::end(self)?;
        }
        // Handles first.
        if let Some(h) = handle_at(&doc, &sel, ev.p(), ev.zoom) {
            let ids = roots(&doc, &sel);
            if let Handle::Rotate(_) = h {
                let Some(b) = selection_box(&doc, &ids) else { return Ok(()) };
                let center = Point::new(b.iter().map(|c| c.x).sum::<f64>() / 4.0, b.iter().map(|c| c.y).sum::<f64>() / 4.0);
                self.begin_interaction("Rotate")?;
                self.tool.gesture = Gesture::Rotate { center, start_angle: (ev.p() - center).atan2(), ids };
                return Ok(());
            }
            let nodes: Vec<(NodeId, Affine, f64, f64)> =
                ids.iter().filter_map(|i| doc.node(*i).map(|n| (*i, doc.world_transform(*i), n.width, n.height))).collect();
            let single = if ids.len() == 1 { nodes.first().map(|(_, w, a, b)| (*w, *a, *b)) } else { None };
            let bounds = doc.bounds_of(&ids).unwrap_or_default();
            self.begin_interaction(if self.tool.tool == ToolId::Scale { "Scale" } else { "Resize" })?;
            self.tool.gesture = Gesture::Resize {
                start: ev.p(),
                r: Box::new(ResizeStart { nodes, single, bounds, handle: h }),
                scale_tool: self.tool.tool == ToolId::Scale,
            };
            return Ok(());
        }
        let leaf = doc.hit_test(page, ev.p(), ev.tol() * 2.0);
        match leaf {
            Some(leaf) => {
                let target = select_target(&doc, leaf, &sel, ev.cmd);
                // Clicking inside the current selection's box keeps it (drag them all).
                let in_sel_box = sel.len() > 1 && doc.bounds_of(&sel).is_some_and(|b| b.contains(ev.p())) && !ev.shift;
                let new_sel = if in_sel_box && (sel.contains(&target) || sel.iter().any(|s| doc.is_ancestor(*s, target))) {
                    sel.clone()
                } else if ev.shift {
                    let mut s2 = sel.clone();
                    if let Some(pos) = s2.iter().position(|x| *x == target) {
                        s2.remove(pos);
                    } else {
                        s2.push(target);
                    }
                    s2
                } else if sel.contains(&target) {
                    sel.clone()
                } else {
                    vec![target]
                };
                self.set_selection(new_sel.clone());
                if new_sel.is_empty() || doc.node(target).is_some_and(|n| n.locked) {
                    return Ok(());
                }
                let ids = roots(&doc, &new_sel);
                let bounds = doc.bounds_of(&ids).unwrap_or_default();
                let targets = Targets::gather(&doc, page, &ids);
                self.begin_interaction(if ev.alt { "Duplicate" } else { "Move" })?;
                self.tool.gesture = Gesture::Move { start: ev.p(), ids, bounds, targets, duplicate: ev.alt, moved: false };
                self.tool.overlay.cursor = if ev.alt { Cursor::Copy } else { Cursor::Default };
            }
            None => {
                // Marquee; starting inside a top-level frame selects its children.
                let within = frame_at(&doc, page, ev.p(), &[]).filter(|f| doc.node(*f).is_some_and(|n| n.parent == Some(page)));
                let within = within.filter(|f| doc.world_bounds(*f).is_some_and(|b| b.contains(ev.p())));
                if !ev.shift {
                    self.set_selection(vec![]);
                }
                self.tool.gesture = Gesture::Marquee { start: ev.p(), additive: ev.shift, base: if ev.shift { sel } else { vec![] }, within };
            }
        }
        Ok(())
    }

    fn drag(&mut self, ev: &PointerEvent) -> Result<()> {
        let g = std::mem::take(&mut self.tool.gesture);
        let res = match &g {
            Gesture::None => return self.hover(ev),
            Gesture::Marquee { start, additive: _, base, within } => {
                let r = Rect::from_points(*start, ev.p());
                self.tool.overlay.marquee = Some([r.x0, r.y0, r.x1, r.y1]);
                let doc = self.doc()?.clone();
                let page = self.page()?;
                let mut hits = figcraft_doc::geometry::marquee_select(&doc, page, r, *within);
                // A marquee over a frame from outside selects the frame only when it covers it fully.
                if within.is_none() {
                    hits.retain(|h| {
                        let full = doc.world_bounds(*h).is_some_and(|b| r.contains(b.origin()) && r.contains(Point::new(b.x1, b.y1)));
                        full || !doc.node(*h).is_some_and(|n| n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. }))
                    });
                }
                let mut sel = base.clone();
                for h in hits {
                    if !sel.contains(&h) {
                        sel.push(h);
                    }
                }
                self.set_selection(sel);
                Ok(())
            }
            Gesture::Move { start, ids, bounds, targets, duplicate, .. } => {
                let mut d = ev.p() - *start;
                if ev.shift {
                    if d.x.abs() > d.y.abs() {
                        d.y = 0.0;
                    } else {
                        d.x = 0.0;
                    }
                }
                let snap_on = self.ds().map(|ds| ds.view.snap_objects).unwrap_or(true) && !ev.cmd;
                let mut guides = Vec::new();
                if snap_on {
                    let moved = Rect::new(bounds.x0 + d.x, bounds.y0 + d.y, bounds.x1 + d.x, bounds.y1 + d.y);
                    let (sx, sy, g2) = targets.snap_rect(moved, SNAP_PX / ev.zoom.max(1e-6));
                    d.x += sx;
                    d.y += sy;
                    guides = g2;
                }
                if self.ds().map(|ds| ds.view.snap_pixel).unwrap_or(true) {
                    // Keep the box's origin on whole pixels.
                    d.x = (bounds.x0 + d.x).round() - bounds.x0;
                    d.y = (bounds.y0 + d.y).round() - bounds.y0;
                }
                let ids = ids.clone();
                let dup = *duplicate;
                let page = self.page()?;
                let mut insert_line = None;
                let mut drop_target = None;
                self.preview(|doc, sel| {
                    let mut moving = ids.clone();
                    if dup {
                        let mut copies = Vec::new();
                        for i in &ids {
                            let parent = doc.get(*i)?.parent.unwrap_or(page);
                            let idx = doc.index_in_parent(*i).map(|x| x + 1);
                            copies.push(doc.duplicate_into(*i, parent, idx)?);
                        }
                        moving = copies;
                        *sel = moving.clone();
                    }
                    // Auto layout children reorder instead of moving freely.
                    let al_parent = moving
                        .first()
                        .and_then(|i| doc.node(*i))
                        .and_then(|n| n.parent)
                        .filter(|p| doc.node(*p).is_some_and(|pn| pn.layout.is_some()));
                    let flow = moving.iter().all(|i| doc.node(*i).is_some_and(|n| !n.child_layout.absolute));
                    if let (Some(pp), true) = (al_parent, flow) {
                        let pb = doc.world_bounds(pp).unwrap_or_default();
                        let still_inside = pb.inflate(8.0, 8.0).contains(ev.p());
                        if still_inside {
                            let (idx, line_at) = crate::drop::auto_layout_slot(doc, pp, ev.p(), &moving).unwrap_or((0, None));
                            insert_line = line_at;
                            let mut new_kids: Vec<NodeId> = doc.get(pp)?.children.iter().copied().filter(|c| !moving.contains(c)).collect();
                            for (k, m) in moving.iter().enumerate() {
                                new_kids.insert((idx + k).min(new_kids.len()), *m);
                            }
                            doc.get_mut(pp)?.children = new_kids;
                            return Ok(());
                        }
                        // Dragged out of the auto layout frame: becomes free and moves.
                        let kids_out = moving.clone();
                        for m in &kids_out {
                            doc.reparent(*m, page, None)?;
                        }
                    }
                    for i in &moving {
                        move_world(doc, *i, d.x, d.y)?;
                    }
                    // Drop target: the frame under the pointer.
                    drop_target = frame_at(doc, page, ev.p(), &moving);
                    Ok(())
                })?;
                self.tool.overlay.guides = guides;
                self.tool.overlay.insert_line = insert_line;
                self.tool.overlay.drop_target = drop_target;
                self.tool.gesture = g.clone();
                if let Gesture::Move { moved, .. } = &mut self.tool.gesture {
                    *moved = true;
                }
                return Ok(());
            }
            Gesture::Resize { start, r, scale_tool } => self.resize_drag(ev, *start, r, *scale_tool),
            Gesture::Rotate { center, start_angle, ids } => {
                let mut a = ((ev.p() - *center).atan2() - start_angle).to_degrees();
                if ev.shift {
                    a = (a / 15.0).round() * 15.0;
                }
                let (c, ids) = (*center, ids.clone());
                self.preview(|doc, _| {
                    for i in &ids {
                        crate::cmd::arrange::rotate_world(doc, *i, c, -a)?;
                    }
                    Ok(())
                })?;
                self.tool.overlay.cursor = Cursor::Rotate;
                Ok(())
            }
            Gesture::Draw { start, id, parent, targets } => {
                let (start, id, parent, targets) = (*start, *id, *parent, targets.clone());
                self.tool.gesture = g;
                return self.draw_drag(ev, start, id, parent, &targets);
            }
            Gesture::Pan { sx, sy, cx, cy } => {
                let ds = self.ds_mut()?;
                let z = ds.view.zoom.max(1e-6);
                ds.view.cx = cx - (ev.sx - sx) / z;
                ds.view.cy = cy - (ev.sy - sy) / z;
                ds.revision += 1;
                Ok(())
            }
            Gesture::Pencil { points } => {
                let mut pts = points.clone();
                if pts.last().is_none_or(|l| (*l - ev.p()).hypot() > 0.5 / ev.zoom.max(1e-6)) {
                    pts.push(ev.p());
                }
                self.tool.overlay.pen = pts.iter().map(|p| [p.x, p.y]).collect();
                self.tool.gesture = Gesture::Pencil { points: pts };
                if let Ok(ds) = self.ds_mut() {
                    ds.revision += 1;
                }
                return Ok(());
            }
            Gesture::TextSelect { id } => {
                let id = *id;
                let idx = self.text_index_at(id, ev.p())?;
                textedit::set_caret(self, idx, true)?;
                Ok(())
            }
            Gesture::Vector(_) => Ok(()),
            Gesture::PenDrag { anchor } => {
                let out = ev.p() - *anchor;
                if let Some(pen) = self.tool.pen.as_mut()
                    && let Some(last) = pen.points.last_mut()
                {
                    last.2 = out;
                    last.1 = -out;
                }
                self.pen_update()?;
                Ok(())
            }
        };
        self.tool.gesture = g;
        res
    }

    fn resize_drag(&mut self, ev: &PointerEvent, start: Point, r: &ResizeStart, scale_tool: bool) -> Result<()> {
        let page = self.page()?;
        let snap = self.ds().map(|d| d.view.snap_objects).unwrap_or(true) && !ev.cmd;
        let ids: Vec<NodeId> = r.nodes.iter().map(|n| n.0).collect();
        let targets = if snap { Some(Targets::gather(self.doc()?, page, &ids)) } else { None };
        let mut p = ev.p();
        let mut guides = Vec::new();
        if let Some(t) = &targets {
            let (dx, dy, g) = t.snap_point(p.x, p.y, SNAP_PX / ev.zoom.max(1e-6));
            p.x += dx;
            p.y += dy;
            guides = g;
        }
        let _ = start;
        // In the reference frame: single node → its local space; several → page space AABB.
        let (frame, w0, h0) = match r.single {
            Some((w, a, b)) => (w, a, b),
            None => (Affine::translate((r.bounds.x0, r.bounds.y0)), r.bounds.width(), r.bounds.height()),
        };
        let q = frame.inverse() * p;
        let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, w0, h0);
        match r.handle {
            Handle::Corner(0) => {
                x0 = q.x;
                y0 = q.y;
            }
            Handle::Corner(1) => {
                x1 = q.x;
                y0 = q.y;
            }
            Handle::Corner(2) => {
                x1 = q.x;
                y1 = q.y;
            }
            Handle::Corner(_) => {
                x0 = q.x;
                y1 = q.y;
            }
            Handle::Edge(0) => y0 = q.y,
            Handle::Edge(1) => x1 = q.x,
            Handle::Edge(2) => y1 = q.y,
            Handle::Edge(_) => x0 = q.x,
            Handle::Rotate(_) => {}
        }
        let is_line =
            r.single.is_some() && ids.first().and_then(|i| self.doc().ok()?.node(*i).map(|n| matches!(n.kind, NodeKind::Line))).unwrap_or(false);
        if is_line {
            // Line end handles: move the end point freely (length + angle).
            let (a, b) = match r.handle {
                Handle::Corner(0) => (p, frame * Point::new(w0, 0.0)),
                _ => (frame * Point::new(0.0, 0.0), p),
            };
            let id = ids.first().copied().unwrap_or(NodeId(0));
            self.preview(|doc, _| {
                let mut v = b - a;
                if ev.shift {
                    let ang = (v.atan2() / (std::f64::consts::PI / 4.0)).round() * std::f64::consts::PI / 4.0;
                    v = Vec2::from_angle(ang) * v.hypot();
                }
                let len = v.hypot();
                let world = Affine::translate(a.to_vec2()) * Affine::rotate(v.atan2());
                doc.set_world_transform(id, world)?;
                figcraft_layout::set_size(doc, id, len, 0.0);
                Ok(())
            })?;
            self.tool.overlay.guides = guides;
            return Ok(());
        }
        // Constrain proportions (Shift, or the node's lock).
        let keep = ev.shift
            || r.single.is_some() && ids.first().and_then(|i| self.doc().ok()?.node(*i).map(|n| n.constrain_proportions)).unwrap_or(false)
            || scale_tool;
        if keep && w0 > 1e-9 && h0 > 1e-9 {
            let (sx, sy) = ((x1 - x0) / w0, (y1 - y0) / h0);
            let k = match r.handle {
                Handle::Edge(0) | Handle::Edge(2) => sy,
                Handle::Edge(_) => sx,
                _ => {
                    if sx.abs() > sy.abs() {
                        sx
                    } else {
                        sy
                    }
                }
            };
            let (nw, nh) = (w0 * k, h0 * k);
            match r.handle {
                Handle::Corner(0) => {
                    x0 = w0 - nw;
                    y0 = h0 - nh;
                }
                Handle::Corner(1) => {
                    x1 = nw;
                    y0 = h0 - nh;
                }
                Handle::Corner(2) => {
                    x1 = nw;
                    y1 = nh;
                }
                Handle::Corner(_) => {
                    x0 = w0 - nw;
                    y1 = nh;
                }
                Handle::Edge(0) | Handle::Edge(2) => {
                    let c = w0 / 2.0;
                    x0 = c - nw / 2.0;
                    x1 = c + nw / 2.0;
                    if matches!(r.handle, Handle::Edge(0)) {
                        y0 = h0 - nh;
                    } else {
                        y1 = nh;
                    }
                }
                Handle::Rotate(_) => {}
                Handle::Edge(_) => {
                    let c = h0 / 2.0;
                    y0 = c - nh / 2.0;
                    y1 = c + nh / 2.0;
                    if matches!(r.handle, Handle::Edge(3)) {
                        x0 = w0 - nw;
                    } else {
                        x1 = nw;
                    }
                }
            }
        }
        if ev.alt {
            // Resize from the centre.
            let (cx, cy) = (w0 / 2.0, h0 / 2.0);
            let hw = ((x1 - x0) / 2.0).abs().max((cx - x0).abs().max((x1 - cx).abs()));
            let hh = ((y1 - y0) / 2.0).abs().max((cy - y0).abs().max((y1 - cy).abs()));
            match r.handle {
                Handle::Edge(0) | Handle::Edge(2) => {
                    y0 = cy - hh;
                    y1 = cy + hh;
                }
                Handle::Edge(_) => {
                    x0 = cx - hw;
                    x1 = cx + hw;
                }
                _ => {
                    x0 = cx - hw;
                    x1 = cx + hw;
                    y0 = cy - hh;
                    y1 = cy + hh;
                }
            }
        }
        let (flip_x, flip_y) = (x1 < x0, y1 < y0);
        let (nx0, ny0) = (x0.min(x1), y0.min(y1));
        let (nw, nh) = ((x1 - x0).abs(), (y1 - y0).abs());
        let nodes = r.nodes.clone();
        let single = r.single.is_some();
        let handle = r.handle;
        self.preview(|doc, _| {
            if single {
                let Some((id, world, _, _)) = nodes.first() else { return Ok(()) };
                let mut t = *world * Affine::translate((nx0, ny0));
                if flip_x {
                    t *= Affine::new([-1.0, 0.0, 0.0, 1.0, nw, 0.0]);
                }
                if flip_y {
                    t *= Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, nh]);
                }
                doc.set_world_transform(*id, t)?;
                if scale_tool {
                    let (_, _, ow, _) = nodes.first().copied().unwrap_or((NodeId(0), Affine::IDENTITY, 1.0, 1.0));
                    let k = if ow > 1e-9 { nw / ow } else { 1.0 };
                    crate::cmd::arrange::scale_node(doc, *id, k)?;
                } else {
                    crate::cmd::props::resize_node(doc, *id, nw.max(0.01), nh.max(0.01))?;
                    let n = doc.get_mut(*id)?;
                    if matches!(handle, Handle::Corner(_) | Handle::Edge(1) | Handle::Edge(3)) {
                        n.child_layout.sizing_h = Sizing::Fixed;
                    }
                    if matches!(handle, Handle::Corner(_) | Handle::Edge(0) | Handle::Edge(2)) {
                        n.child_layout.sizing_v = Sizing::Fixed;
                    }
                    if let NodeKind::Text(t) = &mut n.kind {
                        t.auto_resize = match handle {
                            Handle::Edge(1) | Handle::Edge(3) => TextAutoResize::Height,
                            _ => TextAutoResize::None,
                        };
                    }
                }
                return Ok(());
            }
            // Several layers: map the old AABB onto the new one.
            let (sx, sy) = (if w0 > 1e-9 { nw / w0 } else { 1.0 }, if h0 > 1e-9 { nh / h0 } else { 1.0 });
            let origin = Point::new(r.bounds.x0, r.bounds.y0);
            let new_origin = Point::new(r.bounds.x0 + nx0, r.bounds.y0 + ny0);
            for (id, world, w, h) in &nodes {
                let o = *world * Point::ZERO;
                let no = new_origin + Vec2::new((o.x - origin.x) * sx, (o.y - origin.y) * sy);
                let c = world.as_coeffs();
                doc.set_world_transform(*id, Affine::new([c[0], c[1], c[2], c[3], no.x, no.y]))?;
                if scale_tool {
                    crate::cmd::arrange::scale_node(doc, *id, sx.min(sy))?;
                } else {
                    crate::cmd::props::resize_node(doc, *id, w * sx, h * sy)?;
                }
            }
            Ok(())
        })?;
        self.tool.overlay.guides = guides;
        Ok(())
    }

    fn draw_down(&mut self, ev: &PointerEvent) -> Result<()> {
        let ds = self.ds()?;
        let doc = ds.doc.clone();
        let page = ds.page;
        if ds.text_edit.is_some() {
            textedit::end(self)?;
        }
        // Frames drawn inside frames nest; sections only at the top or in sections.
        let parent = match self.tool.tool {
            ToolId::Section => {
                frame_at(&doc, page, ev.p(), &[]).filter(|f| doc.node(*f).is_some_and(|n| matches!(n.kind, NodeKind::Section { .. }))).unwrap_or(page)
            }
            _ => frame_at(&doc, page, ev.p(), &[]).unwrap_or(page),
        };
        let targets = Targets::gather(&doc, page, &[]);
        self.begin_interaction(self.tool.tool.label())?;
        self.tool.gesture = Gesture::Draw { start: ev.p(), id: None, parent, targets };
        Ok(())
    }

    fn draw_drag(&mut self, ev: &PointerEvent, start: Point, id: Option<NodeId>, parent: NodeId, targets: &Targets) -> Result<()> {
        let tool = self.tool.tool;
        let mut p = ev.p();
        let mut guides = Vec::new();
        if self.ds().map(|d| d.view.snap_objects).unwrap_or(true) && !ev.cmd {
            let (dx, dy, g) = targets.snap_point(p.x, p.y, SNAP_PX / ev.zoom.max(1e-6));
            p.x += dx;
            p.y += dy;
            guides = g;
        }
        let pixel = self.ds().map(|d| d.view.snap_pixel).unwrap_or(true);
        let mut created = id;
        let page = self.page()?;
        self.preview(|doc, sel| {
            let (mut a, mut b) = (start, p);
            if pixel {
                a = Point::new(a.x.round(), a.y.round());
                b = Point::new(b.x.round(), b.y.round());
            }
            let pw = doc.world_transform(parent);
            let (kind, base) = match tool {
                ToolId::Frame => (NodeKind::Frame, "Frame"),
                ToolId::Section => (NodeKind::Section { contents_hidden: false }, "Section"),
                ToolId::Slice => (NodeKind::Slice, "Slice"),
                ToolId::Rectangle => (NodeKind::Rectangle, "Rectangle"),
                ToolId::Ellipse => (NodeKind::Ellipse { start: 0.0, sweep: std::f64::consts::TAU, inner: 0.0 }, "Ellipse"),
                ToolId::Polygon => (NodeKind::Polygon { count: 3 }, "Polygon"),
                ToolId::Star => (NodeKind::Star { count: 5, inner_ratio: 0.38 }, "Star"),
                ToolId::Line => (NodeKind::Line, "Line"),
                ToolId::Arrow => (NodeKind::Line, "Arrow"),
                _ => return Ok(()),
            };
            let name = match tool {
                ToolId::Frame | ToolId::Section | ToolId::Slice => doc.next_name(base),
                _ => base.to_string(),
            };
            let nid = doc.alloc_id();
            let mut n = Node::new(nid, kind, name, 0.0, 0.0);
            if tool == ToolId::Arrow {
                n.stroke.cap_end = StrokeCap::LineArrow;
            }
            if tool == ToolId::Section {
                n.fills = vec![figcraft_color::Paint::solid(figcraft_color::Color::WHITE)];
                n.strokes = vec![figcraft_color::Paint::solid(figcraft_color::Color::from_rgb8_const(0xE6, 0xE6, 0xE6))];
                n.corners.radii = [2.0; 4];
                n.clip = false;
            }
            if matches!(tool, ToolId::Line | ToolId::Arrow) {
                let mut v = b - a;
                if ev.shift {
                    let ang = (v.atan2() / (std::f64::consts::PI / 4.0)).round() * std::f64::consts::PI / 4.0;
                    v = Vec2::from_angle(ang) * v.hypot();
                }
                n.width = v.hypot();
                n.transform = pw.inverse() * Affine::translate(a.to_vec2()) * Affine::rotate(v.atan2());
            } else {
                let mut d = b - a;
                if ev.shift {
                    let m = d.x.abs().max(d.y.abs());
                    d = Vec2::new(m * d.x.signum(), m * d.y.signum());
                }
                let (x0, y0, x1, y1) = if ev.alt { (a.x - d.x, a.y - d.y, a.x + d.x, a.y + d.y) } else { (a.x, a.y, a.x + d.x, a.y + d.y) };
                let r = Rect::new(x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
                n.width = r.width();
                n.height = r.height();
                n.transform = pw.inverse() * Affine::translate((r.x0, r.y0));
            }
            doc.insert(n, parent, None)?;
            *sel = vec![nid];
            created = Some(nid);
            let _ = page;
            Ok(())
        })?;
        self.tool.overlay.guides = guides;
        self.tool.gesture = Gesture::Draw { start, id: created, parent, targets: targets.clone() };
        Ok(())
    }

    fn up(&mut self, ev: &PointerEvent) -> Result<()> {
        let g = std::mem::take(&mut self.tool.gesture);
        self.tool.overlay.marquee = None;
        self.tool.overlay.guides.clear();
        self.tool.overlay.insert_line = None;
        let drop_target = self.tool.overlay.drop_target.take();
        match g {
            Gesture::Move { ids, moved, duplicate, .. } => {
                if moved {
                    // Re-parent into the frame under the pointer (or out to the page).
                    let page = self.page()?;
                    let sel = self.selection();
                    let moving = if duplicate { sel } else { ids };
                    let doc = self.doc()?.clone();
                    let in_al =
                        moving.iter().any(|m| doc.node(*m).and_then(|n| n.parent).and_then(|p| doc.node(p)).is_some_and(|p| p.layout.is_some()));
                    if !in_al {
                        let target = drop_target.unwrap_or(page);
                        let current = moving.first().and_then(|m| doc.node(*m)).and_then(|n| n.parent);
                        let target_is_al = doc.node(target).is_some_and(|n| n.layout.is_some());
                        if current != Some(target) {
                            let ds = self.ds_mut()?;
                            let d = ds.doc_mut();
                            for m in &moving {
                                if d.reparent(*m, target, None).is_err() {
                                    continue;
                                }
                            }
                            if target_is_al {
                                // Dropped into auto layout: place by pointer position.
                                let before = ds.doc.clone();
                                crate::normalize(ds.doc_mut(), &before);
                            }
                            let before = self.ds()?.doc.clone();
                            let ds = self.ds_mut()?;
                            crate::normalize(ds.doc_mut(), &before);
                        }
                    }
                    self.commit_interaction()?;
                } else {
                    self.cancel_interaction()?;
                    // Plain click on an already-selected layer among several: select just it.
                    let _ = ev;
                }
            }
            Gesture::Resize { .. } | Gesture::Rotate { .. } => self.commit_interaction()?,
            Gesture::Draw { start, id, parent, .. } => {
                let tiny = (ev.p() - start).hypot() * ev.zoom < 3.0;
                if tiny || id.is_none() {
                    // Click without drag: Figma makes a 100×100 shape (frames 100×100 too).
                    self.cancel_interaction()?;
                    self.begin_interaction(self.tool.tool.label())?;
                    let tool = self.tool.tool;
                    let (w, h) = match tool {
                        ToolId::Line | ToolId::Arrow => (100.0, 0.0),
                        ToolId::Section => (400.0, 300.0),
                        _ => (100.0, 100.0),
                    };
                    let target = start + Vec2::new(w, h);
                    let mut fake = ev.clone();
                    fake.x = target.x;
                    fake.y = target.y;
                    fake.shift = false;
                    fake.alt = false;
                    self.tool.gesture = Gesture::None;
                    let targets = Targets::default();
                    self.draw_drag(&fake, start, None, parent, &targets)?;
                    self.tool.gesture = Gesture::None;
                }
                self.commit_interaction()?;
                if !self.prefs.keep_tool {
                    self.set_tool(ToolId::Move);
                }
            }
            Gesture::Pan { .. } => {
                self.tool.overlay.cursor = Cursor::Grab;
                if let Some(t) = self.tool.temp_from.take() {
                    self.tool.tool = t;
                }
            }
            Gesture::Pencil { points } => {
                self.tool.overlay.pen.clear();
                self.pencil_finish(&points)?;
            }
            Gesture::TextSelect { .. } | Gesture::Marquee { .. } | Gesture::None => {}
            Gesture::PenDrag { .. } => {}
            Gesture::Vector(_) => self.commit_interaction()?,
        }
        if let Ok(ds) = self.ds_mut() {
            ds.revision += 1;
        }
        Ok(())
    }

    fn double_click(&mut self, ev: &PointerEvent) -> Result<()> {
        let ds = self.ds()?;
        let (doc, sel, page) = (ds.doc.clone(), ds.selection.clone(), ds.page);
        if self.tool.tool == ToolId::Pen {
            return self.pen_finish();
        }
        let Some(leaf) = doc.hit_test(page, ev.p(), ev.tol() * 2.0) else { return Ok(()) };
        let target = select_target(&doc, leaf, &sel, true);
        // Text: edit at the click.
        if doc.node(target).is_some_and(|n| matches!(n.kind, NodeKind::Text(_)))
            && (sel.contains(&target) || select_target(&doc, leaf, &sel, false) == target)
        {
            let idx = self.text_index_at(target, ev.p())?;
            textedit::begin(self, target, Some(idx), false)?;
            textedit::select_word(self, idx)?;
            return Ok(());
        }
        // Shapes and vectors: enter vector edit mode.
        if doc.node(target).is_some_and(|n| n.kind.is_shape()) && sel.contains(&target) {
            return crate::vectoredit::begin(self, target);
        }
        // Groups/frames: select one level deeper (the child under the pointer).
        let deeper =
            select_target(&doc, leaf, &[select_target(&doc, leaf, &sel, false)].into_iter().chain(sel.iter().copied()).collect::<Vec<_>>(), false);
        if deeper != target || !sel.contains(&deeper) {
            self.set_selection(vec![deeper]);
        }
        Ok(())
    }

    /// The char index in text node `id` at page point `p`.
    pub fn text_index_at(&self, id: NodeId, p: Point) -> Result<usize> {
        let doc = self.doc()?;
        let n = doc.get(id)?;
        let local = doc.world_transform(id).inverse() * p;
        let Some((layout, dy)) = figcraft_text::layout_node(n) else { return Ok(0) };
        Ok(layout.hit(local.x, local.y - dy))
    }

    fn text_click(&mut self, id: NodeId, ev: &PointerEvent, extend: bool) -> Result<()> {
        let idx = self.text_index_at(id, ev.p())?;
        textedit::set_caret(self, idx, extend)?;
        self.tool.gesture = Gesture::TextSelect { id };
        Ok(())
    }

    fn text_down(&mut self, ev: &PointerEvent) -> Result<()> {
        let ds = self.ds()?;
        let (doc, page) = (ds.doc.clone(), ds.page);
        if let Some(te) = ds.text_edit.clone() {
            if doc.world_bounds(te.id).is_some_and(|b| b.contains(ev.p())) {
                return self.text_click(te.id, ev, ev.shift);
            }
            textedit::end(self)?;
        }
        // Clicking existing text edits it.
        if let Some(leaf) = doc.hit_test(page, ev.p(), ev.tol() * 2.0)
            && doc.node(leaf).is_some_and(|n| matches!(n.kind, NodeKind::Text(_)))
        {
            let idx = self.text_index_at(leaf, ev.p())?;
            textedit::begin(self, leaf, Some(idx), false)?;
            self.tool.gesture = Gesture::TextSelect { id: leaf };
            return Ok(());
        }
        // New text at the click (Figma puts the baseline-ish top-left there).
        let parent = frame_at(&doc, page, ev.p(), &[]).unwrap_or(page);
        let ds = self.ds_mut()?;
        let d = ds.doc_mut();
        let id = d.alloc_id();
        let mut n = Node::new(id, NodeKind::Text(TextData::default()), "Text", 1.0, 14.0);
        let pw = d.world_transform(parent);
        let at = Point::new(ev.x.round(), (ev.y - 7.0).round());
        n.transform = pw.inverse() * Affine::translate(at.to_vec2());
        n.name = String::new();
        if let Some((w, h)) = figcraft_text::auto_size(&n) {
            n.width = w.max(1.0);
            n.height = h;
        }
        d.insert(n, parent, None)?;
        ds.selection = vec![id];
        textedit::begin(self, id, Some(0), true)?;
        if !self.prefs.keep_tool {
            self.tool.tool = ToolId::Move;
        }
        Ok(())
    }

    fn eyedrop(&mut self, ev: &PointerEvent) -> Result<()> {
        let ds = self.ds()?;
        let doc = ds.doc.clone();
        let view = figcraft_render::View { origin: (ev.x, ev.y), scale: ev.zoom.max(0.01), width: 1, height: 1 };
        let Some(pix) = figcraft_render::render_view(&doc, ds.page, &view, &Default::default()) else { return Ok(()) };
        let rgba = figcraft_render::to_rgba8(&pix);
        let c = figcraft_color::Color::from_rgba8(
            rgba.first().copied().unwrap_or(0),
            rgba.get(1).copied().unwrap_or(0),
            rgba.get(2).copied().unwrap_or(0),
            255,
        );
        let sel = ds.selection.clone();
        if !sel.is_empty() {
            self.execute("node.set", &json!({"ids": sel, "props": {"fill": c.to_css_hex()}}))?;
        }
        self.recent_colors.retain(|x| *x != c);
        self.recent_colors.insert(0, c);
        self.recent_colors.truncate(16);
        self.message = Some(format!("Picked #{}", c.to_hex()));
        self.set_tool(ToolId::Move);
        Ok(())
    }

    fn pen_down(&mut self, ev: &PointerEvent) -> Result<()> {
        let p = ev.p();
        let (doc, page) = {
            let ds = self.ds()?;
            (ds.doc.clone(), ds.page)
        };
        let pen = self.tool.pen.get_or_insert_with(PenPath::default);
        // Clicking the first anchor closes the path.
        if pen.points.len() > 2
            && let Some((first, _, _)) = pen.points.first()
            && (*first - p).hypot() * ev.zoom < 8.0
        {
            pen.closed = true;
            self.pen_update()?;
            return self.pen_finish();
        }
        if pen.parent.is_none() {
            pen.parent = Some(frame_at(&doc, page, p, &[]).unwrap_or(page));
        }
        let mut p = p;
        if ev.shift
            && let Some((last, _, _)) = pen.points.last()
        {
            let v = p - *last;
            let ang = (v.atan2() / (std::f64::consts::PI / 4.0)).round() * std::f64::consts::PI / 4.0;
            p = *last + Vec2::from_angle(ang) * v.hypot();
        }
        pen.points.push((p, Vec2::ZERO, Vec2::ZERO));
        self.tool.gesture = Gesture::PenDrag { anchor: p };
        self.pen_update()
    }

    /// Rebuild the pen's vector node from its anchors.
    fn pen_update(&mut self) -> Result<()> {
        let Some(pen) = self.tool.pen.clone() else { return Ok(()) };
        self.tool.overlay.pen = pen.points.iter().map(|(p, _, _)| [p.x, p.y]).collect();
        if pen.points.len() < 2 {
            if let Ok(ds) = self.ds_mut() {
                ds.revision += 1;
            }
            return Ok(());
        }
        let page = self.page()?;
        let parent = pen.parent.unwrap_or(page);
        if pen.id.is_none() {
            self.begin_interaction("Pen")?;
        }
        let ds = self.ds_mut()?;
        let doc = ds.doc_mut();
        let pw = doc.world_transform(parent);
        let inv = pw.inverse();
        // Anchors and handles into the parent's space.
        let pts: Vec<(Point, Point, Point)> = pen.points.iter().map(|(p, i, o)| (inv * *p, inv * (*p + *i), inv * (*p + *o))).collect();
        let mut path = figcraft_geom::BezPath::new();
        for (k, (p, _, _)) in pts.iter().enumerate() {
            if k == 0 {
                path.move_to(*p);
            } else if let (Some(prev), Some(cur)) = (pts.get(k - 1), pts.get(k)) {
                path.curve_to(prev.2, cur.1, *p);
            }
        }
        if pen.closed
            && let (Some(last), Some(first)) = (pts.last(), pts.first())
        {
            path.curve_to(last.2, first.1, first.0);
            path.close_path();
        }
        use figcraft_geom::Shape;
        let bb = path.bounding_box();
        path.apply_affine(Affine::translate((-bb.x0, -bb.y0)));
        let mut net = VectorNetwork::from_path(&path);
        // Straight segments stay straight.
        for s in &mut net.segments {
            if (s.ta.0.abs() + s.ta.1.abs()) < 1e-9 && (s.tb.0.abs() + s.tb.1.abs()) < 1e-9 {
                s.ta = (0.0, 0.0);
                s.tb = (0.0, 0.0);
            }
        }
        if !pen.closed {
            net.regions.clear();
        }
        let id = match pen.id {
            Some(id) if doc.contains(id) => id,
            _ => {
                let id = doc.alloc_id();
                let mut n = Node::new(id, NodeKind::Vector { network: VectorNetwork::default() }, "Vector", 0.0, 0.0);
                n.fills.clear();
                n.strokes = vec![figcraft_color::Paint::solid(figcraft_color::Color::BLACK)];
                n.stroke.align = StrokeAlign::Center;
                doc.insert(n, parent, None)?;
                id
            }
        };
        let n = doc.get_mut(id)?;
        n.kind = NodeKind::Vector { network: net };
        n.width = bb.width();
        n.height = bb.height();
        n.transform = Affine::translate((bb.x0, bb.y0));
        if pen.closed && n.fills.is_empty() {
            n.fills = vec![figcraft_color::Paint::solid(figcraft_color::Color::DEFAULT_SHAPE)];
        }
        ds.selection = vec![id];
        ds.revision += 1;
        if let Some(p) = self.tool.pen.as_mut() {
            p.id = Some(id);
        }
        Ok(())
    }

    /// Finish the pen path (Enter/Escape/double-click).
    pub fn pen_finish(&mut self) -> Result<()> {
        let pen = self.tool.pen.take();
        self.tool.overlay.pen.clear();
        self.tool.overlay.rubber = None;
        self.tool.gesture = Gesture::None;
        if pen.as_ref().is_some_and(|p| p.id.is_some()) {
            self.commit_interaction()?;
        }
        Ok(())
    }

    fn pencil_finish(&mut self, points: &[Point]) -> Result<()> {
        if points.len() < 2 {
            return Ok(());
        }
        // Smooth: fit a curve through the samples.
        let mut poly = figcraft_geom::BezPath::new();
        for (k, p) in points.iter().enumerate() {
            if k == 0 {
                poly.move_to(*p);
            } else {
                poly.line_to(*p);
            }
        }
        let fitted = figcraft_geom::kurbo::simplify::simplify_bezpath(poly.iter(), 0.75, &Default::default());
        let d = fitted.to_svg();
        let page = self.page()?;
        let parent = frame_at(self.doc()?, page, points.first().copied().unwrap_or_default(), &[]).unwrap_or(page);
        let pw = self.doc()?.world_transform(parent);
        let mut local = fitted.clone();
        local.apply_affine(pw.inverse());
        let _ = d;
        self.execute(
            "node.create",
            &json!({"type": "vector", "path": local.to_svg(), "closed": false, "parent": parent, "name": "Vector",
                    "props": {"fills": [], "strokes": ["#000000"], "strokeWeight": 2, "strokeCap": "round", "strokeJoin": "round", "strokeAlign": "center"}}),
        )?;
        Ok(())
    }

    /// Keyboard Escape: cancel gestures, finish pens, leave text editing, then deselect.
    pub fn escape(&mut self) -> Result<()> {
        if self.tool.pen.is_some() {
            return self.pen_finish();
        }
        if self.tool.busy() {
            self.tool.gesture = Gesture::None;
            self.tool.overlay = Overlay::default();
            return self.cancel_interaction();
        }
        if self.ds()?.text_edit.is_some() {
            return textedit::end(self);
        }
        if self.ds()?.vector_edit.is_some() {
            return crate::vectoredit::escape(self);
        }
        if self.tool.tool != ToolId::Move {
            self.set_tool(ToolId::Move);
            return Ok(());
        }
        // Escape selects the parent (like Figma), then nothing.
        let doc = self.doc()?;
        let sel = self.selection();
        let parents: Vec<NodeId> =
            sel.iter().filter_map(|i| doc.node(*i).and_then(|n| n.parent)).filter(|p| !doc.node(*p).is_some_and(Node::is_page)).collect();
        if !parents.is_empty() {
            let mut uniq = Vec::new();
            for p in parents {
                if !uniq.contains(&p) {
                    uniq.push(p);
                }
            }
            self.set_selection(uniq);
        } else {
            self.set_selection(vec![]);
        }
        Ok(())
    }

    /// Snapshot of tool state for the UI / inspect.
    pub fn tool_info(&self) -> serde_json::Value {
        json!({"tool": self.tool.tool.id(), "busy": self.tool.busy(), "overlay": self.tool.overlay})
    }
}

#[allow(dead_code)]
fn unused(_: BTreeMap<u8, u8>) {}
