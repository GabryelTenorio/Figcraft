//! The FigCraft renderer.
//!
//! [`render_view`] draws a page region at any zoom into a premultiplied RGBA pixmap (the canvas);
//! [`render_node`] rasterises one node for export at a scale. Everything is CPU (tiny-skia) so it
//! runs identically on desktop, in tests and in the browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod draw;
pub mod fx;
pub mod images;
pub mod paint;
pub mod retained;

pub use draw::{node_outline, paint_bounds, subtree_outline, ts_path};
pub use retained::{Retained, Work};
pub use tiny_skia::{FillRule, Pixmap};

use figcraft_color::Color;
use figcraft_doc::{Document, NodeId, NodeKind};
use figcraft_geom::{Affine, Rect};

/// Which part of a page to draw, and how big.
#[derive(Clone, Debug)]
pub struct View {
    /// Page coordinate shown at the pixmap's top-left.
    pub origin: (f64, f64),
    /// Device pixels per page unit.
    pub scale: f64,
    pub width: u32,
    pub height: u32,
}

impl View {
    pub fn transform(&self) -> Affine {
        Affine::scale(self.scale) * Affine::translate((-self.origin.0, -self.origin.1))
    }
    pub fn page_rect(&self) -> Rect {
        let s = self.scale.max(1e-9);
        Rect::new(self.origin.0, self.origin.1, self.origin.0 + f64::from(self.width) / s, self.origin.1 + f64::from(self.height) / s)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Fill the background (None = transparent, e.g. export without page colour).
    pub background: Option<Color>,
    pub outline_mode: bool,
    /// Nodes to leave out (an editor drawing them itself).
    pub skip: Vec<NodeId>,
}

fn new_pixmap(w: u32, h: u32) -> Option<Pixmap> {
    let w = w.clamp(1, 16_384);
    let h = h.clamp(1, 16_384);
    Pixmap::new(w, h)
}

/// The largest visible background blur radius in the document (0 without any).
pub(crate) fn max_backdrop_blur(doc: &Document) -> f64 {
    doc.nodes()
        .flat_map(|n| n.effects.iter())
        .filter(|e| e.visible)
        .filter_map(|e| match e.kind {
            figcraft_doc::EffectKind::BackgroundBlur { radius } if radius.is_finite() => Some(radius),
            _ => None,
        })
        .fold(0.0, f64::max)
}

/// How far (device pixels) a background blur of `radius` reads its backdrop at `scale`.
pub(crate) fn backdrop_reach(radius: f64, scale: f64) -> f64 {
    // The three box passes of a blur with sigma = radius / 2 reach at most 3 sigma + 3 pixels.
    if radius > 0.0 { radius * 1.5 * scale + 3.0 } else { 0.0 }
}

/// Render a page region, splitting it into horizontal bands rendered in parallel (desktop).
pub fn render_view(doc: &Document, page: NodeId, view: &View, opts: &Options) -> Option<Pixmap> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = rayon::current_num_threads().clamp(1, 16) as u32;
        let bands = threads.min(view.height / 96).max(1);
        if bands > 1
            && view.width.saturating_mul(view.height) > 300_000
            && let Some(p) = render_bands(doc, page, view, opts, bands)
        {
            return Some(p);
        }
    }
    render_view_single(doc, page, view, opts)
}

/// Render `view` as `bands` horizontal bands in parallel. Bands overlap by what background blurs
/// read and each keeps only its own rows, so a backdrop never ends at a band's edge. `None` when a
/// blur reaches past a whole band (every band would render most of the view: render it in one
/// piece instead).
#[cfg(not(target_arch = "wasm32"))]
fn render_bands(doc: &Document, page: NodeId, view: &View, opts: &Options, bands: u32) -> Option<Pixmap> {
    use rayon::prelude::*;
    let band_h = view.height.div_ceil(bands.max(1));
    let overlap = backdrop_reach(max_backdrop_blur(doc), view.scale).ceil();
    if !overlap.is_finite() || overlap > f64::from(band_h) {
        return None;
    }
    let overlap = overlap as u32;
    let s = view.scale.max(1e-9);
    let parts: Vec<(u32, Option<Pixmap>)> = (0..bands)
        .into_par_iter()
        .map(|i| {
            let y0 = i * band_h;
            let h = band_h.min(view.height.saturating_sub(y0)).max(1);
            let top = y0.saturating_sub(overlap);
            let bottom = (y0 + h + overlap).min(view.height).max(y0 + h);
            let v = View { origin: (view.origin.0, view.origin.1 + f64::from(top) / s), scale: view.scale, width: view.width, height: bottom - top };
            let part = render_view_single(doc, page, &v, opts);
            let own = tiny_skia::IntRect::from_xywh(0, (y0 - top) as i32, view.width.clamp(1, 16_384), h);
            (y0, if overlap == 0 { part } else { part.zip(own).and_then(|(p, r)| p.clone_rect(r)) })
        })
        .collect();
    let mut out = new_pixmap(view.width, view.height)?;
    for (y0, part) in parts {
        let Some(part) = part else { continue };
        out.draw_pixmap(
            0,
            y0 as i32,
            part.as_ref(),
            &tiny_skia::PixmapPaint { blend_mode: tiny_skia::BlendMode::Source, ..Default::default() },
            tiny_skia::Transform::identity(),
            None,
        );
    }
    Some(out)
}

