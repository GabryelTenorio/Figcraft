//! Pointer gestures in vector edit mode, one per edit tool. Each gesture is one undo step: it
//! starts an interaction and previews its edit against the snapshot until the pointer goes up.
//! Positions are read in the snapshot's coordinates (the preview re-fits the vector, which moves
//! its origin). Dragged points and handles, and pen clicks, snap (see `point_snap.rs`).

use std::sync::Arc;

use figcraft_doc::*;
use figcraft_geom::{Affine, Point, Rect, Vec2};

use super::point_snap;
use super::transform::{self, BoxHit, PointBox};
use super::{VHit, VTool, VectorEdit, edit, editing, hit, network, ops, set_state};
use crate::snap::Targets;
use crate::tools::{Cursor, Gesture, Handle, PointerEvent, PointerKind};
use crate::{Result, Session};

/// An in-progress edit-mode gesture (cheap to clone: snap targets are shared).
#[derive(Clone, Debug)]
pub(crate) enum VGesture {
    /// Drag points (also a segment's ends). `split` re-adds a point on a segment first; a click
    /// without a drag clears the point selection when `click_clears` (inside the transform box).
    Points {
        start: Point,
        verts: Vec<usize>,
        split: Option<(usize, f64)>,
        moved: bool,
        click_clears: bool,
        snap: Arc<PointSnap>,
    },
    Handle {
        seg: usize,
        start: bool,
        independent: bool,
        targets: Arc<Targets>,
    },
    /// The transform box: a corner or edge scales the points in it.
    BoxScale {
        b: PointBox,
        handle: Handle,
        targets: Arc<Targets>,
    },
    /// The transform box: outside a corner rotates the points about its centre (`angle` so far,
    /// degrees clockwise on screen).
    BoxRotate {
        b: PointBox,
        start_angle: f64,
        angle: f64,
    },
    BendSegment {
        seg: usize,
        t: f64,
    },
    /// Bend tool on a point: drag pulls out handles, a click on a smooth point sharpens it.
    BendVertex {
        v: usize,
        origin: Point,
        moved: bool,
        smooth: bool,
    },
    Marquee {
        start: Point,
        base: Vec<usize>,
    },
    Lasso {
        points: Vec<Point>,
        base: Vec<usize>,
    },
    Pen {
        plan: PenPlan,
        origin: Point,
        out: Option<(f64, f64)>,
    },
    /// Eraser strokes so far (local points) and the brush radius (local units).
    Erase {
        stamps: Vec<Point>,
        radius: f64,
    },
    /// Variable width tool.
    Width(super::width::WidthGesture),
}

/// Snapping for dragged points: their page positions at the start, the one under the pointer (kept
/// on whole pixels) and the targets.
#[derive(Clone, Debug)]
pub(crate) struct PointSnap {
    from: Vec<Point>,
    grab: Option<Point>,
    targets: Targets,
}

/// Eraser brush radius in screen pixels.
const ERASER_PX: f64 = 8.0;

fn erase_all(net: &mut VectorNetwork, stamps: &[Point], radius: f64) -> bool {
    stamps.iter().fold(false, |acc, c| ops::erase(net, *c, radius) | acc)
}

/// What a pen click does (replayed against the snapshot while the handle is dragged).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PenPlan {
    /// A new point at `at` (local), connected from the pen's open end.
    New { at: Point, from: Option<usize> },
    /// Connect the open end to an existing point (ends the path).
    Connect { from: usize, to: usize },
    /// Add a point on a segment, connected from the open end if any (ends the path).
    Split { seg: usize, t: f64, from: Option<usize> },
}

