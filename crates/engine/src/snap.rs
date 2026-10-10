//! Snapping and smart guides: while moving or resizing, edges and centres snap to nearby layers'
//! edges/centres, the parent frame, layout grid columns and page guides; the matches come back as
//! guide lines (with spacing labels) for the canvas to draw in red. Vector points also snap to
//! other points (see `vectoredit::snap`); landing on one shows a marker (a zero-length guide).

use figcraft_doc::{Document, NodeId, NodeKind};
use figcraft_geom::{Point, Rect, Vec2};
use serde::Serialize;

/// A guide line to draw: from (x0, y0) to (x1, y1) in page coordinates, with an optional label
/// (a distance) at its middle; `dashed` marks a measurement's extension line.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Guide {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub label: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dashed: bool,
}

impl Guide {
    /// A plain line from `a` to `b` (a marker when they are the same point).
    pub fn new(a: Point, b: Point) -> Self {
        Self { x0: a.x, y0: a.y, x1: b.x, y1: b.y, label: None, dashed: false }
    }
}

/// A line to snap: its position on one axis and the span it covers on the other (x, y0, y1 for a
/// vertical line; y, x0, x1 for a horizontal one).
pub type Span = (f64, f64, f64);

/// Snap candidates gathered once at the start of a gesture.
#[derive(Clone, Debug, Default)]
pub struct Targets {
    /// Vertical lines (x) with the y-span of the rect they came from.
    pub xs: Vec<Span>,
    /// Horizontal lines (y) with the x-span.
    pub ys: Vec<Span>,
    /// Sibling boxes (for spacing labels).
    pub boxes: Vec<Rect>,
    /// Points to land on exactly (see [`Targets::add_point`]).
    pub points: Vec<Point>,
}

impl Targets {
    /// Gather snap lines from the siblings of `moving` (and the parent frame), page guides and
    /// layout grids, excluding the moving nodes and their descendants.
    pub fn gather(doc: &Document, page: NodeId, moving: &[NodeId]) -> Targets {
        let mut t = Targets::default();
        let push = |r: Rect, t: &mut Targets| {
            for x in [r.x0, r.center().x, r.x1] {
                t.xs.push((x, r.y0, r.y1));
            }
            for y in [r.y0, r.center().y, r.y1] {
                t.ys.push((y, r.x0, r.x1));
            }
            t.boxes.push(r);
        };
        let parent = moving.first().and_then(|m| doc.node(*m)).and_then(|n| n.parent).unwrap_or(page);
        // The parent frame's own box (and its layout grids).
        if parent != page
            && let Some(b) = doc.world_bounds(parent)
        {
            for x in [b.x0, b.center().x, b.x1] {
                t.xs.push((x, b.y0, b.y1));
            }
            for y in [b.y0, b.center().y, b.y1] {
                t.ys.push((y, b.x0, b.x1));
            }
            let (gx, gy) = figcraft_layout::grid_snap_lines(doc, parent);
            for x in gx {
                t.xs.push((x, b.y0, b.y1));
            }
            for y in gy {
                t.ys.push((y, b.x0, b.x1));
            }
        }
        // Siblings and, at the top level, all top-level layers.
        if let Some(p) = doc.node(parent) {
            for c in &p.children {
                if moving.contains(c) || !doc.node(*c).is_some_and(|n| n.visible) {
                    continue;
                }
                if let Some(b) = doc.world_bounds(*c) {
                    push(b, &mut t);
                }
            }
        }
        if let Some(NodeKind::Page { guides, .. }) = doc.node(page).map(|n| &n.kind) {
            for g in guides {
                if g.vertical {
                    t.xs.push((g.offset, -1e7, 1e7));
                } else {
                    t.ys.push((g.offset, -1e7, 1e7));
                }
            }
        }
        // The guides of the frames the moving layers are in, across each frame.
        let mut f = parent;
        for _ in 0..128 {
            let Some(n) = doc.node(f).filter(|_| f != page) else { break };
            if let Some(b) = doc.world_bounds(f).filter(|_| !n.guides.is_empty()) {
                let xf = doc.world_transform(f);
                for g in &n.guides {
                    if g.vertical {
                        t.xs.push(((xf * Point::new(g.offset, 0.0)).x, b.y0, b.y1));
                    } else {
                        t.ys.push(((xf * Point::new(0.0, g.offset)).y, b.x0, b.x1));
                    }
                }
            }
            let Some(up) = n.parent else { break };
            f = up;
        }
        t
    }

    /// A point to snap onto (a vector point, a segment midpoint): lines through it on both axes,
    /// and a marker when a moving point lands on it.
    pub fn add_point(&mut self, p: Point) {
        self.xs.push((p.x, p.y, p.y));
        self.ys.push((p.y, p.x, p.x));
        self.points.push(p);
    }