/// Render a page region on the calling thread.
pub fn render_view_single(doc: &Document, page: NodeId, view: &View, opts: &Options) -> Option<Pixmap> {
    let mut pix = new_pixmap(view.width, view.height)?;
    let bg = opts.background.or_else(|| match doc.node(page).map(|p| &p.kind) {
        Some(NodeKind::Page { background, .. }) => Some(*background),
        _ => None,
    });
    if let Some(bg) = bg {
        pix.fill(paint::ts_color(bg));
    }
    let ctx = draw::Ctx { doc, scale: view.scale, outline_mode: opts.outline_mode, skip: &opts.skip };
    let xf = view.transform();
    let Some(p) = doc.node(page) else { return Some(pix) };
    for c in &p.children {
        draw::draw_node(&ctx, &mut pix, *c, xf, 0);
    }
    Some(pix)
}

/// Render several nodes (in z-order) into an existing pixmap through `view`.
pub fn render_nodes_into(doc: &Document, ids: &[NodeId], view: &View, pix: &mut Pixmap) {
    let ctx = draw::Ctx { doc, scale: view.scale, outline_mode: false, skip: &[] };
    for id in ids {
        let parent_xf = doc.node(*id).and_then(|n| n.parent).map(|p| doc.world_transform(p)).unwrap_or(Affine::IDENTITY);
        draw::draw_node(&ctx, pix, *id, view.transform() * parent_xf, 0);
    }
}

/// The page-space rectangle a node exports: a clipping frame's box (plus its own effects), else
/// everything it paints.
pub fn export_bounds(doc: &Document, id: NodeId) -> Option<Rect> {
    let n = doc.node(id)?;
    if n.kind.is_frame_like() && n.clip {
        let reach = n.paint_reach();
        return Some(figcraft_geom::transform_rect_bbox(doc.world_transform(id), n.local_rect().inflate(reach, reach)));
    }
    doc.world_render_bounds(id)
}

/// Rasterise one node at `scale` (export). Transparent background.
pub fn render_node(doc: &Document, id: NodeId, scale: f64) -> Option<Pixmap> {
    let b = export_bounds(doc, id)?;
    let scale = if scale.is_finite() && scale > 0.0 { scale.min(64.0) } else { 1.0 };
    let w = (b.width() * scale - 1e-6).ceil().max(1.0);
    let h = (b.height() * scale - 1e-6).ceil().max(1.0);
    if w > 16_384.0 || h > 16_384.0 {
        return None;
    }
    let view = View { origin: (b.x0, b.y0), scale, width: w as u32, height: h as u32 };
    let mut pix = new_pixmap(view.width, view.height)?;
    render_nodes_into(doc, &[id], &view, &mut pix);
    Some(pix)
}

/// Un-premultiply into straight RGBA8 bytes.
pub fn to_rgba8(p: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(p.data().len());
    for px in p.data().as_chunks::<4>().0 {
        let a = px.get(3).copied().unwrap_or(0);
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            out.extend_from_slice(&[
                un(px.first().copied().unwrap_or(0)),
                un(px.get(1).copied().unwrap_or(0)),
                un(px.get(2).copied().unwrap_or(0)),
                a,
            ]);
        }
    }
    out
}

/// Encode as PNG.
pub fn encode_png(p: &Pixmap) -> Result<Vec<u8>, String> {
    let rgba = to_rgba8(p);
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    image::ImageEncoder::write_image(enc, &rgba, p.width(), p.height(), image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Encode as JPEG over `background` (JPEG has no alpha).
pub fn encode_jpeg(p: &Pixmap, background: Color, quality: u8) -> Result<Vec<u8>, String> {
    let rgba = to_rgba8(p);
    let bg = background.to_rgba8();
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.as_chunks::<4>().0 {
        let a = u32::from(px.get(3).copied().unwrap_or(0));
        for c in 0..3 {
            let s = u32::from(px.get(c).copied().unwrap_or(0));
            let b = u32::from(bg.get(c).copied().unwrap_or(255));
            rgb.push(((s * a + b * (255 - a) + 127) / 255) as u8);
        }
    }
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
    image::ImageEncoder::write_image(enc, &rgb, p.width(), p.height(), image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Encode as lossless WebP.
pub fn encode_webp(p: &Pixmap) -> Result<Vec<u8>, String> {
    let rgba = to_rgba8(p);
    let mut out = Vec::new();
    let enc = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
    image::ImageEncoder::write_image(enc, &rgba, p.width(), p.height(), image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod band_tests;
#[cfg(test)]
mod retained_tests;
#[cfg(test)]
mod tests;