/// Apply a pen plan; `out` is the open end's outgoing handle, `drag` the handle being dragged out
/// of the new point. Returns the point the click landed on.
fn apply_pen(net: &mut VectorNetwork, plan: PenPlan, out: Option<(f64, f64)>, drag: Option<Vec2>) -> Option<usize> {
    let out = out.unwrap_or((0.0, 0.0));
    let back = drag.map(|d| (-d.x, -d.y)).unwrap_or((0.0, 0.0));
    match plan {
        PenPlan::New { at, from } => {
            net.vertices.push(Vertex::new(at.x, at.y));
            let v = net.vertices.len() - 1;
            if let Some(f) = from {
                ops::connect(net, f, v, out, back);
            }
            if drag.is_some()
                && let Some(x) = net.vertices.get_mut(v)
            {
                x.mirror = Some(HandleMirroring::AngleAndLength);
            }
            Some(v)
        }
        PenPlan::Connect { from, to } => {
            let others = net.incident(to);
            let seg = ops::connect(net, from, to, out, back)?;
            // Dragging while closing shapes the curve on both sides of the point.
            if let (Some(d), [(os, ost)]) = (drag, others.as_slice()) {
                net.set_handle(*os, *ost, (d.x, d.y));
                if let Some(x) = net.vertices.get_mut(to) {
                    x.mirror = Some(HandleMirroring::AngleAndLength);
                }
            }
            let _ = seg;
            Some(to)
        }
        PenPlan::Split { seg, t, from } => {
            let v = ops::split_segment(net, seg, t)?;
            if let Some(f) = from {
                ops::connect(net, f, v, out, (0.0, 0.0));
            }
            Some(v)
        }
    }
}

impl Session {
    /// The transform box to draw around the selected points (page corners tl, tr, br, bl): turned
    /// with them while a rotation is dragged.
    pub fn vector_edit_box(&self) -> Option<[Point; 4]> {
        let ds = self.ds().ok()?;
        if let Gesture::Vector(VGesture::BoxRotate { b, angle, .. }) = &self.tool.gesture {
            let m = figcraft_geom::rotate_about(Affine::IDENTITY, b.center(), -angle);
            return Some(b.corners().map(|c| m * c));
        }
        transform::shown_box(&ds.doc, ds.vector_edit.as_ref()?).map(|b| b.corners())
    }

    /// Feed a pointer event to vector edit mode (the canvas is in edit mode and the main tool is
    /// Move). Returns false when edit mode ended and the event should go to the normal tool.
    pub(crate) fn vector_pointer(&mut self, ev: &PointerEvent) -> Result<bool> {
        let Ok(ve) = editing(self) else { return Ok(false) };
        match ev.kind {
            PointerKind::Hover => self.vector_hover(&ve, ev).map(|()| true),
            PointerKind::Down => self.vector_down(&ve, ev),
            PointerKind::Move => self.vector_drag(&ve, ev).map(|()| true),
            PointerKind::Up => self.vector_up(&ve, ev).map(|()| true),
            PointerKind::DoubleClick => {
                let tol = 6.0 / ev.zoom.max(1e-6);
                let doc = self.doc()?.clone();
                if ve.anchor.is_some() {
                    super::escape(self)?;
                } else if hit(&doc, &ve, ev.p(), tol).is_none() && ve.tool == VTool::Move {
                    super::end(self)?;
                }
                Ok(true)
            }
        }
    }

    fn vgesture(&mut self, g: VGesture) {
        self.tool.gesture = Gesture::Vector(g);
    }

    /// The document the gesture started from (its preview is applied to it); the current one
    /// outside gestures.
    fn gesture_doc(&self) -> Result<Arc<Document>> {
        let ds = self.ds()?;
        Ok(ds.interaction.as_ref().map_or_else(|| ds.doc.clone(), |it| it.doc.clone()))
    }

    /// A gesture dragging points `verts` of vector `id` from `start` (positions and snap targets
    /// taken from the document as it is now).
    fn points_gesture(&self, id: NodeId, start: Point, verts: Vec<usize>, split: Option<(usize, f64)>, click_clears: bool) -> Result<VGesture> {
        let ds = self.ds()?;
        let w = ds.doc.world_transform(id);
        let from: Vec<Point> =
            network(&ds.doc, id).map(|n| verts.iter().filter_map(|v| n.vertices.get(*v)).map(|v| w * v.point()).collect()).unwrap_or_default();
        let grab = from.iter().copied().min_by(|a, b| (*a - start).hypot().total_cmp(&(*b - start).hypot()));
        let targets = point_snap::targets(&ds.doc, ds.page, Some(id), &verts, &[]);
        Ok(VGesture::Points { start, verts, split, moved: false, click_clears, snap: Arc::new(PointSnap { from, grab, targets }) })
    }

