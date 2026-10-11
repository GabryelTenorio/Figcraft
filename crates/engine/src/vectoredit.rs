//! Vector edit mode: editing a vector network's points, segments and bezier handles on the canvas,
//! with its own tools (move, pen, bend, paint bucket, lasso, cut).
//!
//! Entering edit mode on a non-vector shape converts it to a vector first (like the incumbent).
//! Vertex coordinates are node-local; after each edit the node is re-fitted so its box wraps the
//! network again. The network edits themselves live in [`ops`]; pointer gestures in `pointer.rs`,
//! the transform box around several points in [`transform`] and point snapping in `point_snap.rs`.

pub mod ops;
pub(crate) mod point_snap;
mod pointer;
#[cfg(test)]
mod tests;
pub mod transform;
#[cfg(test)]
mod transform_tests;
mod width;
#[cfg(test)]
mod width_sides_tests;
#[cfg(test)]
mod width_tests;

use figcraft_doc::*;
use figcraft_geom::{Affine, Point, Rect, Vec2};
use serde::Serialize;
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::*;
use crate::{EngineError, Result, Session, bad, spec};

pub(crate) use pointer::VGesture;
pub use width::{WHit, WidthHandle, width_handles, width_hit, width_preview};

/// The edit-mode tools (the toolbar shown while editing a vector).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
pub enum VTool {
    #[default]
    Move,
    Pen,
    Bend,
    Paint,
    Lasso,
    Cut,
    Eraser,
    /// Variable width: width points along the stroke.
    Width,
}

impl VTool {
    pub const ALL: [VTool; 8] = [VTool::Move, VTool::Pen, VTool::Bend, VTool::Paint, VTool::Lasso, VTool::Cut, VTool::Eraser, VTool::Width];
    pub fn id(self) -> &'static str {
        match self {
            VTool::Move => "move",
            VTool::Pen => "pen",
            VTool::Bend => "bend",
            VTool::Paint => "paint",
            VTool::Lasso => "lasso",
            VTool::Cut => "cut",
            VTool::Eraser => "eraser",
            VTool::Width => "width",
        }
    }
    pub fn from_id(s: &str) -> Option<VTool> {
        VTool::ALL.into_iter().find(|t| t.id().eq_ignore_ascii_case(s))
    }
    pub fn label(self) -> &'static str {
        match self {
            VTool::Move => "Move",
            VTool::Pen => "Pen",
            VTool::Bend => "Bend",
            VTool::Paint => "Paint bucket",
            VTool::Lasso => "Lasso",
            VTool::Cut => "Cut",
            VTool::Eraser => "Eraser",
            VTool::Width => "Variable width",
        }
    }
    /// Keymap-style shortcut ("Shift+E"); the Bend tool is held with Cmd (Ctrl elsewhere).
    pub fn shortcut(self) -> &'static str {
        match self {
            VTool::Move => "V",
            VTool::Pen => "P",
            VTool::Bend => "Cmd",
            VTool::Paint => "B",
            VTool::Lasso => "Q",
            VTool::Cut => "X",
            VTool::Eraser => "Shift+E",
            VTool::Width => "Shift+W",
        }
    }
}

/// Vector edit mode state: the vector being edited, its selected points and segments, the active
/// edit tool and the pen's open end.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorEdit {
    pub id: NodeId,
    pub vertices: Vec<usize>,
    pub segments: Vec<usize>,
    pub tool: VTool,
    /// Pen: the point the next click continues from.
    pub anchor: Option<usize>,
    /// Pen: the outgoing handle dragged out of `anchor`, used by the next segment.
    pub pen_out: Option<(f64, f64)>,
    /// Variable width tool: the selected width point (an index into the stroke's profile).
    pub width_point: Option<usize>,
}

impl VectorEdit {
    pub fn new(id: NodeId) -> Self {
        Self { id, vertices: vec![], segments: vec![], tool: VTool::Move, anchor: None, pen_out: None, width_point: None }
    }

    /// Drop indices that no longer exist (after undo or an edit).
    pub fn clamp(&mut self, net: &VectorNetwork) {
        let (nv, ns) = (net.vertices.len(), net.segments.len());
        self.vertices.retain(|v| *v < nv);
        self.segments.retain(|s| *s < ns);
        if self.anchor.is_some_and(|a| a >= nv) {
            self.anchor = None;
            self.pen_out = None;
        }
    }
}

