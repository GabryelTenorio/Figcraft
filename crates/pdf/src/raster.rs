//! Rasterised parts: effects (shadows, blurs, noise, texture) drawn by the tiny-skia renderer
//! and placed as images in page space.
//!
//! - Nodes with a layer blur, noise or texture are rendered whole (`figcraft_render`) and placed
//!   as one image.
//! - Drop and inner shadows are computed from a render of the node without its effects (the
//!   renderer's own shadow steps: spread, blur, tint, offset, knock-out), so the node itself
//!   stays vector: drop shadows go under it, inner shadows over it.

use figcraft_color::BlendMode;
use figcraft_doc::{EffectKind, NodeId};
use figcraft_geom::{Affine, Rect};
use figcraft_render::fx;
use figcraft_render::{Pixmap, View};
use krilla::geom::Size;
use krilla::image::Image;
use krilla::surface::Surface;
use tiny_skia::{PixmapPaint, Transform};

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

fn color4(c: figcraft_color::Color) -> [f32; 4] {
    let c = c.sanitized();
    [c.r, c.g, c.b, c.a]
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
        if as_mask && let Some(tn) = tmp.node_mut(id) {
            tn.is_mask = false;
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
                EffectKind::LayerBlur { radius } | EffectKind::BackgroundBlur { radius } => radius.abs() * 2.0,
                EffectKind::DropShadow(sh) => sh.radius.abs() * 2.0 + sh.spread.abs() + sh.offset.0.abs() + sh.offset.1.abs(),
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

    /// The drop and inner shadows of node `id`, as images.
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
                    sh.radius.abs() * 2.0 + sh.spread.abs() + sh.offset.0.abs() + sh.offset.1.abs()
                }
                _ => 0.0,
            })
            .filter(|m| m.is_finite())
            .fold(0.0, f64::max)
            * sc;
        let b = tmp.world_render_bounds(id)?.inflate(reach + 1.0, reach + 1.0);
        let view = self.raster_view(b)?;
        let rect = Self::view_rect(&view);
        let k = view.scale;
        let (w, h) = (view.width, view.height);
        let mut layer = Pixmap::new(w, h)?;
        figcraft_render::render_nodes_into(&tmp, &[id], &view, &mut layer);
        let tn = tmp.node(id)?;
        let shape_alpha = || -> Option<Pixmap> {
            let (path, rule) = figcraft_render::node_outline(&tmp, tn)?;
            let tp = figcraft_render::ts_path(&path)?;
            let mut sa = Pixmap::new(w, h)?;
            let paint = tiny_skia::Paint { shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::WHITE), anti_alias: true, ..Default::default() };
            sa.fill_path(&tp, &paint, rule, figcraft_render::draw::ts_transform(view.transform() * world), None);
            Some(sa)
        };
        let mut out = Shadows::default();
        for e in n.effects.iter().filter(|e| e.visible) {
            match &e.kind {
                EffectKind::DropShadow(sh) => {
                    let mut blurred = layer.clone();
                    fx::morph(&mut blurred, finite_px(sh.spread * k));
                    fx::blur(&mut blurred, sh.radius / 2.0 * k);
                    fx::tint(&mut blurred, color4(sh.color));
                    let mut shifted = Pixmap::new(w, h)?;
                    shifted.draw_pixmap(
                        finite_px(sh.offset.0 * k),
                        finite_px(sh.offset.1 * k),
                        blurred.as_ref(),
                        &PixmapPaint::default(),
                        Transform::identity(),
                        None,
                    );
                    if !sh.show_behind {
                        // Hidden behind the layer's own pixels.
                        fx::mask_by(&mut shifted, &layer, true);
                    }
                    out.drop.push((raster(&shifted, rect), sh.blend));
                }
                EffectKind::InnerShadow(sh) => {
                    let Some(sa) = shape_alpha() else { continue };
                    let mut inv = sa.clone();
                    fx::invert_alpha(&mut inv);
                    let mut shifted = Pixmap::new(w, h)?;
                    // Outside the raster counts as outside the shape too.
                    shifted.fill(tiny_skia::Color::WHITE);
                    let (dx, dy) = (finite_px(sh.offset.0 * k), finite_px(sh.offset.1 * k));
                    if let Some(r) = tiny_skia::Rect::from_xywh(dx as f32, dy as f32, w as f32, h as f32) {
                        let clear = tiny_skia::Paint {
                            shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::TRANSPARENT),
                            blend_mode: tiny_skia::BlendMode::Source,
                            ..Default::default()
                        };
                        shifted.fill_rect(r, &clear, Transform::identity(), None);
                    }
                    shifted.draw_pixmap(dx, dy, inv.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
                    fx::morph(&mut shifted, finite_px(sh.spread * k));
                    fx::blur(&mut shifted, sh.radius / 2.0 * k);
                    fx::mask_by(&mut shifted, &sa, false);
                    fx::tint(&mut shifted, color4(sh.color));
                    out.inner.push(raster(&shifted, rect));
                }
                _ => {}
            }
        }
        Some(out)
    }
}

/// A device-pixel distance as an integer offset (non-finite → 0, capped).
fn finite_px(v: f64) -> i32 {
    if v.is_finite() { v.round().clamp(-100_000.0, 100_000.0) as i32 } else { 0 }
}
