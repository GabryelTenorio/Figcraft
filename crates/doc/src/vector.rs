//! Vector networks: vertices joined by curved segments, any number of segments per vertex, plus
//! filled regions made of closed loops.

use figcraft_geom::{BezPath, Point};
use serde::{Deserialize, Serialize};

/// How a vertex's two bezier handles follow each other when one is dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum HandleMirroring {
    /// Handles move independently (a corner).
    #[default]
    None,
    /// The other handle stays opposite but keeps its own length.
    Angle,
    /// The other handle mirrors angle and length (a symmetric smooth point).
    AngleAndLength,
}

impl HandleMirroring {
    pub fn id(self) -> &'static str {
        match self {
            HandleMirroring::None => "none",
            HandleMirroring::Angle => "angle",
            HandleMirroring::AngleAndLength => "angleAndLength",
        }
    }
    pub fn from_id(s: &str) -> Option<Self> {
        [HandleMirroring::None, HandleMirroring::Angle, HandleMirroring::AngleAndLength].into_iter().find(|m| m.id().eq_ignore_ascii_case(s))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    /// Per-vertex corner radius (0 = sharp).
    #[serde(default)]
    pub radius: f64,
    /// Handle mirroring; `None` infers it from the handles (see [`VectorNetwork::mirroring`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<HandleMirroring>,
}

impl Vertex {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y, radius: 0.0, mirror: None }
    }
    pub fn point(&self) -> Point {
        Point::new(self.x, self.y)
    }
}

/// A segment from vertex `a` to `b` with tangent handles relative to each end vertex.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub a: usize,
    pub b: usize,
    #[serde(default)]
    pub ta: (f64, f64),
    #[serde(default)]
    pub tb: (f64, f64),
}

