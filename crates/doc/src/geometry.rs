//! Node geometry: local outline paths, and hit testing with Figma's click-target rules.

use crate::{Document, Node, NodeId, NodeKind};
use figcraft_geom::{BezPath, Point, Rect, Shape};

/// The node's outline in its local space (`None` for groups, booleans, text and pages, whose
/// geometry comes from children or glyphs).
pub fn shape_path(n: &Node) -> Option<BezPath> {
    let (w, h) = (n.width, n.height);
    let c = &n.corners;
    Some(match &n.kind {
        NodeKind::Rectangle
        | NodeKind::Frame
        | NodeKind::Component { .. }
        | NodeKind::ComponentSet { .. }
        | NodeKind::Instance { .. }
        | NodeKind::Section { .. } => figcraft_geom::rounded_rect(w, h, c.radii, c.smoothing),
        NodeKind::Slice => figcraft_geom::rounded_rect(w, h, [0.0; 4], 0.0),
        NodeKind::Ellipse { start, sweep, inner } => figcraft_geom::ellipse(w, h, *start, *sweep, *inner),
        NodeKind::Polygon { count } => figcraft_geom::rounded_polygon(&figcraft_geom::polygon_points(w, h, *count), c.radii[0]),
        NodeKind::Star { count, inner_ratio } => figcraft_geom::rounded_polygon(&figcraft_geom::star_points(w, h, *count, *inner_ratio), c.radii[0]),
        NodeKind::Line => {
            let mut p = BezPath::new();
            p.move_to((0.0, 0.0));
            p.line_to((w, 0.0));
            p
        }
        NodeKind::Vector { network } => {
            let f = network.fill_path();
            if f.elements().is_empty() { network.stroke_path() } else { f }
        }
        NodeKind::Text(_) => figcraft_geom::rounded_rect(w, h, [0.0; 4], 0.0),
        NodeKind::Page { .. } | NodeKind::Group | NodeKind::BooleanOp { .. } => return None,
    })
}

/// The open/closed path strokes follow (vector networks stroke every segment, filled or not).
pub fn stroke_geometry(n: &Node) -> Option<BezPath> {
    match &n.kind {
        NodeKind::Vector { network } => Some(network.stroke_path()),
        _ => shape_path(n),
    }
}

/// Whether the shape's path is closed (fills apply).
pub fn is_closed(n: &Node) -> bool {
    match &n.kind {
        NodeKind::Line => false,
        NodeKind::Vector { network } => !network.regions.is_empty(),
        _ => true,
    }
}

fn hits_node_itself(doc: &Document, id: NodeId, n: &Node, pt: Point, tol: f64) -> bool {
    let inv = doc.world_transform(id).inverse();
    let lp = inv * pt;
    // Tolerance in local units (approximate under scale).
    let tol_l = tol;
    match &n.kind {
        NodeKind::Group | NodeKind::BooleanOp { .. } | NodeKind::Page { .. } => false,
        NodeKind::Text(_) | NodeKind::Slice => n.local_rect().inflate(tol_l, tol_l).contains(lp),
        NodeKind::Line => {
            let half = (n.stroke.weight / 2.0).max(tol_l);
            lp.x >= -half && lp.x <= n.width + half && lp.y.abs() <= half
        }
        NodeKind::Frame | NodeKind::Component { .. } | NodeKind::ComponentSet { .. } | NodeKind::Instance { .. } | NodeKind::Section { .. } => {
            n.local_rect().inflate(tol_l, tol_l).contains(lp)
        }
        _ => {
            let Some(path) = shape_path(n) else { return false };
            if is_closed(n) && (n.fills.iter().any(|f| f.visible) || !n.has_visible_stroke()) && path.winding(lp) != 0 {
                return true;
            }
            // Near the outline.
            let reach = (n.stroke.max_weight() / 2.0).max(tol_l);
            if !path.bounding_box().inflate(reach, reach).contains(lp) {
                return false;
            }
            near_path(&path, lp, reach)
        }
    }
}

fn near_path(path: &BezPath, p: Point, dist: f64) -> bool {
    use figcraft_geom::kurbo::ParamCurveNearest;
    path.segments().any(|s| s.nearest(p, 0.01).distance_sq <= dist * dist)
}

/// The innermost visible, unlocked node under `pt` (page coordinates), honouring clip content.
pub fn hit_leaf(doc: &Document, page: NodeId, pt: Point, tol: f64) -> Option<NodeId> {
    let p = doc.node(page)?;
    for c in p.children.iter().rev() {
        if let Some(h) = hit_rec(doc, *c, pt, tol, 0) {
            return Some(h);
        }
    }
    None
}

