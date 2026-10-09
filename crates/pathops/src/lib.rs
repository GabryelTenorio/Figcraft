//! FigCraft path operations.
//!
//! - [`boolean`]: Union, Subtract, Intersect and Exclude over any number of closed paths (robust
//!   sweep-line topology from `linesweeper`); results use the even-odd rule.
//! - [`stroke_outline`]: a stroke as a fill, with Figma's Inside / Center / Outside alignment,
//!   dashes, caps, joins and arrowheads.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use figcraft_doc::{BoolOp, StrokeAlign, StrokeCap, StrokeJoin, StrokeProps};
use figcraft_geom::kurbo::{self, BezPath, Cap, Join, PathEl, PathSeg, Point, Shape, Vec2};

#[derive(Debug, PartialEq)]
pub enum PathOpsError {
    /// The inputs contain NaN or infinite coordinates.
    BadInput,
}

impl std::fmt::Display for PathOpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "path contains invalid coordinates")
    }
}

/// Close every open subpath (a filled open path paints as if closed).
pub fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if open {
                    out.close_path();
                }
                open = true;
            }
            PathEl::ClosePath => open = false,
            _ => {}
        }
        out.push(*el);
    }
    if open {
        out.close_path();
    }
    out
}

/// Every coordinate is finite and of a sane magnitude (the sweep needs bounded input).
pub fn finite(path: &BezPath) -> bool {
    let ok = |p: &Point| p.x.is_finite() && p.y.is_finite() && p.x.abs() < 1e9 && p.y.abs() < 1e9;
    path.elements().iter().all(|el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => ok(p),
        PathEl::QuadTo(a, b) => ok(a) && ok(b),
        PathEl::CurveTo(a, b, c) => ok(a) && ok(b) && ok(c),
        PathEl::ClosePath => true,
    })
}

fn binary(a: &BezPath, b: &BezPath, op: linesweeper::BinaryOp) -> Result<BezPath, PathOpsError> {
    if a.elements().is_empty() && b.elements().is_empty() {
        return Ok(BezPath::new());
    }
    if !finite(a) || !finite(b) {
        return Err(PathOpsError::BadInput);
    }
    let contours = linesweeper::binary_op(&closed(a), &closed(b), linesweeper::FillRule::NonZero, op).map_err(|_| PathOpsError::BadInput)?;
    let mut out = BezPath::new();
    for c in contours.contours() {
        out.extend(c.path.elements().iter().copied());
    }
    Ok(out)
}

/// Normalise a single path's winding (non-zero) into simple even-odd contours.
pub fn simplify(path: &BezPath) -> Result<BezPath, PathOpsError> {
    binary(path, &BezPath::new(), linesweeper::BinaryOp::Union)
}

/// Apply a boolean operation to `paths` in order (bottom to top, like a boolean group's children):
/// Subtract removes every later shape from the first one.
pub fn boolean(paths: &[BezPath], op: BoolOp) -> Result<BezPath, PathOpsError> {
    let mut it = paths.iter();
    let Some(first) = it.next() else { return Ok(BezPath::new()) };
    let mut acc = first.clone();
    match op {
        BoolOp::Union => {
            for p in it {
                acc = binary(&acc, p, linesweeper::BinaryOp::Union)?;
            }
        }
        BoolOp::Intersect => {
            for p in it {
                acc = binary(&acc, p, linesweeper::BinaryOp::Intersection)?;
            }
        }
        BoolOp::Exclude => {
            for p in it {
                acc = binary(&acc, p, linesweeper::BinaryOp::Xor)?;
            }
        }
        BoolOp::Subtract => {
            let rest: Vec<BezPath> = it.cloned().collect();
            if rest.is_empty() {
                return Ok(acc);
            }
            let mut cut = BezPath::new();
            for p in &rest {
                cut = binary(&cut, p, linesweeper::BinaryOp::Union)?;
            }
            acc = binary(&acc, &cut, linesweeper::BinaryOp::Difference)?;
        }
    }
    if paths.len() == 1 {
        return simplify(&acc);
    }
    Ok(acc)
}

fn kcap(c: StrokeCap) -> Cap {
    match c {
        StrokeCap::Round | StrokeCap::CircleFilled => Cap::Round,
        StrokeCap::Square | StrokeCap::DiamondFilled => Cap::Square,
        _ => Cap::Butt,
    }
}

fn kjoin(j: StrokeJoin) -> Join {
    match j {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Bevel => Join::Bevel,
        StrokeJoin::Round => Join::Round,
    }
}

