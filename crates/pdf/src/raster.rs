//! Rasterised parts: effects (shadows, blurs, noise, texture) drawn by the tiny-skia renderer
//! and placed as images in page space.
//!
//! - Nodes with a layer blur, noise or texture are rendered whole (`figcraft_render`) and placed
//!   as one image.
//! - Drop and inner shadows are computed from a render of the node without its effects (the
//!   renderer's own shadows: spread offsets the outline, blur, tint, offset, knock-out), so the
//!   node itself stays vector: drop shadows go under it, inner shadows over it.
//! - Background blur and glass are drawn by the renderer from a render of what lies under the
//!   node on the page, and placed under its content.

use figcraft_color::BlendMode;
use figcraft_doc::{EffectKind, NodeId};
use figcraft_geom::{Affine, Rect};
use figcraft_render::{Pixmap, View, backdrop, shadow};
use krilla::geom::Size;
use krilla::image::Image;
use krilla::surface::Surface;

use crate::draw::{Exporter, MAX_RASTER_SIDE, affine_ok, blend, norm, to_xf};

/// An image covering `rect` (document page coordinates).
pub(crate) struct Raster {
    image: Image,
    rect: Rect,
    px: (u32, u32),
}

#[derive(Default)]
pub(crate) struct Shadows {
    pub drop: Vec<(Raster, BlendMode)>,
    pub inner: Vec<Raster>,
}

fn raster(p: &Pixmap, rect: Rect) -> Raster {
    Raster { image: Image::from_rgba8(figcraft_render::to_rgba8(p), p.width(), p.height()), rect, px: (p.width(), p.height()) }
}