/// Like [`hit_leaf`], but only within layer `root` (it included).
pub fn hit_in(doc: &Document, root: NodeId, pt: Point, tol: f64) -> Option<NodeId> {
    hit_rec(doc, root, pt, tol, 0)
}

fn hit_rec(doc: &Document, id: NodeId, pt: Point, tol: f64, depth: usize) -> Option<NodeId> {
    if depth > 512 {
        return None;
    }
    let n = doc.node(id)?;
    if !n.visible || n.locked {
        return None;
    }
    if let NodeKind::Section { contents_hidden: true } = n.kind {
        return hits_node_itself(doc, id, n, pt, tol).then_some(id);
    }
    let clips = n.clip && n.kind.is_frame_like();
    if clips && !hits_node_itself(doc, id, n, pt, 0.0) {
        return None;
    }
    // A boolean group is hit where an operand is (an approximation of its result). The leaf is the
    // operand, so a plain click still selects the group (see `select_target`) while a deep click,
    // a double click or a selected sibling operand reaches the operand itself.
    for c in n.children.iter().rev() {
        if let Some(h) = hit_rec(doc, *c, pt, tol, depth + 1) {
            return Some(h);
        }
    }
    if hits_node_itself(doc, id, n, pt, tol) {
        return Some(id);
    }
    None
}

/// Every visible, unlocked layer under `pt` (page coordinates) with its depth below the page, in
/// layer-tree order (each layer before what it contains, the front-most first): what
/// "Select layer" in the context menu lists. Clip content is honoured; boolean groups and
/// sections with hidden contents count as one layer.
pub fn layers_at(doc: &Document, page: NodeId, pt: Point, tol: f64) -> Vec<(NodeId, usize)> {
    fn rec(doc: &Document, id: NodeId, pt: Point, tol: f64, depth: usize, out: &mut Vec<(NodeId, usize)>) {
        let Some(n) = doc.node(id) else { return };
        if !n.visible || n.locked || depth > 512 {
            return;
        }
        let clips = n.clip && n.kind.is_frame_like();
        if clips && !hits_node_itself(doc, id, n, pt, 0.0) {
            return;
        }
        let at = out.len();
        let whole = matches!(n.kind, NodeKind::BooleanOp { .. } | NodeKind::Section { contents_hidden: true });
        if whole {
            if hit_rec(doc, id, pt, tol, depth).is_some() {
                out.push((id, depth));
            }
            return;
        }
        for c in n.children.iter().rev() {
            rec(doc, *c, pt, tol, depth + 1, out);
        }
        if out.len() > at || hits_node_itself(doc, id, n, pt, tol) {
            out.insert(at, (id, depth));
        }
    }
    let mut out = Vec::new();
    if let Some(p) = doc.node(page) {
        for c in p.children.iter().rev() {
            rec(doc, *c, pt, tol, 0, &mut out);
        }
    }
    out
}

/// Resolve a clicked leaf to what a plain click selects.
///
/// Figma's rules: top-level frames behave like artboards, so a click selects their direct child;
/// groups select as a whole; when something is already selected, its siblings (and their
/// contents) resolve relative to that context, which is how double-click "enters" groups.
/// `deep` (Cmd-click) selects the leaf itself.
pub fn select_target(doc: &Document, leaf: NodeId, context: &[NodeId], deep: bool) -> NodeId {
    if deep {
        return leaf;
    }
    // Chain from the page's child down to the leaf.
    let mut chain: Vec<NodeId> = doc.ancestors(leaf);
    chain.reverse();
    chain.push(leaf);
    // chain[0] is the page.
    let chain: Vec<NodeId> = chain.into_iter().skip(1).collect();
    let Some(top) = chain.first().copied() else { return leaf };
    // Context: if a selected node's parent is in the chain, select that parent's child in the chain.
    let mut best: Option<usize> = None;
    for s in context {
        let Some(parent) = doc.node(*s).and_then(|n| n.parent) else { continue };
        if let Some(pos) = chain.iter().position(|c| *c == parent)
            && pos + 1 < chain.len()
        {
            best = Some(best.map_or(pos + 1, |b: usize| b.max(pos + 1)));
        }
    }
    if let Some(i) = best
        && let Some(id) = chain.get(i)
    {
        return *id;
    }
    let top_is_board = doc.node(top).is_some_and(|n| n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. }));
    if top_is_board && chain.len() > 1 {
        // Sections contain frames: the frame inside a section is the board.
        let is_section = |i: usize| chain.get(i).and_then(|c| doc.node(*c)).is_some_and(|n| matches!(n.kind, NodeKind::Section { .. }));
        let is_board =
            |i: usize| chain.get(i).and_then(|c| doc.node(*c)).is_some_and(|n| n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. }));
        let mut i = 1;
        while is_section(i - 1) && is_board(i) && i + 1 < chain.len() {
            i += 1;
        }
        return chain.get(i).copied().unwrap_or(leaf);
    }
    top
}

