//! Recursive scene drawing.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use figcraft_color::{BlendMode, Paint};
use figcraft_doc::{Document, EffectKind, Node, NodeId, NodeKind, StrokeAlign};
use figcraft_geom::{Affine, BezPath, Rect, Shape};
use tiny_skia::{FillRule, IntRect, Mask, MaskType, Pixmap, PixmapPaint, Transform};

use crate::paint::{prepare, ts_blend};
use crate::{fx, lod};

/// Shared state for one render.
pub struct Ctx<'a> {
    pub doc: &'a Document,
    /// Device pixels per document unit (for effect radii).
    pub scale: f64,
    pub outline_mode: bool,
    /// Nodes not to draw (e.g. text being edited draws through the editor).
    pub skip: &'a [NodeId],
    /// Approximate what's too small to see in detail (canvas previews; see [`crate::lod`]).
    pub lod: bool,
}

pub fn ts_transform(a: Affine) -> Transform {
    let c = a.as_coeffs();
    Transform::from_row(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32, c[4] as f32, c[5] as f32)
}

/// kurbo path → tiny-skia path (None when empty or degenerate).
pub fn ts_path(p: &BezPath) -> Option<tiny_skia::Path> {
    use figcraft_geom::kurbo::PathEl;
    let mut b = tiny_skia::PathBuilder::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(p) => b.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => b.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(c, p) => b.quad_to(c.x as f32, c.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(c1, c2, p) => b.cubic_to(c1.x as f32, c1.y as f32, c2.x as f32, c2.y as f32, p.x as f32, p.y as f32),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

fn fill_with(
    pix: &mut Pixmap,
    path: &tiny_skia::Path,
    paints: &[Paint],
    w: f64,
    h: f64,
    xf: Affine,
    rule: FillRule,
    doc: &Document,
    mask: Option<&Mask>,
) {
    for p in paints {
        let Some(prep) = prepare(p, w, h, doc) else { continue };
        let paint = tiny_skia::Paint { shader: prep.shader(), anti_alias: true, blend_mode: ts_blend(p.blend), ..Default::default() };
        pix.fill_path(path, &paint, rule, ts_transform(xf), mask);
    }
}

/// Stroke outline cache: (geometry hash) → outline path.
fn stroke_cache() -> &'static Mutex<HashMap<u64, Arc<BezPath>>> {
    static C: OnceLock<Mutex<HashMap<u64, Arc<BezPath>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hash_path(p: &BezPath, h: &mut impl Hasher) {
    for el in p.elements() {
        use figcraft_geom::kurbo::PathEl::*;
        match *el {
            MoveTo(a) | LineTo(a) => (0u8, a.x.to_bits(), a.y.to_bits()).hash(h),
            QuadTo(a, b) => (1u8, a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits()).hash(h),
            CurveTo(a, b, c) => (2u8, a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits(), c.x.to_bits(), c.y.to_bits()).hash(h),
            ClosePath => 3u8.hash(h),
        }
    }
}

/// The stroke area of a node in local coordinates.
pub fn stroke_area(n: &Node, path: &BezPath, closed: bool) -> Arc<BezPath> {
    let props = &n.stroke;
    // Per-side strokes on rectangles/frames.
    if let Some(sides) = props.sides
        && matches!(n.kind, NodeKind::Rectangle | NodeKind::Frame | NodeKind::Component { .. } | NodeKind::Instance { .. })
        && n.corners.uniform() == Some(0.0)
    {
        return Arc::new(figcraft_pathops::rect_side_strokes(n.width, n.height, sides, props.align));
    }
    // Rect/ellipse inside/outside strokes without booleans: an even-odd ring of two outlines.
    if props.dashes.is_empty() && closed && props.align != StrokeAlign::Center {
        let w = props.weight;
        let ring = |inset: f64| -> Option<BezPath> {
            let (x0, y0, ww, hh) = (inset, inset, n.width - 2.0 * inset, n.height - 2.0 * inset);
            if ww <= 0.0 || hh <= 0.0 {
                return None;
            }
            match n.kind {
                NodeKind::Rectangle
                | NodeKind::Frame
                | NodeKind::Component { .. }
                | NodeKind::ComponentSet { .. }
                | NodeKind::Instance { .. }
                | NodeKind::Section { .. }
                    if n.corners.smoothing == 0.0 =>
                {
                    let radii = n.corners.radii.map(|r| if r > 0.0 { (r - inset).max(0.0) } else { 0.0 });
                    let mut p = figcraft_geom::rounded_rect(ww, hh, radii, 0.0);
                    p.apply_affine(Affine::translate((x0, y0)));
                    Some(p)
                }
                NodeKind::Ellipse { sweep, inner, .. } if sweep.abs() >= std::f64::consts::TAU - 1e-9 && inner == 0.0 => {
                    Some(figcraft_geom::kurbo::Ellipse::from_rect(Rect::new(x0, y0, x0 + ww, y0 + hh)).to_path(0.05))
                }
                _ => None,
            }
        };
        let (outer, inner) = match props.align {
            StrokeAlign::Inside => (ring(0.0), ring(w)),
            _ => (ring(-w), ring(0.0)),
        };
        if let Some(mut o) = outer {
            if let Some(i) = inner {
                o.extend(i.elements().iter().copied());
            }
            return Arc::new(o);
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    hash_path(path, &mut h);
    (props.weight.to_bits(), props.align as u8, props.cap_start as u8, props.cap_end as u8, props.join as u8, props.miter_limit.to_bits(), closed)
        .hash(&mut h);
    for d in &props.dashes {
        d.to_bits().hash(&mut h);
    }
    let key = h.finish();
    if let Some(p) = stroke_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
        return p.clone();
    }
    let out = Arc::new(figcraft_pathops::stroke_outline(path, props, closed));
    let mut c = stroke_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.len() > 4096 {
        c.clear();
    }
    c.insert(key, out.clone());
    out
}

/// Even-odd ring paths need the even-odd rule; everything else non-zero.
fn stroke_rule(n: &Node, closed: bool) -> FillRule {
    if n.stroke.sides.is_some() || (n.stroke.dashes.is_empty() && closed && n.stroke.align != StrokeAlign::Center) {
        FillRule::EvenOdd
    } else {
        FillRule::Winding
    }
}

/// The geometry a node fills, in local space: shapes, booleans (computed from children) and text
/// (glyph outlines). Groups and pages have none.
pub fn node_outline(doc: &Document, n: &Node) -> Option<(BezPath, FillRule)> {
    match &n.kind {
        NodeKind::BooleanOp { op } => {
            let paths: Vec<BezPath> = n
                .children
                .iter()
                .filter_map(|c| doc.node(*c))
                .filter(|c| c.visible)
                .filter_map(|c| {
                    let (mut p, _) = subtree_outline(doc, c, 0)?;
                    p.apply_affine(c.transform);
                    Some(p)
                })
                .collect();
            let p = boolean_cached(&paths, *op);
            Some((p, FillRule::EvenOdd))
        }
        NodeKind::Text(_) => {
            let (l, dy) = figcraft_text::layout_node(n)?;
            let mut p = l.to_path();
            p.apply_affine(Affine::translate((0.0, dy)));
            Some((p, FillRule::Winding))
        }
        NodeKind::Vector { network } => {
            let rule =
                if network.regions.iter().any(|r| r.winding == figcraft_doc::WindingRule::EvenOdd) { FillRule::EvenOdd } else { FillRule::Winding };
            Some((network.fill_path(), rule))
        }
        _ => figcraft_doc::geometry::shape_path(n).map(|p| (p, FillRule::Winding)),
    }
}

/// The union outline of a node and its descendants (for booleans of groups/frames).
pub fn subtree_outline(doc: &Document, n: &Node, depth: usize) -> Option<(BezPath, FillRule)> {
    if depth > 64 {
        return None;
    }
    match &n.kind {
        NodeKind::Group | NodeKind::Frame | NodeKind::Component { .. } | NodeKind::Instance { .. } if !n.children.is_empty() => {
            let mut out = BezPath::new();
            if n.kind.is_frame_like()
                && let Some(p) = figcraft_doc::geometry::shape_path(n)
                && n.fills.iter().any(|f| f.visible)
            {
                out.extend(p.elements().iter().copied());
            }
            for c in n.children.iter().filter_map(|c| doc.node(*c)) {
                if !c.visible {
                    continue;
                }
                if let Some((mut p, _)) = subtree_outline(doc, c, depth + 1) {
                    p.apply_affine(c.transform);
                    out.extend(p.elements().iter().copied());
                }
            }
            Some((out, FillRule::Winding))
        }
        _ => node_outline(doc, n),
    }
}

fn boolean_cache() -> &'static Mutex<HashMap<u64, Arc<BezPath>>> {
    static C: OnceLock<Mutex<HashMap<u64, Arc<BezPath>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn boolean_cached(paths: &[BezPath], op: figcraft_doc::BoolOp) -> BezPath {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (op as u8).hash(&mut h);
    for p in paths {
        hash_path(p, &mut h);
        9u8.hash(&mut h);
    }
    let key = h.finish();
    if let Some(p) = boolean_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
        return (**p).clone();
    }
    let out = figcraft_pathops::boolean(paths, op).unwrap_or_default();
    let mut c = boolean_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if c.len() > 2048 {
        c.clear();
    }
    c.insert(key, Arc::new(out.clone()));
    out
}

fn device_rect(xf: Affine, r: Rect) -> Rect {
    figcraft_geom::transform_rect_bbox(xf, r)
}

/// The area text may paint without its glyphs being looked at: its box, generously grown.
fn text_box_bounds(n: &Node) -> Rect {
    n.local_rect().inflate(n.height.max(10.0), n.height.max(10.0) * 0.5)
}

/// Local bounds a node paints into (its box grown by stroke/effect reach, plus unclipped children).
/// Text that overflows its box counts where its glyphs are, so it isn't culled while visible;
/// text greeked at `greek_scale` (see [`lod::greeked`]) paints only inside [`text_box_bounds`].
fn local_paint_bounds(doc: &Document, n: &Node, depth: usize, greek_scale: Option<f64>) -> Rect {
    let reach = n.paint_reach();
    let mut r = n.local_rect().inflate(reach, reach);
    let clips = n.clip && n.kind.is_frame_like();
    if (!clips || n.kind.bounds_follow_children()) && depth < 64 {
        for c in n.children.iter().filter_map(|c| doc.node(*c)) {
            if !c.visible {
                continue;
            }
            let cb = local_paint_bounds(doc, c, depth + 1, greek_scale);
            r = r.union(figcraft_geom::transform_rect_bbox(c.transform, cb));
        }
    }
    // Text can overflow its box.
    if let NodeKind::Text(_) = n.kind {
        r = r.union(text_box_bounds(n).inflate(reach, reach));
        if greek_scale.is_some_and(|s| lod::greeked(n, s)) {
            return r;
        }
        if let Some((l, dy)) = figcraft_text::layout_node(n)
            && let Some(ink) = l.ink_bounds()
        {
            r = r.union(ink + figcraft_geom::kurbo::Vec2::new(0.0, dy));
        }
    }
    r
}

/// How far (document units) effects that read outside a node's paint bounds reach: blurs and
/// drop shadows. Culling and partial renders keep this much margin.
pub fn effect_margin(n: &Node) -> f64 {
    n.effects
        .iter()
        .filter(|e| e.visible)
        .map(|e| match &e.kind {
            EffectKind::BackgroundBlur { radius } | EffectKind::LayerBlur { radius } => *radius * 2.0,
            EffectKind::DropShadow(s) => s.radius * 2.0 + s.spread.abs() + s.offset.0.abs() + s.offset.1.abs(),
            _ => 0.0,
        })
        .fold(0.0, f64::max)
}

/// Everything node `id` can paint, in page coordinates: its paint bounds (glyphs of overflowing
/// text included) grown by its effects' margin. What a change to the node can affect on screen.
pub fn paint_bounds(doc: &Document, id: NodeId) -> Option<Rect> {
    let n = doc.node(id)?;
    let m = effect_margin(n).abs();
    let local = local_paint_bounds(doc, n, 0, None);
    Some(figcraft_geom::transform_rect_bbox(doc.world_transform(id), local).inflate(m, m))
}

fn needs_layer(n: &Node) -> bool {
    n.opacity < 0.999 || !matches!(n.blend, BlendMode::Normal | BlendMode::PassThrough) || n.effects.iter().any(|e| e.visible)
}

fn int_rect(r: Rect, limit: (u32, u32)) -> Option<IntRect> {
    let x0 = r.x0.floor().max(0.0);
    let y0 = r.y0.floor().max(0.0);
    let x1 = r.x1.ceil().min(f64::from(limit.0));
    let y1 = r.y1.ceil().min(f64::from(limit.1));
    if !(x1 > x0 && y1 > y0) {
        return None;
    }
    IntRect::from_xywh(x0 as i32, y0 as i32, (x1 - x0) as u32, (y1 - y0) as u32)
}

/// Draw node `id` onto `pix`, where `parent_xf` maps the parent's space to `pix` pixels.
pub fn draw_node(ctx: &Ctx, pix: &mut Pixmap, id: NodeId, parent_xf: Affine, depth: usize) {
    if depth > 256 || ctx.skip.contains(&id) {
        return;
    }
    let Some(n) = ctx.doc.node(id) else { return };
    if !n.visible || n.opacity <= 0.0 || n.is_mask {
        return;
    }
    let xf = parent_xf * n.transform;
    let (pw, ph) = (pix.width(), pix.height());
    let local = local_paint_bounds(ctx.doc, n, 0, ctx.lod.then_some(ctx.scale));
    let dev = device_rect(xf, local);
    // Effects that read outside (blur) need a margin when culling.
    let blur_margin = effect_margin(n) * ctx.scale;
    let view = Rect::new(0.0, 0.0, f64::from(pw), f64::from(ph)).inflate(blur_margin, blur_margin);
    if dev.intersect(view).is_zero_area() && !(dev.width() == 0.0 || dev.height() == 0.0) {
        return;
    }
    if ctx.outline_mode {
        draw_outline(ctx, pix, n, xf, depth);
        return;
    }
    if ctx.lod && dev.width().max(dev.height()) < lod::TINY_PX && lod::splattable(ctx.doc, n, 0) {
        lod::splat_subtree(ctx, pix, n, xf, None, 1.0, depth);
        return;
    }
    if !needs_layer(n) {
        draw_content(ctx, pix, n, xf, depth);
        return;
    }
    // Isolated layer covering the node's paint bounds, clipped to the visible area + margin. Its
    // origin can be negative (inside the margin); the size counts from there, or a layer starting
    // above or left of the pixmap came out short by the margin (a band missing at its far edge).
    let vis = dev.intersect(view);
    let (x0, y0) = (vis.x0.floor(), vis.y0.floor());
    let (w, h) = (vis.x1.ceil() - x0, vis.y1.ceil() - y0);
    if !(x0.is_finite() && y0.is_finite() && w >= 1.0 && h >= 1.0) {
        return;
    }
    let (lx, ly) = (x0 as i32, y0 as i32);
    let lw = w.min(16_384.0) as u32;
    let lh = h.min(16_384.0) as u32;
    let Some(mut layer) = Pixmap::new(lw, lh) else { return };
    let lxf = Affine::translate((-f64::from(lx), -f64::from(ly))) * xf;
    let shape = node_outline(ctx.doc, n);
    // Background blur: blurred backdrop clipped to the shape, under the content.
    for e in n.effects.iter().filter(|e| e.visible) {
        if let EffectKind::BackgroundBlur { radius } = e.kind
            && let Some((path, rule)) = &shape
            && let Some(tp) = ts_path(path)
        {
            let mut back = Pixmap::new(lw.max(1), lh.max(1));
            if let Some(back) = back.as_mut() {
                back.draw_pixmap(-lx, -ly, pix.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
                fx::blur(back, radius / 2.0 * ctx.scale);
                if let Some(mut m) = Mask::new(lw.max(1), lh.max(1)) {
                    m.fill_path(&tp, *rule, true, ts_transform(lxf));
                    layer.draw_pixmap(0, 0, back.as_ref(), &PixmapPaint::default(), Transform::identity(), Some(&m));
                }
            }
        }
    }
    draw_content(ctx, &mut layer, n, lxf, depth);
    // The shape's alpha in a `w` × `h` pixmap through `xf` (for inner shadows / noise).
    let shape_alpha_in = |w: u32, h: u32, xf: Affine| -> Option<Pixmap> {
        let (path, rule) = shape.as_ref()?;
        let tp = ts_path(path)?;
        let mut s = Pixmap::new(w, h)?;
        let paint = tiny_skia::Paint { shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::WHITE), anti_alias: true, ..Default::default() };
        s.fill_path(&tp, &paint, *rule, ts_transform(xf), None);
        Some(s)
    };
    let shape_alpha = || shape_alpha_in(lw, lh, lxf);
    let mut inner_shadows: Vec<Pixmap> = Vec::new();
    for e in n.effects.iter().filter(|e| e.visible) {
        match &e.kind {
            EffectKind::InnerShadow(s) => {
                let Some(sa) = shape_alpha() else { continue };
                // Work in a buffer padded by the shadow's reach, with the shape drawn at its offset
                // over all of it: everything outside the shape casts shadow inwards, from every
                // edge. The shape itself (not the layer's box) decides what is outside: the layer
                // can be cropped (to a render band or the view) where the shape carries on, and
                // treating that crop as an edge drew shadow bands across the layer.
                let (dx, dy) = (s.offset.0 * ctx.scale, s.offset.1 * ctx.scale);
                let pad =
                    ((s.radius / 2.0 * ctx.scale * 3.0) + s.spread.abs() * ctx.scale + dx.abs().max(dy.abs())).ceil().clamp(0.0, 2048.0) as u32 + 2;
                let at = Affine::translate((f64::from(pad) + dx, f64::from(pad) + dy)) * lxf;
                let Some(mut big) = shape_alpha_in(lw + pad * 2, lh + pad * 2, at) else { continue };
                fx::invert_alpha(&mut big);
                fx::morph(&mut big, (s.spread * ctx.scale).round() as i32);
                fx::blur(&mut big, s.radius / 2.0 * ctx.scale);
                let Some(mut cropped) = Pixmap::new(lw.max(1), lh.max(1)) else { continue };
                cropped.draw_pixmap(
                    -(pad as i32),
                    -(pad as i32),
                    big.as_ref(),
                    &PixmapPaint { blend_mode: tiny_skia::BlendMode::Source, ..Default::default() },
                    Transform::identity(),
                    None,
                );
                let shifted = &mut cropped;
                fx::mask_by(shifted, &sa, false);
                fx::tint(shifted, [s.color.r, s.color.g, s.color.b, s.color.a]);
                inner_shadows.push(shifted.clone());
            }
            EffectKind::Noise { size, density, color } => {
                if let Some(sa) = shape_alpha() {
                    fx::noise(&mut layer, &sa, *density, [color.r, color.g, color.b, color.a], id.0 as u32, size * ctx.scale * 2.0);
                }
            }
            _ => {}
        }
    }
    for s in &inner_shadows {
        layer.draw_pixmap(0, 0, s.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    for e in n.effects.iter().filter(|e| e.visible) {
        if let EffectKind::LayerBlur { radius } = e.kind {
            fx::blur(&mut layer, radius / 2.0 * ctx.scale);
        }
    }
    // Drop shadows go under the layer, in effect-list order.
    for e in n.effects.iter().filter(|e| e.visible) {
        if let EffectKind::DropShadow(s) = &e.kind {
            let mut sh = layer.clone();
            fx::morph(&mut sh, (s.spread * ctx.scale).round() as i32);
            fx::blur(&mut sh, s.radius / 2.0 * ctx.scale);
            fx::tint(&mut sh, [s.color.r, s.color.g, s.color.b, s.color.a]);
            let (dx, dy) = ((s.offset.0 * ctx.scale).round() as i32, (s.offset.1 * ctx.scale).round() as i32);
            let mut knock: Option<Mask> = None;
            if !s.show_behind {
                // Hide the shadow behind the layer's own pixels.
                if let Some(m) = Mask::from_pixmap(layer.as_ref(), MaskType::Alpha).into() {
                    let mut inv = m;
                    inv.invert();
                    knock = Some(inv);
                }
            }
            let op = n.opacity.clamp(0.0, 1.0);
            let paint = PixmapPaint { opacity: op, blend_mode: ts_blend(s.blend), ..Default::default() };
            if let Some(k) = &knock {
                // Mask is in layer space: shift the shadow into layer space first.
                let mut shifted = Pixmap::new(lw.max(1), lh.max(1));
                if let Some(shifted) = shifted.as_mut() {
                    shifted.draw_pixmap(dx, dy, sh.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
                    shifted.apply_mask(k);
                    pix.draw_pixmap(lx, ly, shifted.as_ref(), &paint, Transform::identity(), None);
                }
            } else {
                pix.draw_pixmap(lx + dx, ly + dy, sh.as_ref(), &paint, Transform::identity(), None);
            }
        }
    }
    let paint = PixmapPaint { opacity: n.opacity.clamp(0.0, 1.0), blend_mode: ts_blend(n.blend), ..Default::default() };
    pix.draw_pixmap(lx, ly, layer.as_ref(), &paint, Transform::identity(), None);
}

fn draw_outline(ctx: &Ctx, pix: &mut Pixmap, n: &Node, xf: Affine, depth: usize) {
    if let Some((path, _)) = node_outline(ctx.doc, n)
        && !matches!(n.kind, NodeKind::Group)
        && let Some(tp) = ts_path(&path)
    {
        let paint = tiny_skia::Paint { shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::BLACK), anti_alias: true, ..Default::default() };
        let stroke = tiny_skia::Stroke { width: 1.0 / ctx.scale.max(1e-6) as f32, ..Default::default() };
        pix.stroke_path(&tp, &paint, &stroke, ts_transform(xf), None);
    }
    if !matches!(n.kind, NodeKind::BooleanOp { .. }) {
        for c in &n.children {
            draw_node(ctx, pix, *c, xf, depth + 1);
        }
    }
}

/// Fills, children (clipped), strokes and text of a node, without opacity/effects.
fn draw_content(ctx: &Ctx, pix: &mut Pixmap, n: &Node, xf: Affine, depth: usize) {
    let doc = ctx.doc;
    let (w, h) = (n.width, n.height);
    match &n.kind {
        NodeKind::Page { .. } => {
            draw_children(ctx, pix, n, xf, depth);
        }
        NodeKind::Group => draw_children(ctx, pix, n, xf, depth),
        NodeKind::Text(_) if ctx.lod && lod::greeked(n, ctx.scale) => lod::greek_text(pix, n, xf, text_box_bounds(n)),
        NodeKind::Text(t) => {
            let Some((layout, dy)) = figcraft_text::layout_node(n) else { return };
            let gxf = xf * Affine::translate((0.0, dy));
            // Group glyphs by style so runs with their own fills paint with them.
            let mut by_style: HashMap<usize, BezPath> = HashMap::new();
            for g in &layout.glyphs {
                by_style.entry(g.style).or_default().extend(g.path().elements().iter().copied());
            }
            for d in &layout.decorations {
                let mut r = d.rect.to_path(0.1);
                r.apply_affine(Affine::IDENTITY);
                by_style.entry(d.style).or_default().extend(r.elements().iter().copied());
            }
            let mut keys: Vec<usize> = by_style.keys().copied().collect();
            keys.sort_unstable();
            for k in keys {
                let Some(path) = by_style.get(&k) else { continue };
                let Some(tp) = ts_path(path) else { continue };
                let fills = layout.styles.get(k).and_then(|s| s.fills.as_ref()).unwrap_or(&n.fills);
                fill_with(pix, &tp, fills, w, h, gxf, FillRule::Winding, doc, None);
                if n.has_visible_stroke() {
                    let props = figcraft_doc::StrokeProps { align: StrokeAlign::Center, ..n.stroke.clone() };
                    let s = figcraft_pathops::stroke_center(path, props.weight, &props);
                    if let Some(ts) = ts_path(&s) {
                        fill_with(pix, &ts, &n.strokes, w, h, gxf, FillRule::Winding, doc, None);
                    }
                }
            }
            let _ = t;
        }
        _ => {
            let closed = figcraft_doc::geometry::is_closed(n) || matches!(n.kind, NodeKind::BooleanOp { .. });
            let splatted = ctx.lod && closed && lod::splat_fills(pix, n, xf);
            let clips = n.clip && n.kind.is_frame_like() && !n.children.is_empty();
            // The outline fills, clips children to rounded corners and strokes booleans.
            let outline = if splatted && !(clips && n.corners.uniform() != Some(0.0)) { None } else { node_outline(doc, n) };
            if !splatted
                && let Some((path, rule)) = &outline
                && closed
                && let Some(tp) = ts_path(path)
            {
                fill_with(pix, &tp, &n.fills, w, h, xf, *rule, doc, None);
            }
            if !matches!(n.kind, NodeKind::BooleanOp { .. }) && !n.children.is_empty() {
                if clips {
                    draw_children_clipped(ctx, pix, n, xf, depth, outline.as_ref().map(|(p, _)| p));
                } else {
                    draw_children(ctx, pix, n, xf, depth);
                }
            }
            if n.has_visible_stroke() {
                let geom = match &n.kind {
                    NodeKind::BooleanOp { .. } => outline.as_ref().map(|(p, _)| p.clone()),
                    _ => figcraft_doc::geometry::stroke_geometry(n),
                };
                if let Some(g) = geom {
                    let area = stroke_area(n, &g, closed);
                    if let Some(tp) = ts_path(&area) {
                        fill_with(pix, &tp, &n.strokes, w, h, xf, stroke_rule(n, closed), doc, None);
                    }
                }
            }
        }
    }
}

fn draw_children(ctx: &Ctx, pix: &mut Pixmap, n: &Node, xf: Affine, depth: usize) {
    let kids = &n.children;
    let mut i = 0;
    while i < kids.len() {
        let Some(&c) = kids.get(i) else { break };
        let is_mask = ctx.doc.node(c).is_some_and(|cn| cn.is_mask && cn.visible);
        if is_mask {
            // Everything above the mask (until the next mask) is clipped by its alpha.
            let mut j = i + 1;
            while kids.get(j).is_some_and(|k| !ctx.doc.node(*k).is_some_and(|kn| kn.is_mask)) {
                j += 1;
            }
            draw_masked(ctx, pix, c, kids.get(i + 1..j).unwrap_or(&[]), xf, depth);
            i = j;
            continue;
        }
        draw_node(ctx, pix, c, xf, depth + 1);
        i += 1;
    }
}

fn draw_masked(ctx: &Ctx, pix: &mut Pixmap, mask_id: NodeId, content: &[NodeId], xf: Affine, depth: usize) {
    let (w, h) = (pix.width(), pix.height());
    let Some(mut mask_px) = Pixmap::new(w, h) else { return };
    let mask_type = ctx.doc.node(mask_id).map(|m| m.mask_type).unwrap_or_default();
    // Draw the mask node as if it weren't a mask (a vector mask: its outline, fully opaque).
    if let Some(m) = ctx.doc.node(mask_id) {
        let mut m2 = m.clone();
        m2.is_mask = false;
        let mxf = xf * m2.transform;
        if mask_type == figcraft_doc::MaskType::Vector {
            if let Some((outline, rule)) = subtree_outline(ctx.doc, &m2, depth + 1)
                && let Some(path) = ts_path(&outline)
            {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(255, 255, 255, 255);
                paint.anti_alias = true;
                mask_px.fill_path(&path, &paint, rule, ts_transform(mxf), None);
            }
        } else if needs_layer(&m2) {
            let paint = PixmapPaint { opacity: m2.opacity, ..Default::default() };
            let mut tmp = Pixmap::new(w, h);
            if let Some(tmp) = tmp.as_mut() {
                draw_content(ctx, tmp, &m2, mxf, depth + 1);
                mask_px.draw_pixmap(0, 0, tmp.as_ref(), &paint, Transform::identity(), None);
            }
        } else {
            draw_content(ctx, &mut mask_px, &m2, mxf, depth + 1);
        }
    }
    let Some(mut layer) = Pixmap::new(w, h) else { return };
    for c in content {
        draw_node(ctx, &mut layer, *c, xf, depth + 1);
    }
    let kind = if mask_type == figcraft_doc::MaskType::Luminance { MaskType::Luminance } else { MaskType::Alpha };
    let mask = Mask::from_pixmap(mask_px.as_ref(), kind);
    pix.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), Some(&mask));
}

fn draw_children_clipped(ctx: &Ctx, pix: &mut Pixmap, n: &Node, xf: Affine, depth: usize, clip: Option<&BezPath>) {
    let (pw, ph) = (pix.width(), pix.height());
    let dev = device_rect(xf, n.local_rect());
    let Some(ir) = int_rect(dev, (pw, ph)) else { return };
    let (lx, ly) = (ir.x(), ir.y());
    let Some(mut layer) = Pixmap::new(ir.width(), ir.height()) else { return };
    let lxf = Affine::translate((-f64::from(lx), -f64::from(ly))) * xf;
    draw_children(ctx, &mut layer, n, lxf, depth);
    let rect_clip = n.corners.uniform() == Some(0.0) && is_axis_aligned(xf);
    if rect_clip {
        if ctx.lod {
            lod::blit_over(pix, lx, ly, &layer);
        } else {
            pix.draw_pixmap(lx, ly, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        return;
    }
    if let Some(cp) = clip.and_then(ts_path)
        && let Some(mut m) = Mask::new(ir.width(), ir.height())
    {
        m.fill_path(&cp, FillRule::Winding, true, ts_transform(lxf));
        layer.apply_mask(&m);
    }
    pix.draw_pixmap(lx, ly, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

fn is_axis_aligned(xf: Affine) -> bool {
    let c = xf.as_coeffs();
    c[1].abs() < 1e-9 && c[2].abs() < 1e-9 && fract_ok(c[4]) && fract_ok(c[5])
}

fn fract_ok(_v: f64) -> bool {
    true
}