    /// A gesture dragging handle `start` of segment `seg` (snapping clear of its own point).
    fn handle_gesture(&self, id: NodeId, seg: usize, start: bool, independent: bool) -> Result<VGesture> {
        let ds = self.ds()?;
        let own: Vec<usize> = network(&ds.doc, id).and_then(|n| n.segments.get(seg)).map(|s| vec![if start { s.a } else { s.b }]).unwrap_or_default();
        let targets = Arc::new(point_snap::targets(&ds.doc, ds.page, Some(id), &own, &[]));
        Ok(VGesture::Handle { seg, start, independent, targets })
    }

    /// Start a transform box gesture.
    fn box_down(&mut self, ve: &VectorEdit, bh: BoxHit, ev: &PointerEvent) -> Result<()> {
        let ds = self.ds()?;
        let (doc, page) = (ds.doc.clone(), ds.page);
        let Some(b) = transform::shown_box(&doc, ve) else { return Ok(()) };
        match bh {
            BoxHit::Inside => {
                let g = self.points_gesture(ve.id, ev.p(), b.points, None, true)?;
                self.begin_interaction("Move points")?;
                self.vgesture(g);
            }
            BoxHit::Handle(Handle::Rotate(_)) => {
                let start_angle = (ev.p() - b.center()).atan2();
                self.begin_interaction("Rotate points")?;
                self.vgesture(VGesture::BoxRotate { b, start_angle, angle: 0.0 });
            }
            BoxHit::Handle(handle) => {
                let targets = Arc::new(point_snap::targets(&doc, page, Some(ve.id), &b.points, &[]));
                self.begin_interaction("Scale points")?;
                self.vgesture(VGesture::BoxScale { b, handle, targets });
            }
        }
        Ok(())
    }

    fn vector_hover(&mut self, ve: &VectorEdit, ev: &PointerEvent) -> Result<()> {
        let doc = self.doc()?.clone();
        let tol = 6.0 / ev.zoom.max(1e-6);
        let width_tool = ve.tool == VTool::Width;
        let h = if width_tool { None } else { hit(&doc, ve, ev.p(), tol) };
        // A pen click on empty canvas lands where it snaps.
        let pen_at = if ve.tool == VTool::Pen && h.is_none() && !ev.shift { Some(self.snap_pen(ev, Some(ve.id), &[])?) } else { None };
        let ov = &mut self.tool.overlay;
        ov.vector_hover = h;
        ov.width_hover = None;
        ov.face.clear();
        ov.rubber = None;
        ov.guides.clear();
        ov.brush = (ve.tool == VTool::Eraser).then(|| [ev.x, ev.y, ERASER_PX / ev.zoom.max(1e-6)]);
        ov.cursor = match ve.tool {
            VTool::Move if ev.cmd => Cursor::Crosshair,
            VTool::Move => Cursor::Default,
            VTool::Pen => Cursor::Pen,
            _ => Cursor::Crosshair,
        };
        let w = doc.world_transform(ve.id);
        match ve.tool {
            VTool::Move if !ev.cmd => {
                // Over the transform box's handles: their cursor, and no point highlighted.
                if let Some(BoxHit::Handle(handle)) = transform::box_hit(&doc, ve, h, ev.p(), ev.zoom)
                    && let Some(b) = transform::shown_box(&doc, ve)
                {
                    self.tool.overlay.cursor = crate::tools::handle_cursor(&b.corners(), handle);
                    self.tool.overlay.vector_hover = None;
                }
            }
            VTool::Pen => {
                let (to, guides) = pen_at.unwrap_or((ev.p(), Vec::new()));
                if let Some(a) = ve.anchor.and_then(|a| network(&doc, ve.id)?.vertices.get(a).copied()) {
                    let ap = w * a.point();
                    self.tool.overlay.rubber = Some([ap.x, ap.y, to.x, to.y]);
                }
                self.tool.overlay.guides = guides;
            }
            VTool::Paint => {
                // Highlight the closed area the bucket would fill (skipped on huge networks).
                if let Some(net) = network(&doc, ve.id).filter(|n| n.segments.len() <= 3000)
                    && let Some(face) = net.face_at(w.inverse() * ev.p())
                {
                    let mut polys: Vec<Vec<[f64; 2]>> = Vec::new();
                    let mut path = face.path.clone();
                    path.apply_affine(w);
                    figcraft_geom::kurbo::flatten(path.iter(), 0.25 / ev.zoom.max(1e-6), |el| {
                        use figcraft_geom::kurbo::PathEl;
                        match el {
                            PathEl::MoveTo(p) => polys.push(vec![[p.x, p.y]]),
                            PathEl::LineTo(p) => {
                                if let Some(l) = polys.last_mut() {
                                    l.push([p.x, p.y]);
                                }
                            }
                            _ => {}
                        }
                    });
                    self.tool.overlay.face = polys;
                }
            }
            VTool::Width => self.width_hover(ve, ev)?,
            _ => {}
        }
        if let Ok(ds) = self.ds_mut() {
            ds.revision += 1;
        }
        Ok(())
    }

