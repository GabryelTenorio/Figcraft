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
    let vertical = pn.layout.as_ref()?.mode == LayoutMode::Vertical;
    let kids: Vec<NodeId> = pn.children.iter().copied().filter(|c| !exclude.contains(c)).collect();
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

/// Where a drop at page point `p` lands.
pub fn slot(doc: &Document, page: NodeId, p: Point) -> DropSlot {
    let parent = frame_at(doc, page, p, &[]).unwrap_or(page);
    match auto_layout_slot(doc, parent, p, &[]) {
        Some((index, line)) => DropSlot { parent, index: Some(index), line },
        None => DropSlot { parent, index: None, line: None },
    }
}