/// What a pointer is over in edit mode.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum VHit {
    Vertex {
        index: usize,
    },
    /// Handle of segment `seg`; `start` = the handle at the segment's `a` end.
    Handle {
        seg: usize,
        start: bool,
    },
    Segment {
        seg: usize,
        t: f64,
    },
}

/// A visible handle in page coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct HandleLine {
    pub seg: usize,
    pub start: bool,
    pub vertex: [f64; 2],
    pub handle: [f64; 2],
}

/// Page-space geometry for drawing the edit overlay.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOverlay {
    /// Cubic control points per segment.
    pub segments: Vec<[[f64; 2]; 4]>,
    pub vertices: Vec<[f64; 2]>,
    pub handles: Vec<HandleLine>,
    pub selected_vertices: Vec<usize>,
    pub selected_segments: Vec<usize>,
    pub anchor: Option<usize>,
}

/// Convert a shape to an editable vector (keeps paints, transform and look).
pub fn ensure_vector(doc: &mut Document, id: NodeId) -> Result<()> {
    let n = doc.get(id)?.clone();
    if matches!(n.kind, NodeKind::Vector { .. }) {
        return Ok(());
    }
    if !n.kind.is_shape() && !matches!(n.kind, NodeKind::BooleanOp { .. }) {
        return Err(EngineError::Other(format!("{} layers can't be edited as vectors", n.kind.label())));
    }
    let (path, rule) = figcraft_render::node_outline(doc, &n).ok_or_else(|| EngineError::Other("no outline".into()))?;
    let mut net = VectorNetwork::from_path(&path);
    if matches!(n.kind, NodeKind::Line) {
        net.regions.clear();
    }
    if rule == figcraft_render::FillRule::EvenOdd {
        for r in &mut net.regions {
            r.winding = WindingRule::EvenOdd;
        }
    }
    let node = doc.get_mut(id)?;
    node.kind = NodeKind::Vector { network: net };
    node.children.clear();
    if matches!(n.kind, NodeKind::Line) {
        node.stroke.align = StrokeAlign::Center;
    }
    node.corners = Corners::default();
    refit(doc, id)?;
    Ok(())
}

/// Make the node's box wrap its network again (moving the node so nothing shifts on screen).
pub fn refit(doc: &mut Document, id: NodeId) -> Result<()> {
    let n = doc.get_mut(id)?;
    let NodeKind::Vector { network } = &mut n.kind else { return Ok(()) };
    let Some(b) = network.bounds() else { return Ok(()) };
    if !(b.x0.is_finite() && b.y0.is_finite() && b.width().is_finite() && b.height().is_finite()) {
        return Ok(());
    }
    if b.x0.abs() > 1e-9 || b.y0.abs() > 1e-9 {
        network.translate(-b.x0, -b.y0);
        n.transform *= Affine::translate((b.x0, b.y0));
    }
    n.width = b.width();
    n.height = b.height();
    Ok(())
}

/// The network of vector `id`.
pub fn network(doc: &Document, id: NodeId) -> Option<&VectorNetwork> {
    match &doc.node(id)?.kind {
        NodeKind::Vector { network } => Some(network),
        _ => None,
    }
}

fn network_mut(doc: &mut Document, id: NodeId) -> Result<&mut VectorNetwork> {
    match doc.node_mut(id).map(|n| &mut n.kind) {
        Some(NodeKind::Vector { network }) => Ok(network),
        _ => Err(EngineError::Other("not a vector".into())),
    }
}

/// Edit the network of `id`, then re-fit the node.
pub fn edit<T>(doc: &mut Document, id: NodeId, f: impl FnOnce(&mut VectorNetwork) -> T) -> Result<T> {
    let out = f(network_mut(doc, id)?);
    refit(doc, id)?;
    Ok(out)
}

/// Handles drawn and hit-tested: at selected points, and at both ends of selected segments.
fn visible_handles(net: &VectorNetwork, ve: &VectorEdit) -> Vec<(usize, bool)> {
    let mut out = Vec::new();
    for (si, s) in net.segments.iter().enumerate() {
        let seg_on = ve.segments.contains(&si);
        for (start, vi, t) in [(true, s.a, s.ta), (false, s.b, s.tb)] {
            if t != (0.0, 0.0) && (seg_on || ve.vertices.contains(&vi)) {
                out.push((si, start));
            }
        }
    }
    out
}