    /// Returns false when the click left edit mode (the caller then runs the normal tool).
    fn vector_down(&mut self, ve: &VectorEdit, ev: &PointerEvent) -> Result<bool> {
        let doc = self.doc()?.clone();
        let tol = 6.0 / ev.zoom.max(1e-6);
        let h = hit(&doc, ve, ev.p(), tol);
        self.tool.overlay.vector_hover = None;
        self.tool.overlay.face.clear();
        self.tool.overlay.guides.clear();
        let tool = if ev.cmd && ve.tool == VTool::Move { VTool::Bend } else { ve.tool };
        let id = ve.id;
        let w = doc.world_transform(id);
        // The transform box (Shift inside it adds to the selection instead).
        if tool == VTool::Move
            && let Some(bh) = transform::box_hit(&doc, ve, h, ev.p(), ev.zoom).filter(|bh| !(ev.shift && *bh == BoxHit::Inside))
        {
            self.box_down(ve, bh, ev)?;
            return Ok(true);
        }
        match tool {
            VTool::Move => match h {
                Some(VHit::Handle { seg, start }) => {
                    let g = self.handle_gesture(id, seg, start, ev.alt)?;
                    self.begin_interaction("Edit handle")?;
                    self.vgesture(g);
                }
                Some(VHit::Vertex { index }) => {
                    let verts = if ev.shift {
                        let mut v = ve.vertices.clone();
                        if let Some(i) = v.iter().position(|x| *x == index) {
                            v.remove(i);
                        } else {
                            v.push(index);
                        }
                        v
                    } else if ve.vertices.contains(&index) {
                        ve.vertices.clone()
                    } else {
                        vec![index]
                    };
                    let keep_segs = ev.shift;
                    set_state(self, |e| {
                        e.vertices = verts.clone();
                        if !keep_segs {
                            e.segments.clear();
                        }
                    })?;
                    let moving = network(&doc, id)
                        .map(|n| {
                            super::selected_points(
                                n,
                                &VectorEdit { vertices: verts, segments: if keep_segs { ve.segments.clone() } else { vec![] }, ..ve.clone() },
                            )
                        })
                        .unwrap_or_default();
                    let g = self.points_gesture(id, ev.p(), moving, None, false)?;
                    self.begin_interaction("Move points")?;
                    self.vgesture(g);
                }
                Some(VHit::Segment { seg, t }) => {
                    if ev.alt {
                        // Add a point on the segment and drag it.
                        self.begin_interaction("Add point")?;
                        let mut v = None;
                        self.preview(|doc, _| {
                            v = edit(doc, id, |net| ops::split_segment(net, seg, t))?;
                            Ok(())
                        })?;
                        let Some(v) = v else { return Ok(true) };
                        set_state(self, |e| {
                            e.vertices = vec![v];
                            e.segments.clear();
                        })?;
                        let g = self.points_gesture(id, ev.p(), vec![v], Some((seg, t)), false)?;
                        self.vgesture(g);
                    } else if ev.shift {
                        set_state(self, |e| {
                            if let Some(i) = e.segments.iter().position(|x| *x == seg) {
                                e.segments.remove(i);
                            } else {
                                e.segments.push(seg);
                            }
                        })?;
                    } else {
                        let ends = network(&doc, id).and_then(|n| n.segments.get(seg)).map(|s| vec![s.a, s.b]).unwrap_or_default();
                        set_state(self, |e| {
                            e.vertices.clear();
                            e.segments = vec![seg];
                        })?;
                        let g = self.points_gesture(id, ev.p(), ends, None, false)?;
                        self.begin_interaction("Move segment")?;
                        self.vgesture(g);
                    }
                }
                None => {
                    // Outside the shape entirely (and not shift): leave edit mode.
                    let inside = doc.world_render_bounds(id).is_some_and(|b| b.inflate(tol * 4.0, tol * 4.0).contains(ev.p()));
                    if !inside && !ev.shift {
                        super::end(self)?;
                        return Ok(false);
                    }
                    let base = if ev.shift { ve.vertices.clone() } else { vec![] };
                    set_state(self, |e| {
                        e.vertices = base.clone();
                        if !ev.shift {
                            e.segments.clear();
                        }
                    })?;
                    self.vgesture(VGesture::Marquee { start: ev.p(), base });
                }
            },
            VTool::Bend => match h {
                Some(VHit::Handle { seg, start }) => {
                    let g = self.handle_gesture(id, seg, start, true)?;
                    self.begin_interaction("Edit handle")?;
                    self.vgesture(g);
                }
                Some(VHit::Vertex { index }) => {
                    let smooth =
                        network(&doc, id).is_some_and(|n| n.incident(index).iter().any(|(s, st)| n.handle(*s, *st).is_some_and(|h| h != (0.0, 0.0))));
                    let origin = network(&doc, id).and_then(|n| n.vertices.get(index)).map(|v| v.point()).unwrap_or_default();
                    set_state(self, |e| {
                        e.vertices = vec![index];
                        e.segments.clear();
                    })?;
                    self.begin_interaction("Bend")?;
                    self.vgesture(VGesture::BendVertex { v: index, origin, moved: false, smooth });
                }
                Some(VHit::Segment { seg, t }) => {
                    self.begin_interaction("Bend")?;
                    self.vgesture(VGesture::BendSegment { seg, t });
                }
                None => {
                    let base = if ev.shift { ve.vertices.clone() } else { vec![] };
                    self.vgesture(VGesture::Marquee { start: ev.p(), base });
                }
            },
            VTool::Pen => {
                let at = if h.is_none() && !ev.shift { self.snap_pen(ev, Some(id), &[])?.0 } else { ev.p() };
                let Some(plan) = self.pen_plan(ve, h, w.inverse() * at, ev.shift) else { return Ok(true) };
                self.begin_interaction("Pen")?;
                let out = ve.pen_out;
                let mut v = None;
                self.preview(|doc, _| {
                    v = edit(doc, id, |net| apply_pen(net, plan, out, None))?;
                    Ok(())
                })?;
                self.pen_state(plan, v, None)?;
                self.vgesture(VGesture::Pen { plan, origin: ev.p(), out });
            }
            VTool::Paint => {
                self.begin_interaction("Paint bucket")?;
                let lp = w.inverse() * ev.p();
                if self.preview(|doc, _| super::paint(doc, id, lp).map(|_| ())).is_ok() {
                    self.commit_interaction()?;
                } else {
                    self.cancel_interaction()?;
                }
            }
            VTool::Eraser => {
                let scale = w.determinant().abs().sqrt().max(1e-9);
                let radius = ERASER_PX / ev.zoom.max(1e-6) / scale;
                let stamps = vec![w.inverse() * ev.p()];
                self.begin_interaction("Eraser")?;
                let st = stamps.clone();
                self.preview(|doc, _| edit(doc, id, |net| erase_all(net, &st, radius)).map(|_| ()))?;
                set_state(self, |e| {
                    e.vertices.clear();
                    e.segments.clear();
                })?;
                self.vgesture(VGesture::Erase { stamps, radius });
            }
            VTool::Width => self.width_down(ve, ev)?,
            VTool::Lasso => {
                let base = if ev.shift { ve.vertices.clone() } else { vec![] };
                self.tool.overlay.lasso = vec![[ev.x, ev.y]];
                self.vgesture(VGesture::Lasso { points: vec![ev.p()], base });
            }
            VTool::Cut => {
                let res: Option<Vec<usize>> = match h {
                    Some(VHit::Vertex { index }) => {
                        self.begin_interaction("Cut")?;
                        let mut out = vec![];
                        self.preview(|doc, _| {
                            out = edit(doc, id, |net| ops::cut_vertex(net, index))?;
                            Ok(())
                        })?;
                        Some(out)
                    }
                    Some(VHit::Segment { seg, t }) => {
                        self.begin_interaction("Cut")?;
                        let mut out = vec![];
                        self.preview(|doc, _| {
                            out = edit(doc, id, |net| ops::cut_segment(net, seg, t))?.map(|(a, b)| vec![a, b]).unwrap_or_default();
                            Ok(())
                        })?;
                        Some(out)
                    }
                    _ => None,
                };
                if let Some(ends) = res {
                    self.commit_interaction()?;
                    set_state(self, |e| {
                        e.vertices = ends;
                        e.segments.clear();
                    })?;
                }
            }
        }
        Ok(true)
    }