impl Segment {
    pub fn line(a: usize, b: usize) -> Self {
        Self { a, b, ta: (0.0, 0.0), tb: (0.0, 0.0) }
    }
    pub fn is_straight(&self) -> bool {
        self.ta == (0.0, 0.0) && self.tb == (0.0, 0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WindingRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// A filled region: loops of segment indices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Region {
    pub loops: Vec<Vec<usize>>,
    #[serde(default)]
    pub winding: WindingRule,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct VectorNetwork {
    pub vertices: Vec<Vertex>,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub regions: Vec<Region>,
}

impl VectorNetwork {
    /// A network from a polyline / polygon.
    pub fn from_points(points: &[Point], closed: bool) -> Self {
        let vertices: Vec<Vertex> = points.iter().map(|p| Vertex::new(p.x, p.y)).collect();
        let n = vertices.len();
        let mut segments: Vec<Segment> = (1..n).map(|i| Segment::line(i - 1, i)).collect();
        let mut regions = Vec::new();
        if closed && n > 2 {
            segments.push(Segment::line(n - 1, 0));
            regions.push(Region { loops: vec![(0..segments.len()).collect()], winding: WindingRule::NonZero });
        }
        Self { vertices, segments, regions }
    }

    /// A network from a bezier path: each subpath becomes a chain (closed subpaths a loop + region).
    pub fn from_path(path: &BezPath) -> Self {
        use figcraft_geom::kurbo::PathEl;
        let mut net = VectorNetwork::default();
        let mut loop_segs: Vec<usize> = Vec::new();
        let mut first_v: Option<usize> = None;
        let mut cur_v: Option<usize> = None;
        let mut all_loops: Vec<Vec<usize>> = Vec::new();
        let push_v = |net: &mut VectorNetwork, p: Point| -> usize {
            net.vertices.push(Vertex::new(p.x, p.y));
            net.vertices.len() - 1
        };
        // A subpath that ends back on its start point without a ClosePath (e.g. ellipse outlines)
        // is closed too, so it keeps its fill.
        let close_open =
            |net: &mut VectorNetwork, loop_segs: &mut Vec<usize>, all_loops: &mut Vec<Vec<usize>>, first_v: Option<usize>, cur_v: Option<usize>| {
                let (Some(a), Some(f)) = (cur_v, first_v) else { return };
                let (Some(pa), Some(pf)) = (net.vertices.get(a).map(Vertex::point), net.vertices.get(f).map(Vertex::point)) else { return };
                if a == f || loop_segs.len() < 2 || (pa - pf).hypot() > 1e-6 || a + 1 != net.vertices.len() {
                    return;
                }
                for s in net.segments.iter_mut() {
                    if s.b == a {
                        s.b = f;
                    }
                }
                net.vertices.pop();
                all_loops.push(std::mem::take(loop_segs));
            };
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    close_open(&mut net, &mut loop_segs, &mut all_loops, first_v, cur_v);
                    loop_segs.clear();
                    let v = push_v(&mut net, p);
                    first_v = Some(v);
                    cur_v = Some(v);
                }
                PathEl::LineTo(p) => {
                    let Some(a) = cur_v else { continue };
                    let b = push_v(&mut net, p);
                    net.segments.push(Segment::line(a, b));
                    loop_segs.push(net.segments.len() - 1);
                    cur_v = Some(b);
                }
                PathEl::QuadTo(c, p) => {
                    let Some(a) = cur_v else { continue };
                    let Some(pa) = net.vertices.get(a).map(Vertex::point) else { continue };
                    let c1 = pa + (c - pa) * (2.0 / 3.0);
                    let c2 = p + (c - p) * (2.0 / 3.0);
                    let b = push_v(&mut net, p);
                    net.segments.push(Segment { a, b, ta: (c1.x - pa.x, c1.y - pa.y), tb: (c2.x - p.x, c2.y - p.y) });
                    loop_segs.push(net.segments.len() - 1);
                    cur_v = Some(b);
                }
                PathEl::CurveTo(c1, c2, p) => {
                    let Some(a) = cur_v else { continue };
                    let Some(pa) = net.vertices.get(a).map(Vertex::point) else { continue };
                    let b = push_v(&mut net, p);
                    net.segments.push(Segment { a, b, ta: (c1.x - pa.x, c1.y - pa.y), tb: (c2.x - p.x, c2.y - p.y) });
                    loop_segs.push(net.segments.len() - 1);
                    cur_v = Some(b);
                }
                PathEl::ClosePath => {
                    let (Some(a), Some(f)) = (cur_v, first_v) else { continue };
                    let close_pt = net.vertices.get(a).map(Vertex::point);
                    let first_pt = net.vertices.get(f).map(Vertex::point);
                    // The end vertex lands on the start (up to float noise): merge them.
                    let same = matches!((close_pt, first_pt), (Some(c), Some(f)) if (c - f).hypot() <= 1e-6);
                    if same && a != f {
                        // Merge the duplicate end vertex into the first one.
                        for s in net.segments.iter_mut() {
                            if s.b == a {
                                s.b = f;
                            }
                        }
                        if a + 1 == net.vertices.len() {
                            net.vertices.pop();
                        }
                    } else if a != f {
                        net.segments.push(Segment::line(a, f));
                        loop_segs.push(net.segments.len() - 1);
                    }
                    if !loop_segs.is_empty() {
                        all_loops.push(std::mem::take(&mut loop_segs));
                    }
                    cur_v = Some(f);
                }
            }
        }
        close_open(&mut net, &mut loop_segs, &mut all_loops, first_v, cur_v);
        if !all_loops.is_empty() {
            net.regions.push(Region { loops: all_loops, winding: WindingRule::NonZero });
        }
        net
    }