    /// Snap a moving box: returns the (dx, dy) correction and guides to draw.
    pub fn snap_rect(&self, r: Rect, threshold: f64) -> (f64, f64, Vec<Guide>) {
        let (xs, ys) = rect_spans(r);
        let (dx, dy, guides) = self.snap_spans(&xs, &ys, threshold);
        (dx.unwrap_or(0.0), dy.unwrap_or(0.0), guides)
    }

    /// Snap a single point (resize handles, drawing).
    pub fn snap_point(&self, x: f64, y: f64, threshold: f64) -> (f64, f64, Vec<Guide>) {
        let (dx, dy, guides) = self.snap_spans(&[(x, y, y)], &[(y, x, x)], threshold);
        (dx.unwrap_or(0.0), dy.unwrap_or(0.0), guides)
    }

    /// Snap several moving points together (vector points): the correction on each axis (`None`
    /// where nothing was in reach), the guides, and a marker (a zero-length guide) on every target
    /// point a moving point lands on.
    pub fn snap_points(&self, pts: &[Point], threshold: f64) -> (Option<f64>, Option<f64>, Vec<Guide>) {
        let xs: Vec<Span> = pts.iter().map(|p| (p.x, p.y, p.y)).collect();
        let ys: Vec<Span> = pts.iter().map(|p| (p.y, p.x, p.x)).collect();
        let (dx, dy, mut guides) = self.snap_spans(&xs, &ys, threshold);
        let d = Vec2::new(dx.unwrap_or(0.0), dy.unwrap_or(0.0));
        for p in pts.iter().map(|p| *p + d) {
            let marker = Guide::new(p, p);
            if self.points.iter().any(|t| (*t - p).hypot() < 1e-6) && !guides.contains(&marker) {
                guides.push(marker);
            }
        }
        (dx, dy, guides)
    }

    /// Snap moving lines: `xs` are vertical ones (x with the y-span they cover), `ys` horizontal
    /// ones. Returns the nearest correction on each axis within `threshold` and the guides for the
    /// lines that coincide with targets afterwards.
    pub fn snap_spans(&self, xs: &[Span], ys: &[Span], threshold: f64) -> (Option<f64>, Option<f64>, Vec<Guide>) {
        let best = |cands: &[Span], vals: &[Span]| -> Option<f64> {
            let mut best: Option<f64> = None;
            for (line, _, _) in cands {
                for (v, _, _) in vals {
                    let d = line - v;
                    if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.abs()) {
                        best = Some(d);
                    }
                }
            }
            best
        };
        let (dx, dy) = (best(&self.xs, xs), best(&self.ys, ys));
        let (ox, oy) = (dx.unwrap_or(0.0), dy.unwrap_or(0.0));
        let xs: Vec<Span> = xs.iter().map(|(x, a, b)| (x + ox, a + oy, b + oy)).collect();
        let ys: Vec<Span> = ys.iter().map(|(y, a, b)| (y + oy, a + ox, b + ox)).collect();
        (dx, dy, self.guides_spans(&xs, &ys))
    }

    /// Guide lines for edges of `r` that coincide with targets.
    pub fn guides_for(&self, r: Rect) -> Vec<Guide> {
        let (xs, ys) = rect_spans(r);
        self.guides_spans(&xs, &ys)
    }

    /// Guides for moving lines (as in [`Self::snap_spans`]) that coincide with targets, each
    /// spanning the line and the targets on it.
    fn guides_spans(&self, xs: &[Span], ys: &[Span]) -> Vec<Guide> {
        let span = |cands: &[Span], (v, a, b): Span| -> Option<(f64, f64)> {
            let hits: Vec<&Span> = cands.iter().filter(|(l, _, _)| (l - v).abs() < 1e-6).collect();
            (!hits.is_empty()).then(|| (hits.iter().map(|h| h.1).fold(a, f64::min).max(-1e6), hits.iter().map(|h| h.2).fold(b, f64::max).min(1e6)))
        };
        let mut out = Vec::new();
        for x in xs {
            if let Some((y0, y1)) = span(&self.xs, *x) {
                out.push(Guide::new(Point::new(x.0, y0), Point::new(x.0, y1)));
            }
        }
        for y in ys {
            if let Some((x0, x1)) = span(&self.ys, *y) {
                out.push(Guide::new(Point::new(x0, y.0), Point::new(x1, y.0)));
            }
        }
        out
    }
}

/// A box's edges and centre lines as moving lines (see [`Targets::snap_spans`]).
fn rect_spans(r: Rect) -> ([Span; 3], [Span; 3]) {
    let c = r.center();
    ([(r.x0, r.y0, r.y1), (c.x, r.y0, r.y1), (r.x1, r.y0, r.y1)], [(r.y0, r.x0, r.x1), (c.y, r.x0, r.x1), (r.y1, r.x0, r.x1)])
}
