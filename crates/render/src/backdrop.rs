//! Backdrops: what lies under a layer, for the effects that show it (background blur and glass).
//!
//! A render draws some layers into surfaces of their own (opacity, blend modes and effects,
//! clipping frames, masks) and composites each onto the surface below once it is done. A layer's
//! backdrop is everything drawn under it so far on all of those surfaces, composited the way they
//! will be: the frame's own fill under a clipped child, the page under that. [`Under`] records the
//! stack of surfaces as the render descends; [`backdrop`] flattens it for a rectangle.
//!
//! The PDF export, which has no pixels to read, renders what lies under the layer instead
//! ([`render_beneath`]) and draws the same [`effects`] from it.

use figcraft_doc::{Document, EffectKind, Node, NodeId};
use figcraft_geom::Affine;
use tiny_skia::{Mask, Pixmap, PixmapPaint, Transform};

use crate::shadow::{Outline, spread_alpha};

/// Deepest ancestry walked (the renderer's nesting limit).
const MAX_DEPTH: usize = 256;
use crate::{View, fx, glass};

/// One isolated surface of a render, and how it will be composited onto the surface below.
#[derive(Clone, Copy)]
pub struct Under<'a> {
    /// The surface below.
    pub pix: &'a Pixmap,
    /// Where this surface's top-left pixel lands on `pix`.
    pub at: (i32, i32),
    pub opacity: f32,
    pub blend: tiny_skia::BlendMode,
    /// What this surface is clipped by when composited, in its own pixels.
    pub clip: Option<&'a Mask>,
    /// The surface under `pix`, if `pix` is itself an isolated surface.
    pub below: Option<&'a Under<'a>>,
}

impl<'a> Under<'a> {
    /// A surface drawn at `at` onto `pix` (itself over `below`), as is.
    pub fn new(pix: &'a Pixmap, at: (i32, i32), below: Option<&'a Under<'a>>) -> Self {
        Under { pix, at, opacity: 1.0, blend: tiny_skia::BlendMode::SourceOver, clip: None, below }
    }
}

/// Multiply `p` (the pixels at `(x, y)` of a surface) by the coverage of `m` (that surface's clip).
fn clip_at(p: &mut Pixmap, m: &Mask, x: i32, y: i32) {
    let (mw, mh) = (i64::from(m.width()), i64::from(m.height()));
    let w = p.width() as usize;
    let md = m.data();
    for (i, px) in p.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (mx, my) = (i64::from(x) + (i % w) as i64, i64::from(y) + (i / w) as i64);
        let c = if mx >= 0 && my >= 0 && mx < mw && my < mh { u32::from(md.get((my * mw + mx) as usize).copied().unwrap_or(0)) } else { 0 };
        if c < 255 {
            for v in px.iter_mut() {
                *v = ((u32::from(*v) * c + 127) / 255) as u8;
            }
        }
    }
}

/// The backdrop of the `w` × `h` pixel rectangle at `(x, y)` of `pix`: what is drawn on `pix` so
/// far, over everything under it.
pub fn backdrop(pix: &Pixmap, under: Option<&Under>, x: i32, y: i32, w: u32, h: u32) -> Option<Pixmap> {
    let mut part = Pixmap::new(w, h)?;
    let copy = PixmapPaint { blend_mode: tiny_skia::BlendMode::Source, ..Default::default() };
    part.draw_pixmap(x.checked_neg()?, y.checked_neg()?, pix.as_ref(), &copy, Transform::identity(), None);
    let Some(u) = under else { return Some(part) };
    let mut out = backdrop(u.pix, u.below, x.checked_add(u.at.0)?, y.checked_add(u.at.1)?, w, h)?;
    if let Some(m) = u.clip {
        clip_at(&mut part, m, x, y);
    }
    out.draw_pixmap(0, 0, part.as_ref(), &PixmapPaint { opacity: u.opacity, blend_mode: u.blend, ..Default::default() }, Transform::identity(), None);
    Some(out)
}

