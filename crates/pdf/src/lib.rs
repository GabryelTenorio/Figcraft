//! PDF export (vector, via krilla).
//!
//! The walk mirrors the SVG exporter and the tiny-skia renderer: every node is drawn in page space
//! (the PDF page's origin is the top-left of the exported bounds).
//!
//! What stays vector:
//! - shapes, booleans, vector networks and frames: their outlines (`figcraft_render::node_outline`);
//! - solid fills and linear/radial gradients (with per-stop opacity), paint opacity and blend modes;
//! - strokes, as filled outlines (`figcraft_render::draw::stroke_area`), so Inside/Outside
//!   alignment, per-side weights, dashes and arrow caps are exact;
//! - clip content (a clip path), masks (alpha and luminance masks are soft masks drawn from the mask
//!   layer, so vector, gradient and image masks all work; vector masks clip to the mask's outline),
//!   node opacity and blend modes (isolated groups);
//! - text: real text in its embedded (subset) font, selectable and searchable, positioned glyph by
//!   glyph from `figcraft_text`'s layout. Fonts that can't be embedded, glyphs without a source
//!   character (list markers, ellipses), non-solid/gradient text fills and text strokes fall back
//!   to glyph outlines. If the PDF writer rejects a font, the whole export is retried with
//!   outlined text. Hyperlinked text becomes clickable link annotations (one per line and link).
//!
//! What is rasterised (at [`PdfOptions::raster_scale`], 2× by default):
//! - image fills are embedded as images (the original PNG/JPEG/GIF/WebP bytes when no image
//!   adjustments apply, else decoded pixels), placed with `figcraft_render::paint::image_transform`;
//! - angular and diamond gradients: an image of the paint clipped to the shape;
//! - drop and inner shadows: images of just the shadow (the renderer's, spread offsetting the
//!   outline), under/over the vector content;
//! - background blur and glass: the renderer draws them from a render of everything under the node
//!   on the page (its ancestors' fills included), placed under the node's content;
//! - layer blur, noise and texture: the whole node is rendered by `figcraft_render` and placed as
//!   one image.
//!
//! Pages are sized in points, one point per pixel of the document.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod draw;
mod raster;
mod text;

use figcraft_doc::{Document, NodeId};
use figcraft_geom::{Affine, Rect};