    /// Segments touching vertex `v`: (segment index, `true` when `v` is the segment's `a` end).
    pub fn incident(&self, v: usize) -> Vec<(usize, bool)> {
        self.segments
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                if s.a == v {
                    Some((i, true))
                } else if s.b == v {
                    Some((i, false))
                } else {
                    None
                }
            })
            .collect()
    }

    /// The handle (relative to its vertex) at one end of a segment.
    pub fn handle(&self, seg: usize, start: bool) -> Option<(f64, f64)> {
        let s = self.segments.get(seg)?;
        Some(if start { s.ta } else { s.tb })
    }

    pub fn set_handle(&mut self, seg: usize, start: bool, t: (f64, f64)) {
        if let Some(s) = self.segments.get_mut(seg) {
            if start {
                s.ta = t;
            } else {
                s.tb = t;
            }
        }
    }

    /// The vertex's handle mirroring: its explicit setting, else inferred from its two handles
    /// (opposite and equal → angle and length, opposite → angle, otherwise none).
    pub fn mirroring(&self, v: usize) -> HandleMirroring {
        if let Some(m) = self.vertices.get(v).and_then(|x| x.mirror) {
            return m;
        }
        let inc = self.incident(v);
        let [(s1, a1), (s2, a2)] = inc.as_slice() else { return HandleMirroring::None };
        let (Some(h1), Some(h2)) = (self.handle(*s1, *a1), self.handle(*s2, *a2)) else { return HandleMirroring::None };
        let (l1, l2) = (h1.0.hypot(h1.1), h2.0.hypot(h2.1));
        if l1 < 1e-9 || l2 < 1e-9 {
            return HandleMirroring::None;
        }
        // Opposite directions: the cross product is ~0 and the dot product negative.
        let cross = (h1.0 * h2.1 - h1.1 * h2.0) / (l1 * l2);
        let dot = (h1.0 * h2.0 + h1.1 * h2.1) / (l1 * l2);
        if cross.abs() > 1e-3 || dot > -0.999 {
            HandleMirroring::None
        } else if (l1 - l2).abs() <= 1e-6 * l1.max(l2).max(1.0) {
            HandleMirroring::AngleAndLength
        } else {
            HandleMirroring::Angle
        }
    }

    /// Segment `i` as a cubic bezier.
    pub fn cubic(&self, i: usize) -> Option<figcraft_geom::kurbo::CubicBez> {
        let (a, c1, c2, b) = self.seg_points(self.segments.get(i)?)?;
        Some(figcraft_geom::kurbo::CubicBez::new(a, c1, c2, b))
    }

    /// One closed loop of segments as a path.
    pub fn loop_path(&self, lp: &[usize]) -> BezPath {
        let mut path = BezPath::new();
        self.append_loop(&mut path, lp);
        path
    }

    /// Whether the rendered fill covers `p` (the renderer fills every region with one rule: even-odd
    /// when any region asks for it).
    pub fn fill_contains(&self, p: Point) -> bool {
        use figcraft_geom::Shape;
        let path = self.fill_path();
        let w = path.winding(p);
        if self.regions.iter().any(|r| r.winding == WindingRule::EvenOdd) { w % 2 != 0 } else { w != 0 }
    }

    fn seg_points(&self, s: &Segment) -> Option<(Point, Point, Point, Point)> {
        let a = self.vertices.get(s.a)?.point();
        let b = self.vertices.get(s.b)?.point();
        Some((a, Point::new(a.x + s.ta.0, a.y + s.ta.1), Point::new(b.x + s.tb.0, b.y + s.tb.1), b))
    }

    fn push_seg(&self, path: &mut BezPath, s: &Segment, reversed: bool) {
        let Some((a, c1, c2, b)) = self.seg_points(s) else { return };
        let (c1, c2, b) = if reversed { (c2, c1, a) } else { (c1, c2, b) };
        if s.is_straight() {
            path.line_to(b);
        } else {
            path.curve_to(c1, c2, b);
        }
    }

    /// The fill path: every region's loops.
    pub fn fill_path(&self) -> BezPath {
        let mut path = BezPath::new();
        for region in &self.regions {
            for lp in &region.loops {
                self.append_loop(&mut path, lp);
            }
        }
        path
    }

    fn append_loop(&self, path: &mut BezPath, lp: &[usize]) {
        // Walk the loop, orienting each segment to continue from the previous end vertex.
        let segs: Vec<&Segment> = lp.iter().filter_map(|i| self.segments.get(*i)).collect();
        let Some(first) = segs.first() else { return };
        let start_v = match segs.get(1) {
            Some(second) if first.a == second.a || first.a == second.b => first.b,
            _ => first.a,
        };
        let Some(start) = self.vertices.get(start_v) else { return };
        path.move_to(start.point());
        let mut cur = start_v;
        for s in segs {
            if s.a == cur {
                self.push_seg(path, s, false);
                cur = s.b;
            } else {
                self.push_seg(path, s, true);
                cur = s.a;
            }
        }
        path.close_path();
    }

    /// The stroke path: segments chained into maximal open runs (vertices of degree 2 continue a
    /// run); closed loops come out closed.
    pub fn stroke_path(&self) -> BezPath {
        let mut path = BezPath::new();
        let n = self.segments.len();
        let mut used = vec![false; n];
        let degree = |v: usize| self.segments.iter().filter(|s| s.a == v || s.b == v).count();
        let next_from = |v: usize, used: &[bool]| -> Option<(usize, bool)> {
            self.segments.iter().enumerate().find_map(|(i, s)| {
                if used.get(i).copied().unwrap_or(true) {
                    return None;
                }
                if s.a == v {
                    Some((i, false))
                } else if s.b == v {
                    Some((i, true))
                } else {
                    None
                }
            })
        };
        // Start runs at endpoints / junctions first, then whatever remains (pure loops).
        let mut starts: Vec<usize> = (0..self.vertices.len()).filter(|v| degree(*v) != 2).collect();
        starts.extend((0..self.vertices.len()).filter(|v| degree(*v) == 2));
        for v0 in starts {
            while let Some((si, rev)) = next_from(v0, &used) {
                let Some(start) = self.vertices.get(v0) else { break };
                path.move_to(start.point());
                let mut cur;
                let mut seg = Some((si, rev));
                while let Some((i, r)) = seg {
                    if let Some(u) = used.get_mut(i) {
                        *u = true;
                    }
                    let Some(s) = self.segments.get(i) else { break };
                    self.push_seg(&mut path, s, r);
                    cur = if r { s.a } else { s.b };
                    if cur == v0 {
                        path.close_path();
                        break;
                    }
                    seg = if degree(cur) == 2 { next_from(cur, &used) } else { None };
                }
            }
        }
        path
    }

    /// Translate/scale every vertex and handle (used when the node is resized).
    pub fn scale(&mut self, sx: f64, sy: f64) {
        for v in &mut self.vertices {
            v.x *= sx;
            v.y *= sy;
        }
        for s in &mut self.segments {
            s.ta = (s.ta.0 * sx, s.ta.1 * sy);
            s.tb = (s.tb.0 * sx, s.tb.1 * sy);
        }
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        for v in &mut self.vertices {
            v.x += dx;
            v.y += dy;
        }
    }

    /// Bounds of the curves.
    pub fn bounds(&self) -> Option<figcraft_geom::Rect> {
        use figcraft_geom::Shape;
        let p = self.stroke_path();
        if p.elements().is_empty() { None } else { Some(p.bounding_box()) }
    }

    /// Remove segments that reference missing vertices, and regions referencing missing segments.
    pub fn sanitize(&mut self) {
        let nv = self.vertices.len();
        let keep: Vec<bool> = self.segments.iter().map(|s| s.a < nv && s.b < nv).collect();
        if keep.iter().all(|k| *k) {
            let ns = self.segments.len();
            for r in &mut self.regions {
                r.loops.retain(|l| l.iter().all(|i| *i < ns));
            }
            return;
        }
        let mut remap = vec![usize::MAX; self.segments.len()];
        let mut out = Vec::new();
        for (i, s) in self.segments.iter().enumerate() {
            if keep.get(i).copied().unwrap_or(false) {
                if let Some(m) = remap.get_mut(i) {
                    *m = out.len();
                }
                out.push(*s);
            }
        }
        self.segments = out;
        for r in &mut self.regions {
            r.loops.retain(|l| l.iter().all(|i| remap.get(*i).is_some_and(|m| *m != usize::MAX)));
            for l in &mut r.loops {
                for i in l.iter_mut() {
                    *i = remap.get(*i).copied().unwrap_or(0);
                }
            }
        }
    }
}