/// Hit test in page coordinates with a tolerance in page units. Visible handles win, then
/// vertices, then segments.
pub fn hit(doc: &Document, ve: &VectorEdit, p: Point, tol: f64) -> Option<VHit> {
    let net = network(doc, ve.id)?;
    let w = doc.world_transform(ve.id);
    let lp = w.inverse() * p;
    // Tolerance in local units (approximate for non-uniform scale).
    let s = (w.determinant().abs()).sqrt().max(1e-9);
    let tl = tol / s;
    if !(lp.x.is_finite() && lp.y.is_finite()) {
        return None;
    }
    for (si, start) in visible_handles(net, ve) {
        let (Some(seg), Some(t)) = (net.segments.get(si), net.handle(si, start)) else { continue };
        let Some(v) = net.vertices.get(if start { seg.a } else { seg.b }) else { continue };
        if (Point::new(v.x + t.0, v.y + t.1) - lp).hypot() <= tl {
            return Some(VHit::Handle { seg: si, start });
        }
    }
    let nearest_vertex =
        net.vertices.iter().enumerate().map(|(i, v)| (i, (v.point() - lp).hypot())).filter(|(_, d)| *d <= tl).min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((index, _)) = nearest_vertex {
        return Some(VHit::Vertex { index });
    }
    use figcraft_geom::kurbo::ParamCurveNearest;
    let nearest_seg = (0..net.segments.len())
        .filter_map(|si| {
            let near = net.cubic(si)?.nearest(lp, 0.01);
            Some((si, near.t, near.distance_sq.sqrt()))
        })
        .filter(|(_, _, d)| *d <= tl)
        .min_by(|a, b| a.2.total_cmp(&b.2));
    nearest_seg.map(|(seg, t, _)| VHit::Segment { seg, t })
}

/// Vertices inside a page-space rectangle.
pub fn vertices_in(doc: &Document, id: NodeId, r: Rect) -> Vec<usize> {
    let Some(net) = network(doc, id) else { return vec![] };
    let w = doc.world_transform(id);
    net.vertices.iter().enumerate().filter(|(_, v)| r.contains(w * v.point())).map(|(i, _)| i).collect()
}

/// Vertices inside a page-space lasso polygon.
pub fn vertices_in_polygon(doc: &Document, id: NodeId, poly: &[Point]) -> Vec<usize> {
    let Some(net) = network(doc, id) else { return vec![] };
    let w = doc.world_transform(id);
    let pts: Vec<Point> = net.vertices.iter().map(|v| w * v.point()).collect();
    ops::in_polygon(&pts, poly)
}

/// Page-space geometry of the network for drawing the edit overlay.
pub fn overlay(doc: &Document, ve: &VectorEdit) -> Option<EditOverlay> {
    let net = network(doc, ve.id)?;
    let w = doc.world_transform(ve.id);
    let p = |q: Point| {
        let q = w * q;
        [q.x, q.y]
    };
    let segments = (0..net.segments.len()).filter_map(|i| net.cubic(i)).map(|c| [p(c.p0), p(c.p1), p(c.p2), p(c.p3)]).collect();
    let vertices = net.vertices.iter().map(|v| p(v.point())).collect();
    let handles = visible_handles(net, ve)
        .into_iter()
        .filter_map(|(si, start)| {
            let s = net.segments.get(si)?;
            let v = net.vertices.get(if start { s.a } else { s.b })?;
            let t = net.handle(si, start)?;
            Some(HandleLine { seg: si, start, vertex: p(v.point()), handle: p(Point::new(v.x + t.0, v.y + t.1)) })
        })
        .collect();
    Some(EditOverlay {
        segments,
        vertices,
        handles,
        selected_vertices: ve.vertices.clone(),
        selected_segments: ve.segments.clone(),
        anchor: ve.anchor,
    })
}

/// Start editing `id` as a vector.
pub fn begin(s: &mut Session, id: NodeId) -> Result<()> {
    let before = s.ds()?.doc.clone();
    {
        let doc = s.doc_mut()?;
        ensure_vector(doc, id)?;
    }
    let ds = s.ds_mut()?;
    let edited = (!std::sync::Arc::ptr_eq(&before, &ds.doc)).then(|| (ds.uid, ds.selection.clone(), ds.page));
    ds.vector_edit = Some(VectorEdit::new(id));
    ds.selection = vec![id];
    ds.text_edit = None;
    ds.revision += 1;
    if let Some((uid, selection, page)) = edited {
        s.record(uid, Some("Edit vector"), before, selection, page);
    }
    Ok(())
}