/// Plain centred stroke of `path` at `width`.
pub fn stroke_center(path: &BezPath, width: f64, props: &StrokeProps) -> BezPath {
    if !(width.is_finite() && width > 0.0 && finite(path)) {
        return BezPath::new();
    }
    let mut style = kurbo::Stroke::new(width)
        .with_join(kjoin(props.join))
        .with_miter_limit(props.miter_limit.clamp(1.0, 100.0))
        .with_start_cap(kcap(props.cap_start))
        .with_end_cap(kcap(props.cap_end));
    let dashes: Vec<f64> = props.dashes.iter().copied().filter(|d| d.is_finite() && *d >= 0.0).collect();
    if !dashes.is_empty() && dashes.iter().sum::<f64>() > 0.01 {
        let pat = if dashes.len() % 2 == 1 { [dashes.clone(), dashes].concat() } else { dashes };
        style = style.with_dashes(0.0, pat);
    }
    kurbo::stroke(path.iter(), &style, &kurbo::StrokeOpts::default(), 0.05)
}

/// The painted stroke area of a shape as a fill path (local coordinates). `closed` says whether the
/// shape is closed (Inside/Outside only apply to closed shapes).
pub fn stroke_outline(path: &BezPath, props: &StrokeProps, closed_shape: bool) -> BezPath {
    let w = props.weight;
    if !(w.is_finite() && w > 0.0) {
        return BezPath::new();
    }
    let align = if closed_shape { props.align } else { StrokeAlign::Center };
    let mut out = match align {
        StrokeAlign::Center => stroke_center(path, w, props),
        StrokeAlign::Inside => {
            let s = stroke_center(path, w * 2.0, props);
            binary(&s, path, linesweeper::BinaryOp::Intersection).unwrap_or(s)
        }
        StrokeAlign::Outside => {
            let s = stroke_center(path, w * 2.0, props);
            binary(&s, path, linesweeper::BinaryOp::Difference).unwrap_or(s)
        }
    };
    if !closed_shape {
        for (at_end, cap) in [(false, props.cap_start), (true, props.cap_end)] {
            if cap.is_arrow()
                && let Some(head) = arrowhead(path, cap, w, at_end)
            {
                out.extend(head.elements().iter().copied());
            }
        }
    }
    out
}

/// End point and outward direction of an open path at its start or end.
fn path_end(path: &BezPath, at_end: bool) -> Option<(Point, Vec2)> {
    let segs: Vec<PathSeg> = path.segments().collect();
    let seg = if at_end { segs.last()? } else { segs.first()? };
    use kurbo::ParamCurve;
    use kurbo::ParamCurveDeriv;
    let (p, d) = if at_end {
        let d = match seg {
            PathSeg::Line(l) => l.p1 - l.p0,
            PathSeg::Quad(q) => q.deriv().eval(1.0).to_vec2(),
            PathSeg::Cubic(c) => {
                let d = c.deriv().eval(1.0).to_vec2();
                if d.hypot() < 1e-9 { c.p3 - c.p2 } else { d }
            }
        };
        (seg.end(), d)
    } else {
        let d = match seg {
            PathSeg::Line(l) => l.p0 - l.p1,
            PathSeg::Quad(q) => -q.deriv().eval(0.0).to_vec2(),
            PathSeg::Cubic(c) => {
                let d = -c.deriv().eval(0.0).to_vec2();
                if d.hypot() < 1e-9 { c.p0 - c.p1 } else { d }
            }
        };
        (seg.start(), d)
    };
    let len = d.hypot();
    if len < 1e-12 {
        return None;
    }
    Some((p, d / len))
}

/// The filled arrowhead shape for `cap` at one end of an open path.
pub fn arrowhead(path: &BezPath, cap: StrokeCap, weight: f64, at_end: bool) -> Option<BezPath> {
    let (tip, dir) = path_end(path, at_end)?;
    let n = Vec2::new(-dir.y, dir.x);
    let s = (weight * 3.0).max(6.0);
    let mut p = BezPath::new();
    match cap {
        StrokeCap::TriangleArrow => {
            p.move_to(tip + dir * (weight * 0.5));
            p.line_to(tip - dir * s + n * (s * 0.6));
            p.line_to(tip - dir * s - n * (s * 0.6));
            p.close_path();
        }
        StrokeCap::ReversedTriangle => {
            p.move_to(tip - dir * s);
            p.line_to(tip + n * (s * 0.6));
            p.line_to(tip - n * (s * 0.6));
            p.close_path();
        }
        StrokeCap::LineArrow => {
            let mut l = BezPath::new();
            l.move_to(tip - dir * s + n * (s * 0.7));
            l.line_to(tip);
            l.line_to(tip - dir * s - n * (s * 0.7));
            let props =
                StrokeProps { weight, cap_start: StrokeCap::Round, cap_end: StrokeCap::Round, join: StrokeJoin::Round, ..StrokeProps::default() };
            return Some(stroke_center(&l, weight, &props));
        }
        StrokeCap::CircleFilled => {
            p = kurbo::Circle::new(tip, s * 0.4).to_path(0.05);
        }
        StrokeCap::DiamondFilled => {
            let r = s * 0.5;
            p.move_to(tip + dir * r);
            p.line_to(tip + n * r);
            p.line_to(tip - dir * r);
            p.line_to(tip - n * r);
            p.close_path();
        }
        _ => return None,
    }
    Some(p)
}