    /// What a pen click at local point `lp` does.
    fn pen_plan(&mut self, ve: &VectorEdit, h: Option<VHit>, lp: Point, shift: bool) -> Option<PenPlan> {
        let doc = self.doc().ok()?;
        let net = network(doc, ve.id)?;
        match h {
            Some(VHit::Vertex { index }) => match ve.anchor {
                Some(a) if a != index => Some(PenPlan::Connect { from: a, to: index }),
                Some(_) => {
                    let _ = super::escape(self);
                    None
                }
                None => {
                    // Continue (or branch) from an existing point.
                    let _ = set_state(self, |e| {
                        e.anchor = Some(index);
                        e.pen_out = None;
                        e.vertices = vec![index];
                        e.segments.clear();
                    });
                    None
                }
            },
            Some(VHit::Segment { seg, t }) => Some(PenPlan::Split { seg, t, from: ve.anchor }),
            _ => {
                let mut at = lp;
                // Shift: 45° steps from the open end.
                if shift && let Some(a) = ve.anchor.and_then(|a| net.vertices.get(a)) {
                    let v = lp - a.point();
                    let step = std::f64::consts::FRAC_PI_4;
                    let ang = (v.atan2() / step).round() * step;
                    at = a.point() + Vec2::from_angle(ang) * v.hypot();
                }
                Some(PenPlan::New { at, from: ve.anchor })
            }
        }
    }