/// What a double click on `leaf` selects: one level deeper than the selection, i.e. the layer
/// holding `leaf` just inside the deepest selected layer that holds it (entering groups, frames
/// and boolean groups), or the plain click target when no selected layer holds it.
pub fn enter_target(doc: &Document, leaf: NodeId, selection: &[NodeId]) -> NodeId {
    let mut chain: Vec<NodeId> = doc.ancestors(leaf);
    chain.reverse();
    chain.push(leaf);
    let inner = chain.iter().rposition(|c| selection.contains(c)).and_then(|i| chain.get(i + 1)).copied();
    inner.unwrap_or_else(|| select_target(doc, leaf, selection, false))
}

/// Nodes on `page` whose world bounds intersect `rect` (marquee): top-level children, or the
/// children of top-level frames when the marquee starts inside a frame (`within`).
pub fn marquee(doc: &Document, page: NodeId, rect: Rect, within: Option<NodeId>) -> Vec<NodeId> {
    let parent = within.unwrap_or(page);
    doc.node(parent).map(|p| marquee_children(doc, p, rect)).unwrap_or_default()
}

/// Nodes on `page` a Cmd/Ctrl marquee selects: the innermost layers whose bounds touch `rect`
/// (frames, groups and instances are looked into; a container none of whose children are touched
/// counts itself; boolean groups count as one shape).
pub fn marquee_deep(doc: &Document, page: NodeId, rect: Rect) -> Vec<NodeId> {
    fn walk(doc: &Document, parent: NodeId, rect: Rect, depth: usize, out: &mut Vec<NodeId>) {
        let Some(p) = doc.node(parent) else { return };
        for c in marquee_children(doc, p, rect) {
            let before = out.len();
            let opens = doc.node(c).is_some_and(|n| !n.children.is_empty() && !matches!(n.kind, NodeKind::BooleanOp { .. }));
            if opens && depth < 256 {
                walk(doc, c, rect, depth + 1, out);
            }
            if out.len() == before {
                out.push(c);
            }
        }
    }
    let mut out = Vec::new();
    walk(doc, page, rect, 0, &mut out);
    out
}

/// The visible, unlocked children of `p` whose world bounds touch `rect` (on both axes: lines and
/// other flat layers too).
fn marquee_children(doc: &Document, p: &Node, rect: Rect) -> Vec<NodeId> {
    p.children
        .iter()
        .copied()
        .filter(|c| doc.node(*c).is_some_and(|n| n.visible && !n.locked))
        .filter(|c| doc.world_bounds(*c).is_some_and(|b| b.x0 <= rect.x1 && b.x1 >= rect.x0 && b.y0 <= rect.y1 && b.y1 >= rect.y0))
        .collect()
}

/// Figma marquee rule: a marquee that fully contains a top-level frame selects the frame; one
/// starting on empty canvas selects top-level objects it touches.
pub fn marquee_select(doc: &Document, page: NodeId, rect: Rect, start_inside: Option<NodeId>) -> Vec<NodeId> {
    if let Some(frame) = start_inside {
        let inside = marquee(doc, page, rect, Some(frame));
        return inside;
    }
    marquee(doc, page, rect, None)
}

/// Bounds of a node's outline in page coordinates for snapping (the box corners and centre).
pub fn snap_points(doc: &Document, id: NodeId) -> Vec<Point> {
    let Some(b) = doc.world_bounds(id) else { return vec![] };
    vec![
        Point::new(b.x0, b.y0),
        Point::new(b.x1, b.y0),
        Point::new(b.x1, b.y1),
        Point::new(b.x0, b.y1),
        b.center(),
        Point::new(b.center().x, b.y0),
        Point::new(b.center().x, b.y1),
        Point::new(b.x0, b.center().y),
        Point::new(b.x1, b.center().y),
    ]
}