pub fn end(s: &mut Session) -> Result<()> {
    let ds = s.ds_mut()?;
    ds.vector_edit = None;
    ds.revision += 1;
    s.tool.overlay = Default::default();
    Ok(())
}

pub fn editing(s: &Session) -> Result<VectorEdit> {
    s.ds()?.vector_edit.clone().ok_or_else(|| EngineError::Disabled("vector".into(), "not in vector edit mode".into()))
}

/// Replace the edit state (selection, tool, pen end) and repaint.
pub(crate) fn set_state(s: &mut Session, f: impl FnOnce(&mut VectorEdit)) -> Result<()> {
    let ds = s.ds_mut()?;
    if let Some(ve) = ds.vector_edit.as_mut() {
        f(ve);
    }
    ds.revision += 1;
    Ok(())
}

pub fn set_tool(s: &mut Session, tool: VTool) -> Result<()> {
    s.tool.overlay = Default::default();
    set_state(s, |ve| {
        ve.tool = tool;
        if tool != VTool::Pen {
            ve.anchor = None;
            ve.pen_out = None;
        }
    })
}

/// Escape: end the pen's path first, then leave edit mode.
pub fn escape(s: &mut Session) -> Result<()> {
    let ve = editing(s)?;
    if ve.anchor.is_some() {
        s.tool.overlay.rubber = None;
        return set_state(s, |ve| {
            ve.anchor = None;
            ve.pen_out = None;
        });
    }
    end(s)
}

/// Parse `[i, ...]` from params.
fn indices(p: &Value, key: &str) -> Option<Vec<usize>> {
    match get(p, key) {
        Some(Value::Array(a)) => Some(a.iter().filter_map(f64_of).filter(|x| *x >= 0.0 && *x < 1e9).map(|x| x as usize).collect()),
        _ => None,
    }
}

fn index(cmd: &str, p: &Value, key: &str) -> Result<usize> {
    let v = req_num(cmd, p, key)?;
    if !(0.0..1e9).contains(&v) {
        return Err(bad(cmd, format!("`{key}` out of range")));
    }
    Ok(v as usize)
}

/// Delete the selected points and segments (`heal` keeps paths through deleted points closed).
fn delete_selection(s: &mut Session, verts: Vec<usize>, segs: Vec<usize>, heal: bool) -> Result<()> {
    let id = editing(s)?.id;
    edit(s.doc_mut()?, id, |net| {
        // Segments first (their indices refer to the network before points go).
        let segs: Vec<usize> =
            segs.into_iter().filter(|si| net.segments.get(*si).is_some_and(|x| !verts.contains(&x.a) && !verts.contains(&x.b))).collect();
        let pts: Vec<Point> = verts.iter().filter_map(|v| net.vertices.get(*v)).map(|v| v.point()).collect();
        ops::remove_segments(net, &segs);
        // Point indices may have shifted: find them again by position.
        let now: Vec<usize> = pts.iter().filter_map(|p| net.vertices.iter().position(|v| v.point() == *p)).collect();
        ops::delete_vertices(net, &now, heal);
    })?;
    set_state(s, |ve| {
        ve.vertices.clear();
        ve.segments.clear();
        ve.anchor = None;
        ve.pen_out = None;
    })
}

/// Selection bounds' top-left in the parent's coordinates (what the X/Y fields show).
pub fn selection_position(doc: &Document, ve: &VectorEdit) -> Option<Point> {
    let net = network(doc, ve.id)?;
    let t = doc.node(ve.id)?.transform;
    let pts: Vec<Point> = selected_points(net, ve).into_iter().filter_map(|v| net.vertices.get(v)).map(|v| t * v.point()).collect();
    let x = pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let y = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    (x.is_finite() && y.is_finite()).then(|| Point::new(x, y))
}

fn default_points(s: &Session, ve: &VectorEdit) -> Vec<usize> {
    s.doc().ok().and_then(|d| network(d, ve.id)).map(|n| selected_points(n, ve)).unwrap_or_default()
}

