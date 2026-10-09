//! Pointer gestures in vector edit mode, one per edit tool. Each gesture is one undo step: it
//! starts an interaction and previews its edit against the snapshot until the pointer goes up.

use figcraft_doc::*;
use figcraft_geom::{Point, Rect, Vec2};

use super::{VHit, VTool, VectorEdit, edit, editing, hit, network, ops, set_state};
use crate::tools::{Cursor, Gesture, PointerEvent, PointerKind};
use crate::{Result, Session};

/// An in-progress edit-mode gesture.
#[derive(Clone, Debug)]
pub(crate) enum VGesture {
    /// Drag points (also a segment's ends). `split` re-adds a point on a segment first.
    Points {
        start: Point,
        verts: Vec<usize>,
        split: Option<(usize, f64)>,
        moved: bool,
    },
    Handle {
        seg: usize,
        start: bool,
        independent: bool,
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

    fn vector_hover(&mut self, ve: &VectorEdit, ev: &PointerEvent) -> Result<()> {
        let doc = self.doc()?.clone();
        let tol = 6.0 / ev.zoom.max(1e-6);
        let h = hit(&doc, ve, ev.p(), tol);
        let ov = &mut self.tool.overlay;
        ov.vector_hover = h;
        ov.face.clear();
        ov.rubber = None;
        ov.cursor = match ve.tool {
            VTool::Move if ev.cmd => Cursor::Crosshair,
            VTool::Move => Cursor::Default,
            VTool::Pen => Cursor::Pen,
            _ => Cursor::Crosshair,
        };
        let w = doc.world_transform(ve.id);
        match ve.tool {
            VTool::Pen => {
                if let Some(a) = ve.anchor.and_then(|a| network(&doc, ve.id)?.vertices.get(a).copied()) {
                    let ap = w * a.point();
                    self.tool.overlay.rubber = Some([ap.x, ap.y, ev.x, ev.y]);
                }
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
        let tool = if ev.cmd && ve.tool == VTool::Move { VTool::Bend } else { ve.tool };
        let id = ve.id;
        let w = doc.world_transform(id);
        match tool {
            VTool::Move => match h {
                Some(VHit::Handle { seg, start }) => {
                    self.begin_interaction("Edit handle")?;
                    self.vgesture(VGesture::Handle { seg, start, independent: ev.alt });
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
                    self.begin_interaction("Move points")?;
                    self.vgesture(VGesture::Points { start: ev.p(), verts: moving, split: None, moved: false });
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
                        self.vgesture(VGesture::Points { start: ev.p(), verts: vec![v], split: Some((seg, t)), moved: false });
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
                        self.begin_interaction("Move segment")?;
                        self.vgesture(VGesture::Points { start: ev.p(), verts: ends, split: None, moved: false });
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
                    self.begin_interaction("Edit handle")?;
                    self.vgesture(VGesture::Handle { seg, start, independent: true });
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
                let Some(plan) = self.pen_plan(ve, h, w.inverse() * ev.p(), ev.shift) else { return Ok(true) };
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
        let w = self.doc()?.world_transform(id);
        let inv = w.inverse();
        let lp = inv * ev.p();
        let to_local = |d: Vec2| inv * (Point::ZERO + d) - inv * Point::ZERO;
        match g {
            VGesture::Points { start, verts, split, .. } => {
                let mut d = ev.p() - start;
                if d.hypot() * ev.zoom < 1.0 {
                    return Ok(());
                }
                if ev.shift {
                    if d.x.abs() > d.y.abs() {
                        d.y = 0.0;
                    } else {
                        d.x = 0.0;
                    }
                }
                let ld = to_local(d);
                self.preview(|doc, _| {
                    edit(doc, id, |net| {
                        if let Some((seg, t)) = split {
                            ops::split_segment(net, seg, t);
                        }
                        ops::move_vertices(net, &verts, ld);
                    })
                })?;
                self.vgesture(VGesture::Points { start, verts, split, moved: true });
            }
            VGesture::Handle { seg, start, independent } => {
                let alt = independent || ev.alt;
                self.preview(|doc, _| {
                    edit(doc, id, |net| {
                        let Some(s) = net.segments.get(seg).copied() else { return };
                        let Some(v) = net.vertices.get(if start { s.a } else { s.b }).copied() else { return };
                        ops::set_handle(net, seg, start, (lp.x - v.x, lp.y - v.y), alt);
                    })
                })?;
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
        }
        Ok(())
    }

    fn vector_up(&mut self, ve: &VectorEdit, _ev: &PointerEvent) -> Result<()> {
        let g = std::mem::take(&mut self.tool.gesture);
        self.tool.overlay.marquee = None;
        self.tool.overlay.lasso.clear();
        let Gesture::Vector(g) = g else { return Ok(()) };
        match g {
            VGesture::Points { moved, split, .. } => {
                if moved || split.is_some() {
                    self.commit_interaction()?;
                } else {
                    self.cancel_interaction()?;
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
            VGesture::Handle { .. } | VGesture::BendSegment { .. } | VGesture::Pen { .. } => self.commit_interaction()?,
            VGesture::Marquee { .. } | VGesture::Lasso { .. } => {}
        }
        if let Ok(ds) = self.ds_mut() {
            ds.revision += 1;
        }
        Ok(())
    }
}