/// Export settings.
#[derive(Clone, Debug)]
pub struct PdfOptions {
    /// Draw text as glyph outlines instead of embedding fonts.
    pub outline_text: bool,
    /// Pixels per point for rasterised parts (effects, angular/diamond gradients).
    pub raster_scale: f64,
    /// Document title (metadata).
    pub title: Option<String>,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self { outline_text: false, raster_scale: 2.0, title: None }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PdfError {
    #[error("nothing to export")]
    Empty,
    #[error("no such node {0}")]
    NoNode(NodeId),
    #[error("node {0} has no exportable area")]
    NoBounds(NodeId),
    #[error("a {0} × {1} page is too large for PDF")]
    TooLarge(f64, f64),
    #[error("PDF writer: {0}")]
    Write(String),
}

/// Largest page side, in points (PDF 1.7 has no hard limit; viewers cope with this).
pub const MAX_PAGE_SIDE: f64 = 200_000.0;

/// One page per id, each sized to the node's export bounds (a clipping frame's box).
pub fn export_pdf(doc: &Document, pages: &[NodeId], opts: &PdfOptions) -> Result<Vec<u8>, PdfError> {
    let groups: Vec<Vec<NodeId>> = pages.iter().map(|p| vec![*p]).collect();
    export_groups(doc, &groups, opts)
}

/// All `ids` (in the given z-order) on one page sized to the union of their export bounds.
pub fn export_nodes_pdf(doc: &Document, ids: &[NodeId]) -> Result<Vec<u8>, PdfError> {
    export_groups(doc, &[ids.to_vec()], &PdfOptions::default())
}

/// One page per group of ids.
pub fn export_groups(doc: &Document, groups: &[Vec<NodeId>], opts: &PdfOptions) -> Result<Vec<u8>, PdfError> {
    let mut pages: Vec<(Vec<NodeId>, Rect)> = Vec::new();
    for g in groups.iter().filter(|g| !g.is_empty()) {
        let mut bounds: Option<Rect> = None;
        for id in g {
            if !doc.contains(*id) {
                return Err(PdfError::NoNode(*id));
            }
            if let Some(b) = figcraft_render::export_bounds(doc, *id).filter(|b| rect_finite(*b)) {
                bounds = Some(bounds.map_or(b, |a| a.union(b)));
            }
        }
        let first = g.first().copied().unwrap_or(NodeId(0));
        let b = bounds.ok_or(PdfError::NoBounds(first))?;
        if b.width() > MAX_PAGE_SIDE || b.height() > MAX_PAGE_SIDE {
            return Err(PdfError::TooLarge(b.width().round(), b.height().round()));
        }
        pages.push((g.clone(), b));
    }
    if pages.is_empty() {
        return Err(PdfError::Empty);
    }
    match write(doc, &pages, opts) {
        Err(e) if !opts.outline_text => {
            log::warn!("PDF export with embedded fonts failed ({e}); retrying with outlined text");
            write(doc, &pages, &PdfOptions { outline_text: true, ..opts.clone() })
        }
        r => r,
    }
}

/// Longest hyperlink kept (bytes).
const MAX_URL: usize = 8192;

/// A clickable area opening `url`, cut to the w×h page (`None` when nothing of it is on the page).
fn link_annotation(r: Rect, url: String, w: f64, h: f64) -> Option<krilla::annotation::Annotation> {
    use krilla::action::LinkAction;
    use krilla::annotation::{LinkAnnotation, Target};
    if !rect_finite(r) {
        return None;
    }
    let r = r.intersect(Rect::new(0.0, 0.0, w, h));
    let url = url.trim();
    if !(r.width() > 0.0 && r.height() > 0.0) || url.is_empty() || url.len() > MAX_URL {
        return None;
    }
    let rect = krilla::geom::Rect::from_ltrb(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)?;
    let target = Target::Action(LinkAction::new(url.to_string()).into());
    Some(LinkAnnotation::new(rect, target).into())
}

fn rect_finite(r: Rect) -> bool {
    r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite()
}

fn write(doc: &Document, pages: &[(Vec<NodeId>, Rect)], opts: &PdfOptions) -> Result<Vec<u8>, PdfError> {
    let mut pdf = krilla::Document::new();
    let mut meta = krilla::metadata::Metadata::new().creator("FigCraft".into()).producer("FigCraft".into());
    if let Some(t) = opts.title.as_ref().filter(|t| !t.trim().is_empty()) {
        meta = meta.title(t.clone());
    }
    pdf.set_metadata(meta);
    let mut ex = draw::Exporter::new(doc, opts);
    for (ids, b) in pages {
        // Zero-area nodes (a line, an empty group) still get a (tiny) page.
        let (w, h) = (b.width().max(1.0), b.height().max(1.0));
        let size = krilla::geom::Size::from_wh(w as f32, h as f32).ok_or(PdfError::TooLarge(w, h))?;
        let mut page = pdf.start_page_with(krilla::page::PageSettings::new(size));
        let mut s = page.surface();
        ex.page_origin = Affine::translate((-b.x0, -b.y0));
        ex.page_rect = *b;
        ex.page_ids = ids.clone();
        for id in ids {
            let parent_xf = doc.node(*id).and_then(|n| n.parent).map(|p| doc.world_transform(p)).unwrap_or(Affine::IDENTITY);
            ex.node(&mut s, *id, ex.page_origin * parent_xf, 0, false);
        }
        s.finish();
        for (r, url) in std::mem::take(&mut ex.links) {
            if let Some(a) = link_annotation(r, url, w, h) {
                page.add_annotation(a);
            }
        }
        page.finish();
    }
    pdf.finish().map_err(|e| PdfError::Write(format!("{e:?}")))
}

#[cfg(test)]
mod backdrop_tests;
#[cfg(test)]
mod effect_order_tests;
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod tests;