/// Per-side stroke of a rectangle `[top, right, bottom, left]`, painted inside/center/outside.
pub fn rect_side_strokes(w: f64, h: f64, sides: [f64; 4], align: StrokeAlign) -> BezPath {
    let [t, r, b, l] = sides.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let f = match align {
        StrokeAlign::Inside => 0.0,
        StrokeAlign::Center => 0.5,
        StrokeAlign::Outside => 1.0,
    };
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| figcraft_geom::Rect::new(x0, y0, x1, y1).to_path(0.1);
    let mut p = BezPath::new();
    // Outer box edges.
    let (ox0, oy0, ox1, oy1) = (-l * f, -t * f, w + r * f, h + b * f);
    if t > 0.0 {
        p.extend(rect(ox0, oy0, ox1, oy0 + t).elements().iter().copied());
    }
    if b > 0.0 {
        p.extend(rect(ox0, oy1 - b, ox1, oy1).elements().iter().copied());
    }
    if l > 0.0 {
        p.extend(rect(ox0, oy0 + t, ox0 + l, oy1 - b).elements().iter().copied());
    }
    if r > 0.0 {
        p.extend(rect(ox1 - r, oy0 + t, ox1, oy1 - b).elements().iter().copied());
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_geom::Rect;

    fn sq(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.1)
    }

    fn area_eo(p: &BezPath) -> f64 {
        // Sum of absolute contour areas with even-odd nesting approximated by signed area sum.
        p.area().abs()
    }

    #[test]
    fn union_intersect_subtract_exclude() {
        let a = sq(0.0, 0.0, 10.0);
        let b = sq(5.0, 5.0, 10.0);
        let u = boolean(&[a.clone(), b.clone()], BoolOp::Union).unwrap();
        assert!((area_eo(&u) - 175.0).abs() < 0.5, "{}", area_eo(&u));
        let i = boolean(&[a.clone(), b.clone()], BoolOp::Intersect).unwrap();
        assert!((area_eo(&i) - 25.0).abs() < 0.5);
        let s = boolean(&[a.clone(), b.clone()], BoolOp::Subtract).unwrap();
        assert!((area_eo(&s) - 75.0).abs() < 0.5);
        let x = boolean(&[a, b], BoolOp::Exclude).unwrap();
        let bb = x.bounding_box();
        assert_eq!((bb.x0, bb.y0, bb.x1, bb.y1), (0.0, 0.0, 15.0, 15.0));
    }

    #[test]
    fn hostile_inputs() {
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, 1.0));
        assert!(boolean(&[bad, sq(0.0, 0.0, 1.0)], BoolOp::Union).is_err());
        assert!(boolean(&[], BoolOp::Union).unwrap().elements().is_empty());
        let _ = stroke_outline(&sq(0.0, 0.0, 10.0), &StrokeProps { weight: f64::INFINITY, ..Default::default() }, true);
    }

    #[test]
    fn stroke_alignment_bounds() {
        let r = sq(0.0, 0.0, 100.0);
        let props = StrokeProps { weight: 10.0, align: StrokeAlign::Inside, ..Default::default() };
        let inside = stroke_outline(&r, &props, true);
        let b = inside.bounding_box();
        assert!(b.x0 >= -0.01 && b.x1 <= 100.01, "{b:?}");
        let outside = stroke_outline(&r, &StrokeProps { align: StrokeAlign::Outside, ..props.clone() }, true);
        let b = outside.bounding_box();
        assert!((b.x0 + 10.0).abs() < 0.01 && (b.x1 - 110.0).abs() < 0.01, "{b:?}");
        let center = stroke_outline(&r, &StrokeProps { align: StrokeAlign::Center, ..props }, true);
        let b = center.bounding_box();
        assert!((b.x0 + 5.0).abs() < 0.01);
    }

    #[test]
    fn arrows_on_open_paths() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let props = StrokeProps { weight: 2.0, cap_end: StrokeCap::TriangleArrow, ..Default::default() };
        let s = stroke_outline(&line, &props, false);
        assert!(s.bounding_box().height() > 4.0);
        assert!(arrowhead(&BezPath::new(), StrokeCap::LineArrow, 1.0, true).is_none());
    }

    #[test]
    fn dashes() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let props = StrokeProps { weight: 2.0, dashes: vec![10.0, 10.0], align: StrokeAlign::Center, ..Default::default() };
        let s = stroke_center(&line, 2.0, &props);
        assert!((s.area().abs() - 100.0).abs() < 2.0, "{}", s.area());
    }
}