    /// Edit state after a pen click.
    fn pen_state(&mut self, plan: PenPlan, v: Option<usize>, out: Option<(f64, f64)>) -> Result<()> {
        set_state(self, |e| {
            e.segments.clear();
            e.vertices = v.into_iter().collect();
            match plan {
                PenPlan::New { .. } => {
                    e.anchor = v;
                    e.pen_out = out;
                }
                PenPlan::Connect { .. } | PenPlan::Split { .. } => {
                    e.anchor = None;
                    e.pen_out = None;
                }
            }
        })
    }

    /// A pen click from a command (`drag` = the handle pulled out of the new point, page units).
    pub(crate) fn pen_click(&mut self, ve: &VectorEdit, p: Point, drag: Vec2, zoom: f64) -> Result<Option<usize>> {
        let doc = self.doc()?.clone();
        let tol = 6.0 / zoom.max(1e-6);
        let w = doc.world_transform(ve.id);
        let Some(plan) = self.pen_plan(ve, hit(&doc, ve, p, tol), w.inverse() * p, false) else {
            return Ok(self.ds()?.vector_edit.as_ref().and_then(|e| e.anchor));
        };
        let inv = w.inverse();
        let ld = inv * (Point::ZERO + drag) - inv * Point::ZERO;
        let drag = (ld.hypot() > 1e-9 && ld.x.is_finite() && ld.y.is_finite()).then_some(ld);
        let v = edit(self.doc_mut()?, ve.id, |net| apply_pen(net, plan, ve.pen_out, drag))?;
        self.pen_state(plan, v, drag.map(|d| (d.x, d.y)))?;
        Ok(v)
    }