/// Selected points plus the ends of selected segments.
pub fn selected_points(net: &VectorNetwork, ve: &VectorEdit) -> Vec<usize> {
    let mut v = ve.vertices.clone();
    for s in ve.segments.iter().filter_map(|i| net.segments.get(*i)) {
        v.extend([s.a, s.b]);
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// Keyboard in edit mode. Returns whether the key was used (others fall through to shortcuts).
pub fn key(s: &mut Session, name: &str, shift: bool, alt: bool, cmd: bool) -> Result<bool> {
    let Ok(ve) = editing(s) else { return Ok(false) };
    let nudge = |big: bool, s: &Session| if big { s.prefs.nudge_big } else { s.prefs.nudge_small };
    let arrow = match name {
        "Left" | "ArrowLeft" => Some((-1.0, 0.0)),
        "Right" | "ArrowRight" => Some((1.0, 0.0)),
        "Up" | "ArrowUp" => Some((0.0, -1.0)),
        "Down" | "ArrowDown" => Some((0.0, 1.0)),
        _ => None,
    };
    if let Some((dx, dy)) = arrow {
        if cmd || alt {
            return Ok(false);
        }
        let n = nudge(shift, s);
        s.execute("vector.edit.nudge", &json!({"dx": dx * n, "dy": dy * n}))?;
        return Ok(true);
    }
    match (name, shift, alt, cmd) {
        ("Escape", ..) => escape(s)?,
        ("Enter", ..) => {
            if ve.anchor.is_some() {
                escape(s)?;
            } else {
                end(s)?;
            }
        }
        ("Backspace" | "Delete", false, false, false) if ve.tool == VTool::Width => {
            // The selected width point (nothing else is selected with the Variable width tool).
            if let Some(i) = ve.width_point {
                s.execute("vector.edit.widthPoint", &json!({"action": "remove", "index": i}))?;
            }
        }
        ("Backspace" | "Delete", heal, false, false) => {
            s.execute(if heal { "vector.deleteHeal" } else { "vector.edit.delete" }, &json!({}))?;
        }
        ("A", false, false, true) => {
            s.execute("vector.edit.selectAll", &json!({}))?;
        }
        ("J", smooth, false, true) => {
            s.execute(if smooth { "vector.smoothJoin" } else { "vector.join" }, &json!({}))?;
        }
        ("V", false, false, false) => set_tool(s, VTool::Move)?,
        ("P", false, false, false) => set_tool(s, VTool::Pen)?,
        // B (Shift+B still works, as it did before).
        ("B", _, false, false) => set_tool(s, VTool::Paint)?,
        ("Q", false, false, false) => set_tool(s, VTool::Lasso)?,
        ("X", false, false, false) => set_tool(s, VTool::Cut)?,
        ("E", true, false, false) => set_tool(s, VTool::Eraser)?,
        ("W", true, false, false) => set_tool(s, VTool::Width)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn join_cmd(s: &mut Session, smooth: bool) -> Result<Value> {
    let ve = editing(s)?;
    let doc = s.doc()?;
    let net = network(doc, ve.id).ok_or_else(|| bad("vector.join", "not a vector"))?;
    let verts = selected_points(net, &ve);
    let joins = edit(s.doc_mut()?, ve.id, |net| ops::join(net, &verts, smooth))?;
    set_state(s, |ve| ve.segments.clear())?;
    Ok(json!({"joins": joins}))
}

/// `vector.join` / `vector.smoothJoin` while editing: join the selected points.
pub fn join_in_edit_mode(s: &mut Session, smooth: bool) -> Option<Result<Value>> {
    editing(s).ok()?;
    Some(join_cmd(s, smooth))
}

/// Page point → the edited node's local coordinates.
fn local(s: &Session, id: NodeId, p: &Value, cmd: &str) -> Result<Point> {
    let pt = Point::new(req_num(cmd, p, "x")?, req_num(cmd, p, "y")?);
    Ok(s.doc()?.world_transform(id).inverse() * pt)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("vector.edit.begin", "Edit vector", "{id?} (converts shapes to vectors)", false, |s, p| {
            let id = targets(s, p).first().copied().ok_or_else(|| bad("vector.edit.begin", "select a shape"))?;
            begin(s, id)?;
            Ok(json!({"id": id}))
        }),
        spec!("vector.edit.end", "Done editing", "{}", false, |s, _| {
            end(s)?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.state", "Vector edit state", "{} → page-space overlay plus the local network", false, |s, _| {
            let ve = editing(s)?;
            let doc = s.doc()?;
            let o = overlay(doc, &ve).map(|o| serde_json::to_value(o).unwrap_or(Value::Null)).unwrap_or(Value::Null);
            let net = network(doc, ve.id).map(|n| serde_json::to_value(n).unwrap_or(Value::Null)).unwrap_or(Value::Null);
            let mirroring: Vec<&str> = network(doc, ve.id).map(|n| (0..n.vertices.len()).map(|v| n.mirroring(v).id()).collect()).unwrap_or_default();
            Ok(json!({"id": ve.id, "tool": ve.tool.id(), "overlay": o, "network": net, "mirroring": mirroring,
                "widthPoints": width_handles(doc, &ve, 1.0), "widthPoint": ve.width_point}))
        }),
        spec!("vector.edit.tool", "Vector edit tool", "{tool: move|pen|bend|paint|lasso|cut|eraser|width}", false, |s, p| {
            editing(s)?;
            let t = string(p, "tool").and_then(|t| VTool::from_id(&t)).ok_or_else(|| bad("vector.edit.tool", "unknown tool"))?;
            set_tool(s, t)?;
            Ok(json!({"tool": t.id()}))
        }),
        spec!("vector.edit.select", "Select points", "{vertices?: [i, ...], segments?: [i, ...], add?}", false, |s, p| {
            editing(s)?;
            let (v, sg, add) =
                (indices(p, "vertices").unwrap_or_default(), indices(p, "segments").unwrap_or_default(), boolean(p, "add").unwrap_or(false));
            set_state(s, |ve| {
                if !add {
                    ve.vertices.clear();
                    ve.segments.clear();
                }
                ve.vertices.extend(v);
                ve.segments.extend(sg);
            })?;
            let doc = s.doc()?.clone();
            let ds = s.ds_mut()?;
            if let Some(ve) = ds.vector_edit.as_mut()
                && let Some(net) = network(&doc, ve.id)
            {
                ve.vertices.sort_unstable();
                ve.vertices.dedup();
                ve.segments.sort_unstable();
                ve.segments.dedup();
                ve.clamp(net);
            }
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.selectAll", "Select all points", "{}", false, |s, _| {
            let ve = editing(s)?;
            let n = network(s.doc()?, ve.id).map(|n| n.vertices.len()).unwrap_or(0);
            set_state(s, |ve| {
                ve.vertices = (0..n).collect();
                ve.segments.clear();
            })?;
            Ok(json!({"count": n}))
        }),
        spec!("vector.edit.moveVertices", "Move points", "{vertices?, dx, dy} (local units; default the selected points)", true, |s, p| {
            let ve = editing(s)?;
            let v = indices(p, "vertices").unwrap_or_else(|| default_points(s, &ve));
            let d = Vec2::new(num_or(p, "dx", 0.0), num_or(p, "dy", 0.0));
            edit(s.doc_mut()?, ve.id, |net| ops::move_vertices(net, &v, d))?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.nudge", "Nudge points", "{dx, dy} (page units)", true, |s, p| {
            let ve = editing(s)?;
            let doc = s.doc()?;
            let v = network(doc, ve.id).map(|n| selected_points(n, &ve)).unwrap_or_default();
            let inv = doc.world_transform(ve.id).inverse();
            let d = inv * (Point::ZERO + Vec2::new(num_or(p, "dx", 0.0), num_or(p, "dy", 0.0))) - inv * Point::ZERO;
            edit(s.doc_mut()?, ve.id, |net| ops::move_vertices(net, &v, d))?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.position", "Point position", "{x?, y?} (the selected points' top-left, in the parent's coordinates)", true, |s, p| {
            let ve = editing(s)?;
            let doc = s.doc()?;
            let cur = selection_position(doc, &ve).ok_or_else(|| bad("vector.edit.position", "select points"))?;
            let target = Point::new(num_or(p, "x", cur.x), num_or(p, "y", cur.y));
            let t = doc.node(ve.id).map(|n| n.transform).unwrap_or_default();
            let inv = t.inverse();
            let d = inv * target - inv * cur;
            let v = network(doc, ve.id).map(|n| selected_points(n, &ve)).unwrap_or_default();
            edit(s.doc_mut()?, ve.id, |net| ops::move_vertices(net, &v, d))?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.setHandle", "Set handle", "{segment, start (bool), x, y (local), independent?}", true, |s, p| {
            let ve = editing(s)?;
            let seg = index("vector.edit.setHandle", p, "segment")?;
            let start = boolean(p, "start").unwrap_or(true);
            let pt = Point::new(req_num("vector.edit.setHandle", p, "x")?, req_num("vector.edit.setHandle", p, "y")?);
            let independent = boolean(p, "independent").unwrap_or(false);
            edit(s.doc_mut()?, ve.id, |net| {
                let Some(sg) = net.segments.get(seg).copied() else { return };
                let Some(v) = net.vertices.get(if start { sg.a } else { sg.b }).copied() else { return };
                ops::set_handle(net, seg, start, (pt.x - v.x, pt.y - v.y), independent);
            })?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.mirroring", "Handle mirroring", "{mode: none|angle|angleAndLength, vertices?}", true, |s, p| {
            let ve = editing(s)?;
            let mode = string(p, "mode")
                .and_then(|m| HandleMirroring::from_id(&m))
                .ok_or_else(|| bad("vector.edit.mirroring", "mode: none, angle or angleAndLength"))?;
            let v = indices(p, "vertices").unwrap_or_else(|| default_points(s, &ve));
            edit(s.doc_mut()?, ve.id, |net| ops::set_mirroring(net, &v, mode))?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.smooth", "Smooth or sharpen points", "{vertices?, smooth? (default true)}", true, |s, p| {
            let ve = editing(s)?;
            let smooth = boolean(p, "smooth").unwrap_or(true);
            let v = indices(p, "vertices").unwrap_or_else(|| ve.vertices.clone());
            edit(s.doc_mut()?, ve.id, |net| {
                for vi in v {
                    if smooth {
                        ops::auto_smooth(net, vi);
                    } else {
                        ops::make_sharp(net, vi);
                    }
                }
            })?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.bend", "Bend segment", "{segment, t (0..1), x, y (page point the curve passes through)}", true, |s, p| {
            let ve = editing(s)?;
            let seg = index("vector.edit.bend", p, "segment")?;
            let t = num_or(p, "t", 0.5);
            let lp = local(s, ve.id, p, "vector.edit.bend")?;
            edit(s.doc_mut()?, ve.id, |net| ops::bend(net, seg, t, lp))?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.addVertex", "Add point", "{segment, t (0..1)}", true, |s, p| {
            let ve = editing(s)?;
            let seg = index("vector.edit.addVertex", p, "segment")?;
            let t = num_or(p, "t", 0.5);
            let v =
                edit(s.doc_mut()?, ve.id, |net| ops::split_segment(net, seg, t))?.ok_or_else(|| bad("vector.edit.addVertex", "no such segment"))?;
            set_state(s, |ve| {
                ve.vertices = vec![v];
                ve.segments.clear();
            })?;
            Ok(json!({"vertex": v}))
        }),
        spec!("vector.deleteHeal", "Delete and heal selection", "{vertices?}", true, |s, p| {
            let ve = editing(s)?;
            delete_selection(s, indices(p, "vertices").unwrap_or(ve.vertices), vec![], true)?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.delete", "Delete points", "{vertices?, segments?} (default the selection)", true, |s, p| {
            let ve = editing(s)?;
            let (v, sg) = match (indices(p, "vertices"), indices(p, "segments")) {
                (None, None) => (ve.vertices, ve.segments),
                (v, sg) => (v.unwrap_or_default(), sg.unwrap_or_default()),
            };
            delete_selection(s, v, sg, false)?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.cornerRadius", "Point corner radius", "{radius, vertices?}", true, |s, p| {
            let ve = editing(s)?;
            let r = req_num("vector.edit.cornerRadius", p, "radius")?.clamp(0.0, 1e6);
            let v = indices(p, "vertices").unwrap_or_else(|| default_points(s, &ve));
            edit(s.doc_mut()?, ve.id, |net| {
                for i in v {
                    if let Some(x) = net.vertices.get_mut(i) {
                        x.radius = r;
                    }
                }
            })?;
            Ok(json!({"ok": true}))
        }),
        spec!("vector.edit.paint", "Paint bucket", "{x, y} (page point: fills or unfills the closed area under it)", true, |s, p| {
            let ve = editing(s)?;
            let lp = local(s, ve.id, p, "vector.edit.paint")?;
            let filled = paint(s.doc_mut()?, ve.id, lp)?;
            Ok(json!({"filled": filled}))
        }),
        spec!("vector.edit.cut", "Cut path", "{segment, t?} | {vertex}", true, |s, p| {
            let ve = editing(s)?;
            let ends = if let Some(v) = num(p, "vertex") {
                let v = if v >= 0.0 { v as usize } else { usize::MAX };
                edit(s.doc_mut()?, ve.id, |net| ops::cut_vertex(net, v))?
            } else {
                let seg = index("vector.edit.cut", p, "segment")?;
                let t = num_or(p, "t", 0.5);
                edit(s.doc_mut()?, ve.id, |net| ops::cut_segment(net, seg, t))?.map(|(a, b)| vec![a, b]).unwrap_or_default()
            };
            if ends.is_empty() {
                return Err(bad("vector.edit.cut", "nothing to cut there"));
            }
            set_state(s, |ve| {
                ve.vertices = ends.clone();
                ve.segments.clear();
            })?;
            Ok(json!({"vertices": ends}))
        }),
        spec!(
            "vector.edit.erase",
            "Erase",
            "{points: [[x, y], ...] (page), radius? (page units, default 8)} — removes the parts of paths the brush passes over",
            true,
            |s, p| {
                let ve = editing(s)?;
                let w = s.doc()?.world_transform(ve.id);
                let inv = w.inverse();
                let scale = w.determinant().abs().sqrt().max(1e-9);
                let r = num_or(p, "radius", 8.0).clamp(0.01, 1e5) / scale;
                let pts: Vec<Point> = match get(p, "points") {
                    Some(Value::Array(a)) => {
                        a.iter().filter_map(|q| Some(inv * Point::new(q.get(0).and_then(f64_of)?, q.get(1).and_then(f64_of)?))).take(10_000).collect()
                    }
                    _ => vec![],
                };
                let changed = edit(s.doc_mut()?, ve.id, |net| pts.iter().fold(false, |acc, c| ops::erase(net, *c, r) | acc))?;
                set_state(s, |ve| {
                    ve.vertices.clear();
                    ve.segments.clear();
                })?;
                Ok(json!({"changed": changed}))
            }
        ),
        spec!("vector.edit.connect", "Connect points", "{a, b} (adds a straight segment)", true, |s, p| {
            let ve = editing(s)?;
            let (a, b) = (index("vector.edit.connect", p, "a")?, index("vector.edit.connect", p, "b")?);
            let seg = edit(s.doc_mut()?, ve.id, |net| ops::connect(net, a, b, (0.0, 0.0), (0.0, 0.0)))?
                .ok_or_else(|| bad("vector.edit.connect", "can't connect those points"))?;
            Ok(json!({"segment": seg}))
        }),
        spec!("vector.edit.lasso", "Lasso select", "{points: [[x, y], ...] (page), add?}", false, |s, p| {
            let ve = editing(s)?;
            let poly: Vec<Point> = match get(p, "points") {
                Some(Value::Array(a)) => {
                    a.iter().filter_map(|q| Some(Point::new(q.get(0).and_then(f64_of)?, q.get(1).and_then(f64_of)?))).take(100_000).collect()
                }
                _ => vec![],
            };
            let found = vertices_in_polygon(s.doc()?, ve.id, &poly);
            let add = boolean(p, "add").unwrap_or(false);
            set_state(s, |ve| {
                if !add {
                    ve.vertices.clear();
                    ve.segments.clear();
                }
                for v in &found {
                    if !ve.vertices.contains(v) {
                        ve.vertices.push(*v);
                    }
                }
            })?;
            Ok(json!({"vertices": found}))
        }),
        spec!(
            "vector.edit.pen",
            "Pen point",
            "{x, y (page), dx?, dy? (handle drag)} — continues from the pen's open end; clicking a point connects to it",
            true,
            |s, p| {
                let ve = editing(s)?;
                let pt = Point::new(req_num("vector.edit.pen", p, "x")?, req_num("vector.edit.pen", p, "y")?);
                let drag = Vec2::new(num_or(p, "dx", 0.0), num_or(p, "dy", 0.0));
                let v = s.pen_click(&ve, pt, drag, 1.0)?;
                Ok(json!({"vertex": v}))
            }
        ),
        spec!("vector.edit.penEnd", "End pen path", "{}", false, |s, _| {
            editing(s)?;
            set_state(s, |ve| {
                ve.anchor = None;
                ve.pen_out = None;
            })?;
            Ok(json!({"ok": true}))
        }),
    ]
    .into_iter()
    .chain(transform::specs())
    .chain(width::specs())
    .collect()
}

/// The paint bucket on node `id` at local point `lp`. A vector without fills gets the default
/// fill so the painted area shows.
pub fn paint(doc: &mut Document, id: NodeId, lp: Point) -> Result<bool> {
    let filled = edit(doc, id, |net| net.paint(lp))?.ok_or_else(|| bad("vector.edit.paint", "click inside a closed area"))?;
    let n = doc.get_mut(id)?;
    if filled && n.fills.iter().all(|f| !f.visible) {
        n.fills = vec![figcraft_color::Paint::solid(figcraft_color::Color::DEFAULT_SHAPE)];
    }
    Ok(filled)
}
