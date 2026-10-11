//! Paths measured by length: subpaths flattened into polylines with the length along them, for
//! anything placed along a path (variable width strokes, text on a path).

use kurbo::{BezPath, PathEl, Point, Vec2};

/// Flattening tolerance (local units).
pub const FLATTEN_TOL: f64 = 0.05;
/// The most flattened points a path is measured with; longer paths are refused.
pub const MAX_SAMPLES: usize = 40_000;

/// Every coordinate is finite and of a sane magnitude.
pub fn finite_path(path: &BezPath) -> bool {
    let ok = |p: &Point| p.x.is_finite() && p.y.is_finite() && p.x.abs() < 1e9 && p.y.abs() < 1e9;
    path.elements().iter().all(|el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => ok(p),
        PathEl::QuadTo(a, b) => ok(a) && ok(b),
        PathEl::CurveTo(a, b, c) => ok(a) && ok(b) && ok(c),
        PathEl::ClosePath => true,
    })
}

/// One subpath flattened, with the length along it at each point. A closed subpath ends on its
/// start point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    pub points: Vec<Point>,
    /// Cumulative length at each point (starts at 0).
    pub lengths: Vec<f64>,
    pub closed: bool,
}

impl Polyline {
    pub fn length(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    fn push(&mut self, p: Point) {
        let Some(last) = self.points.last().copied() else {
            self.points.push(p);
            self.lengths.push(0.0);
            return;
        };
        let d = (p - last).hypot();
        if d > 1e-9 {
            self.points.push(p);
            self.lengths.push(self.length() + d);
        }
    }

    /// The point at `t` (0..1 by length) and the unit normal there (the direction turned 90°).
    pub fn at(&self, t: f64) -> Option<(Point, Vec2)> {
        let len = self.length();
        if len <= 1e-9 || !t.is_finite() {
            return None;
        }
        let s = t.clamp(0.0, 1.0) * len;
        let i = self.lengths.partition_point(|l| *l <= s).clamp(1, self.points.len().saturating_sub(1).max(1));
        let (a, b) = (*self.points.get(i - 1)?, *self.points.get(i)?);
        let (la, lb) = (*self.lengths.get(i - 1)?, *self.lengths.get(i)?);
        let u = if lb - la > 1e-12 { ((s - la) / (lb - la)).clamp(0.0, 1.0) } else { 0.0 };
        let d = b - a;
        let n = d.turn_90() / d.hypot().max(1e-12);
        Some((a.lerp(b, u), n))
    }

    /// The nearest point to `p`: (t by length, distance).
    pub fn nearest(&self, p: Point) -> Option<(f64, f64)> {
        let len = self.length();
        if len <= 1e-9 {
            return None;
        }
        let mut best: Option<(f64, f64)> = None;
        for (i, w) in self.points.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let d = b - a;
            let l2 = d.hypot2();
            let u = if l2 > 1e-18 { ((p - a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let dist = (a.lerp(b, u) - p).hypot();
            if best.is_none_or(|(_, bd)| dist < bd) {
                let s = self.lengths.get(i).copied().unwrap_or(0.0) + u * l2.sqrt();
                best = Some((s / len, dist));
            }
        }
        best
    }
}

/// Every subpath of `path` flattened. `None` when the path has invalid coordinates or is too
/// long to work with.
pub fn polylines(path: &BezPath) -> Option<Vec<Polyline>> {
    if !finite_path(path) {
        return None;
    }
    fn finish(cur: &mut Polyline, out: &mut Vec<Polyline>) {
        let done = std::mem::take(cur);
        if done.points.len() >= 2 {
            out.push(done);
        }
    }
    let mut out: Vec<Polyline> = Vec::new();
    let mut cur = Polyline::default();
    let mut start = Point::ZERO;
    let mut count = 0usize;
    kurbo::flatten(path.iter(), FLATTEN_TOL, |el| {
        count += 1;
        match el {
            PathEl::MoveTo(p) => {
                finish(&mut cur, &mut out);
                start = p;
                cur.push(p);
            }
            PathEl::LineTo(p) => {
                if cur.points.is_empty() {
                    cur.push(start);
                }
                cur.push(p);
            }
            PathEl::ClosePath => {
                cur.push(start);
                cur.closed = cur.points.len() >= 3;
                finish(&mut cur, &mut out);
            }
            _ => {}
        }
    });
    finish(&mut cur, &mut out);
    (count <= MAX_SAMPLES).then_some(out)
}