    fn vector_drag(&mut self, ve: &VectorEdit, ev: &PointerEvent) -> Result<()> {
        let Gesture::Vector(g) = self.tool.gesture.clone() else { return Ok(()) };
        let id = ve.id;
        let w = self.gesture_doc()?.world_transform(id);
        let modes = point_snap::modes(self, ev);
        let inv = w.inverse();
        let lp = inv * ev.p();
        let to_local = |d: Vec2| inv * (Point::ZERO + d) - inv * Point::ZERO;
        match g {
            VGesture::Points { start, verts, split, click_clears, snap, .. } => {
                let mut d = ev.p() - start;
                if d.hypot() * ev.zoom < 1.0 {
                    return Ok(());
                }
                // Shift: along the axis moved most.
                let lock = ev.shift.then_some(d.x.abs() > d.y.abs());
                match lock {
                    Some(true) => d.y = 0.0,
                    Some(false) => d.x = 0.0,
                    None => {}
                }
                let moved: Vec<Point> = snap.from.iter().map(|p| *p + d).collect();
                let (c, guides) = point_snap::snap(&snap.targets, &moved, snap.grab.map(|g| g + d), ev.zoom, modes, lock);
                d += c;
                self.tool.overlay.guides = guides;
                let ld = to_local(d);
                self.preview(|doc, _| {
                    edit(doc, id, |net| {
                        if let Some((seg, t)) = split {
                            ops::split_segment(net, seg, t);
                        }
                        ops::move_vertices(net, &verts, ld);
                    })
                })?;
                self.vgesture(VGesture::Points { start, verts, split, moved: true, click_clears, snap });
            }
            VGesture::Handle { seg, start, independent, targets } => {
                let alt = independent || ev.alt;
                let (c, guides) = point_snap::snap(&targets, &[ev.p()], Some(ev.p()), ev.zoom, modes, None);
                self.tool.overlay.guides = guides;
                let lp = inv * (ev.p() + c);
                self.preview(|doc, _| {
                    edit(doc, id, |net| {
                        let Some(s) = net.segments.get(seg).copied() else { return };
                        let Some(v) = net.vertices.get(if start { s.a } else { s.b }).copied() else { return };
                        ops::set_handle(net, seg, start, (lp.x - v.x, lp.y - v.y), alt);
                    })
                })?;
            }
            VGesture::BoxScale { b, handle, targets } => {
                // The dragged corner or edge snaps like a resize handle.
                let (c, guides) = point_snap::snap(&targets, &[ev.p()], None, ev.zoom, (modes.0, false), None);
                self.tool.overlay.guides = guides;
                let q = b.frame.inverse() * (ev.p() + c);
                let (x0, y0, x1, y1) = crate::tools::resized_box(handle, q, b.width, b.height, ev.shift, ev.alt);
                let m = b.onto(x0, y0, x1, y1);
                self.preview(|doc, _| transform::transform_points(doc, id, &b.points, m))?;
            }
            VGesture::BoxRotate { b, start_angle, .. } => {
                let c = b.center();
                let mut angle = ((ev.p() - c).atan2() - start_angle).to_degrees();
                if ev.shift {
                    angle = (angle / 15.0).round() * 15.0;
                }
                let m = figcraft_geom::rotate_about(Affine::IDENTITY, c, -angle);
                self.preview(|doc, _| transform::transform_points(doc, id, &b.points, m))?;
                self.tool.overlay.cursor = Cursor::Rotate;
                self.vgesture(VGesture::BoxRotate { b, start_angle, angle });
            }
            VGesture::BendSegment { seg, t } => {
                self.preview(|doc, _| edit(doc, id, |net| ops::bend(net, seg, t, lp)))?;
            }
            VGesture::BendVertex { v, origin, moved, smooth } => {
                let d = lp - origin;
                let page_d = (w * lp - w * origin).hypot() * ev.zoom;
                if !moved && page_d < 2.0 {
                    return Ok(());
                }
                self.preview(|doc, _| edit(doc, id, |net| ops::make_smooth(net, v, d)))?;
                self.vgesture(VGesture::BendVertex { v, origin, moved: true, smooth });
            }
            VGesture::Marquee { start, base } => {
                let r = Rect::from_points(start, ev.p());
                self.tool.overlay.marquee = Some([r.x0, r.y0, r.x1, r.y1]);
                let mut v = base.clone();
                for i in super::vertices_in(self.doc()?, id, r) {
                    if !v.contains(&i) {
                        v.push(i);
                    }
                }
                set_state(self, |e| e.vertices = v)?;
            }
            VGesture::Lasso { mut points, base } => {
                if points.last().is_none_or(|l| (*l - ev.p()).hypot() * ev.zoom >= 2.0) && points.len() < 100_000 {
                    points.push(ev.p());
                }
                self.tool.overlay.lasso = points.iter().map(|p| [p.x, p.y]).collect();
                let mut v = base.clone();
                for i in super::vertices_in_polygon(self.doc()?, id, &points) {
                    if !v.contains(&i) {
                        v.push(i);
                    }
                }
                set_state(self, |e| {
                    e.vertices = v;
                    e.segments.clear();
                })?;
                self.vgesture(VGesture::Lasso { points, base });
            }
            VGesture::Erase { mut stamps, radius } => {
                self.tool.overlay.brush = Some([ev.x, ev.y, ERASER_PX / ev.zoom.max(1e-6)]);
                // Stamps a third of the brush apart, filling gaps on fast strokes.
                let to = lp;
                let from = stamps.last().copied().unwrap_or(to);
                let step = (radius / 3.0).max(1e-6);
                let n = ((to - from).hypot() / step).floor().min(500.0) as usize;
                for k in 1..=n {
                    stamps.push(from.lerp(to, k as f64 / n as f64));
                }
                if n > 0 && stamps.len() < 20_000 {
                    let st = stamps.clone();
                    self.preview(|doc, _| edit(doc, id, |net| erase_all(net, &st, radius)).map(|_| ()))?;
                }
                self.vgesture(VGesture::Erase { stamps, radius });
            }
            VGesture::Pen { plan, origin, out, .. } => {
                let d = to_local(ev.p() - origin);
                if (ev.p() - origin).hypot() * ev.zoom < 2.0 {
                    return Ok(());
                }
                let mut v = None;
                self.preview(|doc, _| {
                    v = edit(doc, id, |net| apply_pen(net, plan, out, Some(d)))?;
                    Ok(())
                })?;
                self.pen_state(plan, v, Some((d.x, d.y)))?;
                self.vgesture(VGesture::Pen { plan, origin, out });
            }
            VGesture::Width(g) => self.width_drag(ve, g, ev)?,
        }
        Ok(())
    }