impl Exporter<'_> {
    /// A view of page area `b` (clipped to the page) at the raster scale, at most
    /// [`MAX_RASTER_SIDE`] pixels a side.
    fn raster_view(&self, b: Rect) -> Option<View> {
        let b = b.intersect(self.page_rect);
        if !(b.width() > 0.0 && b.height() > 0.0 && crate::rect_finite(b)) {
            return None;
        }
        let mut k = self.raster_scale();
        let side = b.width().max(b.height()) * k;
        if side > MAX_RASTER_SIDE {
            k *= MAX_RASTER_SIDE / side;
        }
        let (w, h) = ((b.width() * k).ceil().max(1.0), (b.height() * k).ceil().max(1.0));
        Some(View { origin: (b.x0, b.y0), scale: k, width: w as u32, height: h as u32 })
    }

    fn view_rect(v: &View) -> Rect {
        let k = v.scale.max(1e-9);
        Rect::new(v.origin.0, v.origin.1, v.origin.0 + f64::from(v.width) / k, v.origin.1 + f64::from(v.height) / k)
    }

    /// Draw a raster image at its page rectangle.
    pub(crate) fn place_raster(&self, s: &mut Surface, r: &Raster, opacity: f32, b: BlendMode) {
        let (pw, ph) = r.px;
        let Some(size) = Size::from_wh(pw as f32, ph as f32) else { return };
        let t = self.page_origin
            * Affine::translate((r.rect.x0, r.rect.y0))
            * Affine::scale_non_uniform(r.rect.width() / f64::from(pw.max(1)), r.rect.height() / f64::from(ph.max(1)));
        if !affine_ok(t) {
            return;
        }
        let pb = blend(b);
        if let Some(b) = pb {
            s.push_blend_mode(b);
        }
        if opacity < 1.0 {
            s.push_opacity(norm(opacity));
        }
        s.push_transform(&to_xf(t));
        s.draw_image(r.image.clone(), size);
        s.pop();
        if opacity < 1.0 {
            s.pop();
        }
        if pb.is_some() {
            s.pop();
        }
    }

    /// Render node `id` whole (with its effects, opacity and blend) and place it as an image.
    pub(crate) fn raster_node(&mut self, s: &mut Surface, id: NodeId, as_mask: bool) {
        let doc = self.doc;
        let Some(n) = doc.node(id) else { return };
        let mut tmp = doc.clone();
        if let Some(tn) = tmp.node_mut(id) {
            if as_mask {
                tn.is_mask = false;
            }
            // Its backdrop effects are placed separately (see `backdrop`): an isolated render has
            // no backdrop to draw them from.
            tn.effects.retain(|e| !matches!(e.kind, EffectKind::BackgroundBlur { .. } | EffectKind::Glass(_)));
        }
        let world = doc.world_transform(id);
        let sc = world.determinant().abs().sqrt();
        let sc = if sc.is_finite() && sc > 0.0 { sc } else { 1.0 };
        // Blurs read (and spread) beyond the painted bounds.
        let margin = n
            .effects
            .iter()
            .filter(|e| e.visible)
            .map(|e| match &e.kind {
                EffectKind::LayerBlur { radius } => radius.abs() * 2.0,
                EffectKind::DropShadow(sh) => {
                    sh.radius.abs() * 2.0 + sh.spread.abs() * figcraft_doc::SPREAD_MITER_LIMIT + sh.offset.0.abs() + sh.offset.1.abs()
                }
                EffectKind::Texture(t) => t.radius.abs(),
                _ => 0.0,
            })
            .filter(|m| m.is_finite())
            .fold(0.0, f64::max)
            * sc;
        let Some(b) = tmp.world_render_bounds(id) else { return };
        let Some(view) = self.raster_view(b.inflate(margin, margin)) else { return };
        let Some(mut pix) = Pixmap::new(view.width, view.height) else { return };
        figcraft_render::render_nodes_into(&tmp, &[id], &view, &mut pix);
        // The renderer applied opacity; the blend mode applies against the PDF backdrop.
        self.place_raster(s, &raster(&pix, Self::view_rect(&view)), 1.0, n.blend);
    }

    /// Node `id`'s background blurs and glass as an image over its shape, drawn by the renderer
    /// from a render of what lies under the node on this page (everything drawn before it, its
    /// ancestors' own fills included), the way the canvas and PNG export draw them.
    pub(crate) fn backdrop(&mut self, id: NodeId) -> Option<Raster> {
        let (pix, view) = self.backdrop_pixels(id)?;
        Some(raster(&pix, Self::view_rect(&view)))
    }

    /// [`Exporter::backdrop`]'s pixels and the page area they cover.
    pub(crate) fn backdrop_pixels(&self, id: NodeId) -> Option<(Pixmap, View)> {
        let doc = self.doc;
        let n = doc.node(id)?;
        let shape = figcraft_render::node_outline(doc, n)?;
        let world = doc.world_transform(id);
        let bounds = figcraft_geom::transform_rect_bbox(world, figcraft_geom::Shape::bounding_box(&shape.0));
        let view = self.raster_view(bounds)?;
        let pad = backdrop::pad(n, view.scale);
        let p = f64::from(pad) / view.scale.max(1e-9);
        let around = View {
            origin: (view.origin.0 - p, view.origin.1 - p),
            scale: view.scale,
            width: view.width.saturating_add(pad * 2),
            height: view.height.saturating_add(pad * 2),
        };
        let mut back = backdrop::render_beneath(doc, &self.page_ids, id, &around)?;
        // Nothing is exported past the page's edge (the renderer's export ends there too).
        let page = Rect::new(
            (self.page_rect.x0 - around.origin.0) * view.scale,
            (self.page_rect.y0 - around.origin.1) * view.scale,
            (self.page_rect.x1 - around.origin.0) * view.scale,
            (self.page_rect.y1 - around.origin.1) * view.scale,
        );
        keep_inside(&mut back, page);
        let fx = backdrop::effects(n, &shape, &back, pad, view.width, view.height, view.transform() * world, view.scale)?;
        Some((fx, view))
    }

    /// The drop and inner shadows of node `id`, as images (the renderer's own shadows).
    pub(crate) fn shadows(&mut self, id: NodeId) -> Option<Shadows> {
        let doc = self.doc;
        let n = doc.node(id)?;
        let mut tmp = doc.clone();
        {
            let tn = tmp.node_mut(id)?;
            tn.effects.clear();
            tn.opacity = 1.0;
            tn.blend = BlendMode::Normal;
            tn.is_mask = false;
        }
        let world = doc.world_transform(id);
        let sc = world.determinant().abs().sqrt();
        let sc = if sc.is_finite() && sc > 0.0 { sc } else { 1.0 };
        let reach = n
            .effects
            .iter()
            .filter(|e| e.visible)
            .map(|e| match &e.kind {
                // Inner shadows need room outside the shape: everything outside casts them.
                EffectKind::DropShadow(sh) | EffectKind::InnerShadow(sh) => {
                    sh.radius.abs() * 2.0 + sh.spread.abs() * figcraft_doc::SPREAD_MITER_LIMIT + sh.offset.0.abs() + sh.offset.1.abs()
                }
                _ => 0.0,
            })
            .filter(|m| m.is_finite())
            .fold(0.0, f64::max)
            * sc;
        let b = tmp.world_render_bounds(id)?.inflate(reach + 1.0, reach + 1.0);
        let view = self.raster_view(b)?;
        let rect = Self::view_rect(&view);
        let (w, h) = (view.width, view.height);
        let mut layer = Pixmap::new(w, h)?;
        figcraft_render::render_nodes_into(&tmp, &[id], &view, &mut layer);
        let tn = tmp.node(id)?;
        let xf = view.transform() * world;
        let spreads = n.effects.iter().any(|e| e.visible && matches!(&e.kind, EffectKind::DropShadow(s) if s.spread != 0.0));
        let sil = if spreads { shadow::silhouette(&tmp, tn) } else { None };
        let caster = shadow::Caster { layer: &layer, xf, scale: view.scale, silhouette: sil.as_deref() };
        let shape = figcraft_render::node_outline(&tmp, tn);
        let mut out = Shadows::default();
        for e in n.effects.iter().filter(|e| e.visible) {
            match &e.kind {
                EffectKind::DropShadow(sh) => {
                    if let Some(p) = shadow::drop_shadow(&caster, sh) {
                        out.drop.push((raster(&p, rect), sh.blend));
                    }
                }
                EffectKind::InnerShadow(sh) => {
                    if let Some(p) = shape.as_ref().and_then(|o| shadow::inner_shadow(o, w, h, xf, view.scale, sh)) {
                        out.inner.push(raster(&p, rect));
                    }
                }
                _ => {}
            }
        }
        Some(out)
    }
}

/// Clear the pixels of `p` outside `r` (pixel coordinates).
fn keep_inside(p: &mut Pixmap, r: Rect) {
    let w = p.width() as usize;
    for (i, px) in p.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        if !(x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1) {
            *px = [0; 4];
        }
    }
}