/// Whether a node has a visible background blur or glass.
pub fn has_effects(n: &Node) -> bool {
    n.effects.iter().any(|e| (e.visible && matches!(e.kind, EffectKind::Glass(_))) || e.backdrop_radius() > 0.0)
}

/// Pixels around a layer its backdrop effects read at `scale`.
pub fn pad(n: &Node, scale: f64) -> u32 {
    let r = n.effects.iter().map(|e| crate::backdrop_reach(e.backdrop_radius(), scale)).fold(0.0, f64::max);
    if r.is_finite() { r.ceil().clamp(0.0, 2048.0) as u32 } else { 0 }
}

/// A node's backdrop effects (background blurs and glass, in list order) for its `w` × `h` layer,
/// from `back`: its backdrop around the layer, `pad` pixels more each way. `shape` maps through
/// `xf` (local → layer pixels) at `scale` device pixels per unit. `None` when nothing shows.
#[allow(clippy::too_many_arguments)]
pub fn effects(n: &Node, shape: &Outline, back: &Pixmap, pad: u32, w: u32, h: u32, xf: Affine, scale: f64) -> Option<Pixmap> {
    let pad_i = i32::try_from(pad).ok()?;
    let mut out: Option<Pixmap> = None;
    let mut cov: Option<Pixmap> = None;
    for e in n.effects.iter().filter(|e| e.visible) {
        let layer = match &e.kind {
            EffectKind::BackgroundBlur { radius } if *radius > 0.0 => {
                let mut b = back.clone();
                fx::blur(&mut b, radius / 2.0 * scale);
                let mut l = Pixmap::new(w, h)?;
                let copy = PixmapPaint { blend_mode: tiny_skia::BlendMode::Source, ..Default::default() };
                l.draw_pixmap(-pad_i, -pad_i, b.as_ref(), &copy, Transform::identity(), None);
                if cov.is_none() {
                    cov = spread_alpha(std::slice::from_ref(shape), 0.0, xf, w, h);
                }
                fx::mask_by(&mut l, cov.as_ref()?, false);
                l
            }
            EffectKind::Glass(g) => glass::glass(g, shape, back, pad, w, h, xf, scale)?,
            _ => continue,
        };
        match out.as_mut() {
            Some(o) => o.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None),
            None => out = Some(layer),
        }
    }
    out
}

/// Hide node `id` and everything painted after it (later siblings up its ancestry, the later
/// `roots` and the strokes its ancestors paint over their children): what is left is its backdrop
/// on the page made of `roots` (in z-order).
pub fn hide_above(doc: &mut Document, id: NodeId, roots: &[NodeId]) {
    let mut hide: Vec<NodeId> = vec![id];
    let mut cur = id;
    for _ in 0..=MAX_DEPTH {
        if let Some(i) = roots.iter().position(|r| *r == cur) {
            hide.extend(roots.iter().skip(i + 1));
            break;
        }
        let Some(parent) = doc.node(cur).and_then(|n| n.parent) else { break };
        if let Some(p) = doc.node(parent) {
            hide.extend(p.children.iter().skip_while(|c| **c != cur).skip(1));
        }
        if let Some(pn) = doc.node_mut(parent) {
            pn.strokes.clear();
        }
        cur = parent;
    }
    for h in hide {
        if let Some(n) = doc.node_mut(h) {
            n.visible = false;
        }
    }
}

/// What lies under node `id` when `roots` (in z-order) are drawn into `view`: every layer drawn
/// before it, its ancestors' own fills included (see [`hide_above`]), the way the renderer's
/// [`backdrop`] sees it.
pub fn render_beneath(doc: &Document, roots: &[NodeId], id: NodeId, view: &View) -> Option<Pixmap> {
    let mut beneath = doc.clone();
    hide_above(&mut beneath, id, roots);
    let mut pix = Pixmap::new(view.width.clamp(1, 16_384), view.height.clamp(1, 16_384))?;
    crate::render_nodes_into(&beneath, roots, view, &mut pix);
    Some(pix)
}