    fn vector_up(&mut self, ve: &VectorEdit, _ev: &PointerEvent) -> Result<()> {
        let g = std::mem::take(&mut self.tool.gesture);
        self.tool.overlay.marquee = None;
        self.tool.overlay.lasso.clear();
        self.tool.overlay.guides.clear();
        let Gesture::Vector(g) = g else { return Ok(()) };
        match g {
            VGesture::Points { moved, split, click_clears, .. } => {
                if moved || split.is_some() {
                    self.commit_interaction()?;
                } else {
                    self.cancel_interaction()?;
                    if click_clears {
                        set_state(self, |e| {
                            e.vertices.clear();
                            e.segments.clear();
                        })?;
                    }
                }
            }
            VGesture::BendVertex { v, moved, smooth, .. } => {
                if moved {
                    self.commit_interaction()?;
                } else if smooth {
                    let id = ve.id;
                    self.preview(|doc, _| edit(doc, id, |net| ops::make_sharp(net, v)))?;
                    self.commit_interaction()?;
                } else {
                    self.cancel_interaction()?;
                }
            }
            VGesture::Handle { .. }
            | VGesture::BendSegment { .. }
            | VGesture::Pen { .. }
            | VGesture::BoxScale { .. }
            | VGesture::BoxRotate { .. } => self.commit_interaction()?,
            VGesture::Marquee { .. } | VGesture::Lasso { .. } => {}
            VGesture::Erase { .. } => self.commit_interaction()?,
            VGesture::Width(g) => self.width_up(g)?,
        }
        if let Ok(ds) = self.ds_mut() {
            ds.revision += 1;
        }
        Ok(())
    }
}
