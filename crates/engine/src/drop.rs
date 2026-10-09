//! Where something dropped at a page point lands: the frame under the point (or the page) and, in
//! an auto layout frame, the child slot by pointer position plus the insertion line to draw.

use figcraft_doc::*;
use figcraft_geom::Point;
use serde::Serialize;

use crate::cmd::create::frame_at;

/// A drop destination.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DropSlot {
    pub parent: NodeId,
    /// Child index in an auto layout parent (None: on top).
    pub index: Option<usize>,
    /// Insertion indicator (x0, y0, x1, y1) for auto layout parents.
    pub line: Option<[f64; 4]>,
}

/// The slot in auto layout frame `parent` for pointer `p`, ignoring `exclude` (layers being
/// moved): the index before the first child whose centre is past the pointer, and the line to
/// draw there.
pub fn auto_layout_slot(doc: &Document, parent: NodeId, p: Point, exclude: &[NodeId]) -> Option<(usize, Option<[f64; 4]>)> {
    let pn = doc.node(parent)?;
    let layout = pn.layout.as_ref()?;
    let vertical = layout.mode == LayoutMode::Vertical;
    let kids: Vec<NodeId> = pn.children.iter().copied().filter(|c| !exclude.contains(c)).collect();
    if layout.wrap && !vertical && layout.mode != LayoutMode::Grid {
        return Some(wrap_slot(doc, &kids, p));
    }
    let pos = if vertical { p.y } else { p.x };
    let idx = kids
        .iter()
        .position(|c| doc.world_bounds(*c).is_some_and(|b| pos < if vertical { b.center().y } else { b.center().x }))
        .unwrap_or(kids.len());
    let line = kids
        .get(idx)
        .and_then(|c| doc.world_bounds(*c))
        .map(|b| if vertical { [b.x0, b.y0 - 2.0, b.x1, b.y0 - 2.0] } else { [b.x0 - 2.0, b.y0, b.x0 - 2.0, b.y1] })
        .or_else(|| {
            kids.last()
                .and_then(|c| doc.world_bounds(*c))
                .map(|b| if vertical { [b.x0, b.y1 + 2.0, b.x1, b.y1 + 2.0] } else { [b.x1 + 2.0, b.y0, b.x1 + 2.0, b.y1] })
        });
    Some((idx, line))
}

/// The slot in a wrapping row layout: the visual row under the pointer (children wrap in flow
/// order, so a new row starts where a child sits left of the one before), then the position in it.
fn wrap_slot(doc: &Document, kids: &[NodeId], p: Point) -> (usize, Option<[f64; 4]>) {
    let bounds: Vec<(usize, figcraft_geom::Rect)> = kids.iter().enumerate().filter_map(|(i, c)| doc.world_bounds(*c).map(|b| (i, b))).collect();
    let mut rows: Vec<Vec<(usize, figcraft_geom::Rect)>> = Vec::new();
    for (i, b) in bounds {
        match rows.last_mut() {
            Some(row) if row.last().is_some_and(|(_, prev)| b.x0 >= prev.x0 - 0.5) => row.push((i, b)),
            _ => rows.push(vec![(i, b)]),
        }
    }
    let span =
        |row: &[(usize, figcraft_geom::Rect)]| row.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (_, b)| (lo.min(b.y0), hi.max(b.y1)));
    // The row containing the pointer, else the nearest one.
    let Some(row) = rows.iter().min_by(|a, b| {
        let d = |r: &[(usize, figcraft_geom::Rect)]| {
            let (lo, hi) = span(r);
            if p.y < lo {
                lo - p.y
            } else if p.y > hi {
                p.y - hi
            } else {
                0.0
            }
        };
        d(a).total_cmp(&d(b))
    }) else {
        return (0, None);
    };
    let (y0, y1) = span(row);
    match row.iter().find(|(_, b)| p.x < b.center().x) {
        Some((i, b)) => (*i, Some([b.x0 - 2.0, y0, b.x0 - 2.0, y1])),
        None => match row.last() {
            Some((i, b)) => (i + 1, Some([b.x1 + 2.0, y0, b.x1 + 2.0, y1])),
            None => (0, None),
        },
    }
}

/// The grid cell of auto layout grid `parent` under page point `p` (the nearest one): (column,
/// row, the cell's page-space bounds).
pub fn grid_cell_at(doc: &Document, parent: NodeId, p: Point) -> Option<(u32, u32, [f64; 4])> {
    let g = figcraft_layout::grid_geometry(doc, parent)?;
    let lp = doc.world_transform(parent).inverse() * p;
    let nearest = |tracks: &[(f64, f64)], v: f64| {
        tracks
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let d = |t: &(f64, f64)| {
                    if v < t.0 {
                        t.0 - v
                    } else if v > t.1 {
                        v - t.1
                    } else {
                        0.0
                    }
                };
                d(a).total_cmp(&d(b))
            })
            .map(|(i, t)| (i, *t))
    };
    let (ci, (cx0, cx1)) = nearest(&g.columns, lp.x)?;
    let (ri, (ry0, ry1)) = nearest(&g.rows, lp.y)?;
    let r = figcraft_geom::transform_rect_bbox(doc.world_transform(parent), figcraft_geom::Rect::new(cx0, ry0, cx1, ry1));
    Some((u32::try_from(ci).ok()?, u32::try_from(ri).ok()?, [r.x0, r.y0, r.x1, r.y1]))
}

/// Where a drop at page point `p` lands.
pub fn slot(doc: &Document, page: NodeId, p: Point) -> DropSlot {
    let parent = frame_at(doc, page, p, &[]).unwrap_or(page);
    match auto_layout_slot(doc, parent, p, &[]) {
        Some((index, line)) => DropSlot { parent, index: Some(index), line },
        None => DropSlot { parent, index: None, line: None },
    }
}
