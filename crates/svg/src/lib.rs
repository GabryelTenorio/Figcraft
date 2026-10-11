//! SVG export and import.
//!
//! Export writes one `<svg>` sized to the exported nodes' bounds. Shapes become `<path>`s with
//! their fills (solid, linear/radial gradients; angular and diamond fall back to their first
//! colour... or a raster pattern), strokes are written as filled outlines so Inside/Outside
//! alignment survives exactly, text is outlined, images are embedded as data URIs, and shadows
//! and blurs become SVG filters. Shadow spread offsets the outline like the renderer (the shape
//! stroked twice the spread wide with mitred joins, cut away with a mask to shrink); inner shadows
//! are masked, blurred rectangles clipped to the shape, over the fills and under the strokes.
//!
//! What SVG filters can't match is drawn by the renderer and embedded as PNG images, so it looks
//! like the canvas: noise grains over the vector layer (crisp, blended with the noise's blend
//! mode), background blur and glass from a render of what lies under the layer among the exported
//! layers, and layers with texture or a progressive layer blur whole.
//!
//! Import (via usvg) turns an SVG file into an editable frame: groups, shapes, text, gradients,
//! masks and effects (see [`import_svg_with_notes`]).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod import;
pub use import::{ImportError, Imported, import_svg, import_svg_with_notes, looks_like_svg};

use std::fmt::Write as _;

use base64::Engine as _;
use figcraft_color::{Color, GradientKind, Paint, PaintKind};
use figcraft_doc::{Document, EffectKind, Node, NodeId, NodeKind, SPREAD_MITER_LIMIT, Shadow};
use figcraft_geom::{Affine, BezPath, Rect};
use figcraft_render::shadow::Outline;
use figcraft_render::{Alone, Pixmap, View, backdrop};

/// Device pixels per page unit of embedded renders (blurs, glass, texture).
const RASTER_SCALE: f64 = 2.0;
/// Largest side of an embedded render, in pixels.
const MAX_RASTER_SIDE: f64 = 4096.0;

#[derive(Clone, Debug)]
pub struct SvgOptions {
    /// Include node ids as `id` attributes (Figma's "Include id attribute").
    pub include_ids: bool,
    /// Outline text (always true for now: no font embedding).
    pub outline_text: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self { include_ids: false, outline_text: true }
    }
}

struct W<'a> {
    doc: &'a Document,
    /// The exported layers, in z-order (what backdrops see).
    roots: &'a [NodeId],
    /// Page coordinates → SVG coordinates.
    page: Affine,
    defs: String,
    body: String,
    next: u32,
    opts: &'a SvgOptions,
    /// The mask layer being drawn as a mask's content (drawn like any layer, not skipped).
    mask: Option<NodeId>,
}

fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 1000.0).round() / 1000.0;
    let s = format!("{r}");
    if s == "-0" { "0".into() } else { s }
}

fn path_d(p: &BezPath) -> String {
    use figcraft_geom::kurbo::PathEl::*;
    let mut s = String::new();
    for el in p.elements() {
        match *el {
            MoveTo(a) => {
                let _ = write!(s, "M{} {}", num(a.x), num(a.y));
            }
            LineTo(a) => {
                let _ = write!(s, "L{} {}", num(a.x), num(a.y));
            }
            QuadTo(a, b) => {
                let _ = write!(s, "Q{} {} {} {}", num(a.x), num(a.y), num(b.x), num(b.y));
            }
            CurveTo(a, b, c) => {
                let _ = write!(s, "C{} {} {} {} {} {}", num(a.x), num(a.y), num(b.x), num(b.y), num(c.x), num(c.y));
            }
            ClosePath => s.push('Z'),
        }
    }
    s
}

fn matrix(a: Affine) -> String {
    let c = a.as_coeffs();
    format!("matrix({} {} {} {} {} {})", num(c[0]), num(c[1]), num(c[2]), num(c[3]), num(c[4]), num(c[5]))
}

/// A view of page area `b` at `k` device pixels per unit (fewer when that makes it larger than
/// [`MAX_RASTER_SIDE`] a side).
fn raster_view(b: Rect, k: f64) -> Option<View> {
    if !(b.width() > 0.0 && b.height() > 0.0 && [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) && k.is_finite() && k > 0.0) {
        return None;
    }
    let side = b.width().max(b.height()) * k;
    let k = if side > MAX_RASTER_SIDE { k * MAX_RASTER_SIDE / side } else { k };
    let (w, h) = ((b.width() * k).ceil().max(1.0), (b.height() * k).ceil().max(1.0));
    Some(View { origin: (b.x0, b.y0), scale: k, width: w as u32, height: h as u32 })
}

/// ` style="mix-blend-mode:…"` for a blend mode other than normal (else nothing).
fn blend_style(b: figcraft_color::BlendMode) -> String {
    if matches!(b, figcraft_color::BlendMode::Normal | figcraft_color::BlendMode::PassThrough) {
        String::new()
    } else {
        format!(" style=\"mix-blend-mode:{}\"", b.css())
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn hex(c: Color) -> String {
    format!("#{}", c.to_hex())
}

impl W<'_> {
    fn id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    /// `fill="..." [fill-opacity]` for a paint on a w×h box; `None` when it paints nothing.
    fn paint_attr(&mut self, p: &Paint, w: f64, h: f64) -> Option<String> {
        if !p.visible || p.opacity <= 0.0 {
            return None;
        }
        let blend = blend_style(p.blend);
        match &p.kind {
            PaintKind::Solid { color } => {
                let a = color.a * p.opacity;
                let op = if a < 0.999 { format!(" fill-opacity=\"{}\"", num(f64::from(a))) } else { String::new() };
                Some(format!("fill=\"{}\"{op}{blend}", hex(*color)))
            }
            PaintKind::Gradient(g) => {
                let gid = self.id("paint");
                let stops: String = g
                    .sorted_stops()
                    .iter()
                    .map(|s| {
                        let a = s.color.a * p.opacity;
                        let op = if a < 0.999 { format!(" stop-opacity=\"{}\"", num(f64::from(a))) } else { String::new() };
                        format!("<stop offset=\"{}\" stop-color=\"{}\"{op}/>", num(f64::from(s.position)), hex(s.color))
                    })
                    .collect();
                let (f, t, k) = ((g.from.0 * w, g.from.1 * h), (g.to.0 * w, g.to.1 * h), (g.width.0 * w, g.width.1 * h));
                match g.kind {
                    GradientKind::Linear => {
                        let _ = write!(
                            self.defs,
                            "<linearGradient id=\"{gid}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" gradientUnits=\"userSpaceOnUse\">{stops}</linearGradient>",
                            num(f.0),
                            num(f.1),
                            num(t.0),
                            num(t.1)
                        );
                    }
                    _ => {
                        // Unit circle mapped onto the handle ellipse.
                        let m = Affine::new([t.0 - f.0, t.1 - f.1, k.0 - f.0, k.1 - f.1, f.0, f.1]);
                        let _ = write!(
                            self.defs,
                            "<radialGradient id=\"{gid}\" cx=\"0\" cy=\"0\" r=\"1\" gradientUnits=\"userSpaceOnUse\" gradientTransform=\"{}\">{stops}</radialGradient>",
                            matrix(m)
                        );
                    }
                }
                Some(format!("fill=\"url(#{gid})\"{blend}"))
            }
            PaintKind::Image(img) => {
                let data = self.doc.images.get(&img.image)?;
                let pid = self.id("pattern");
                let iid = self.id("image");
                let mime = sniff_mime(&data.bytes);
                let b64 = base64::engine::general_purpose::STANDARD.encode(data.bytes.as_slice());
                let t = figcraft_render::paint::image_transform(img, f64::from(data.width), f64::from(data.height), w, h);
                let m = Affine::new([f64::from(t.sx), f64::from(t.ky), f64::from(t.kx), f64::from(t.sy), f64::from(t.tx), f64::from(t.ty)]);
                let (pw, ph) = if img.scale_mode == figcraft_color::ImageScaleMode::Tile {
                    (f64::from(data.width), f64::from(data.height))
                } else {
                    (f64::from(data.width) * 4.0, f64::from(data.height) * 4.0)
                };
                let _ = write!(
                    self.defs,
                    "<pattern id=\"{pid}\" patternUnits=\"userSpaceOnUse\" width=\"{}\" height=\"{}\" patternTransform=\"{}\"><image id=\"{iid}\" width=\"{}\" height=\"{}\" href=\"data:{mime};base64,{b64}\"/></pattern>",
                    num(pw),
                    num(ph),
                    matrix(m),
                    data.width,
                    data.height
                );
                let op = if p.opacity < 0.999 { format!(" fill-opacity=\"{}\"", num(f64::from(p.opacity))) } else { String::new() };
                Some(format!("fill=\"url(#{pid})\"{op}{blend}"))
            }
            PaintKind::Pattern(pt) => {
                let (tiles, cell) = self.pattern_tiles(pt, w, h)?;
                let pid = self.id("pattern");
                let _ = write!(
                    self.defs,
                    "<pattern id=\"{pid}\" patternUnits=\"userSpaceOnUse\" width=\"{}\" height=\"{}\" patternTransform=\"translate({} {})\">{tiles}</pattern>",
                    num(cell.size.0),
                    num(cell.size.1),
                    num(cell.origin.0),
                    num(cell.origin.1)
                );
                let op = if p.opacity < 0.999 { format!(" fill-opacity=\"{}\"", num(f64::from(p.opacity))) } else { String::new() };
                Some(format!("fill=\"url(#{pid})\"{op}{blend}"))
            }
        }
    }

    /// One cell of pattern `pt` on a `w`×`h` box: the source's vectors at each tile, or (a source
    /// with effects, which draw as page-placed images) an image of the cell.
    fn pattern_tiles(&mut self, pt: &figcraft_color::PatternPaint, w: f64, h: f64) -> Option<(String, figcraft_color::PatternCell)> {
        let doc = self.doc;
        let src = doc.node(NodeId(pt.source)).filter(|n| !n.is_page())?;
        if doc.subtree(src.id).iter().filter_map(|i| doc.node(*i)).any(|n| n.effects.iter().any(|e| e.visible)) {
            let (pix, cell, _) = figcraft_render::pattern::cell_pixmap(doc, pt, w, h, RASTER_SCALE)?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(figcraft_render::encode_png(&pix).ok()?);
            let img = format!(
                "<image width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{b64}\"/>",
                num(cell.size.0),
                num(cell.size.1)
            );
            return Some((img, cell));
        }
        let cell = pt.cell(src.width, src.height, w, h)?;
        let unrotated = src.transform.inverse();
        let page_body = std::mem::take(&mut self.body);
        let _ = figcraft_render::pattern::nested(|| {
            for (tx, ty) in &cell.tiles {
                self.node(src.id, Affine::translate((*tx, *ty)) * Affine::scale(cell.scale) * unrotated, 0);
            }
            Some(())
        });
        Some((std::mem::replace(&mut self.body, page_body), cell))
    }

    /// The layer's filter: drop shadows (but those drawn as `exact` geometry: see
    /// [`W::drop_shadows`]) and layer blur.
    fn filter_for(&mut self, n: &Node, exact: bool) -> Option<String> {
        let fx: Vec<&figcraft_doc::Effect> = n
            .effects
            .iter()
            .filter(|e| e.visible)
            .filter(|e| !(exact && matches!(&e.kind, EffectKind::DropShadow(s) if s.spread != 0.0)))
            .filter(|e| matches!(e.kind, EffectKind::DropShadow(_) | EffectKind::LayerBlur { .. }))
            .collect();
        if fx.is_empty() {
            return None;
        }
        let fid = self.id("filter");
        let mut f = format!("<filter id=\"{fid}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\" color-interpolation-filters=\"sRGB\">");
        let mut merge = Vec::new();
        for (i, e) in fx.iter().enumerate() {
            match &e.kind {
                EffectKind::DropShadow(s) => {
                    // Spread here (layers that aren't one outline) grows the pixels, like the renderer.
                    let morph = if s.spread != 0.0 {
                        format!(
                            "<feMorphology in=\"SourceAlpha\" operator=\"{}\" radius=\"{}\" result=\"s{i}m\"/>",
                            if s.spread > 0.0 { "dilate" } else { "erode" },
                            num(s.spread.abs())
                        )
                    } else {
                        String::new()
                    };
                    let src = if morph.is_empty() { "SourceAlpha".to_string() } else { format!("s{i}m") };
                    let _ = write!(
                        f,
                        "{morph}<feOffset in=\"{src}\" dx=\"{}\" dy=\"{}\"/><feGaussianBlur stdDeviation=\"{}\"/><feColorMatrix type=\"matrix\" values=\"0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} 0 0 0 {} 0\" result=\"s{i}\"/>",
                        num(s.offset.0),
                        num(s.offset.1),
                        num(s.radius / 2.0),
                        num(f64::from(s.color.r)),
                        num(f64::from(s.color.g)),
                        num(f64::from(s.color.b)),
                        num(f64::from(s.color.a))
                    );
                    merge.push(format!("s{i}"));
                }
                EffectKind::LayerBlur(b) => {
                    let _ = write!(f, "<feGaussianBlur in=\"SourceGraphic\" stdDeviation=\"{}\" result=\"blur\"/>", num(b.sanitized().radius / 2.0));
                }
                _ => {}
            }
        }
        let has_blur = fx.iter().any(|e| matches!(e.kind, EffectKind::LayerBlur { .. }));
        f.push_str("<feMerge>");
        for m in &merge {
            let _ = write!(f, "<feMergeNode in=\"{m}\"/>");
        }
        let _ = write!(f, "<feMergeNode in=\"{}\"/></feMerge></filter>", if has_blur { "blur" } else { "SourceGraphic" });
        self.defs.push_str(&f);
        Some(fid)
    }

    fn node(&mut self, id: NodeId, xf: Affine, depth: usize) {
        if depth > 256 {
            return;
        }
        let Some(n) = self.doc.node(id) else { return };
        if !n.visible || (n.is_mask && self.mask != Some(id)) {
            return;
        }
        let xf = xf * n.transform;
        let id_attr = if self.opts.include_ids { format!(" id=\"{}\"", esc(&n.name.replace(' ', "_"))) } else { String::new() };
        if self.raster_whole(id, n, &id_attr) {
            return;
        }
        let mut attrs = id_attr;
        if n.opacity < 0.999 {
            let _ = write!(attrs, " opacity=\"{}\"", num(f64::from(n.opacity)));
        }
        let grains = self.grain_images(id, n);
        let blend = blend_style(n.blend);
        if !blend.is_empty() {
            attrs.push_str(&blend);
        } else if !grains.is_empty() {
            // Grains blend with the layer only, like the canvas's isolated layer.
            attrs.push_str(" style=\"isolation:isolate\"");
        }
        let spreads = n.effects.iter().any(|e| e.visible && matches!(&e.kind, EffectKind::DropShadow(s) if s.spread != 0.0));
        let sil = if spreads { figcraft_render::shadow::silhouette(self.doc, n) } else { None };
        if let Some(sil) = &sil {
            self.drop_shadows(n, sil, xf);
        }
        if let Some(fid) = self.filter_for(n, sil.is_some()) {
            let _ = write!(attrs, " filter=\"url(#{fid})\"");
        }
        let _ = write!(self.body, "<g{attrs}>");
        if backdrop::has_effects(n)
            && let Some(img) = self.backdrop_image(id, None)
        {
            self.body.push_str(&img);
        }
        let (w, h) = (n.width, n.height);
        let outline = match &n.kind {
            NodeKind::Group | NodeKind::Page { .. } => None,
            _ => figcraft_render::node_outline(self.doc, n),
        };
        let closed = figcraft_doc::geometry::is_closed(n) || matches!(n.kind, NodeKind::BooleanOp { .. } | NodeKind::Text(_));
        if let Some((path, rule)) = &outline
            && closed
        {
            let fr = if *rule == figcraft_render::FillRule::EvenOdd { " fill-rule=\"evenodd\"" } else { "" };
            for p in &n.fills {
                if let Some(a) = self.paint_attr(p, w, h) {
                    let _ = write!(self.body, "<path transform=\"{}\" d=\"{}\" {a}{fr}/>", matrix(xf), path_d(path));
                }
            }
        }
        // Inner shadows paint over the fills, under the children and strokes.
        if let Some(o) = &outline {
            for e in n.effects.iter().filter(|e| e.visible) {
                if let EffectKind::InnerShadow(s) = &e.kind {
                    self.inner_shadow(o, s, xf);
                }
            }
        }
        if !matches!(n.kind, NodeKind::BooleanOp { .. }) && !n.children.is_empty() {
            let clip = n.clip && n.kind.is_frame_like();
            if clip && let Some((p, _)) = &outline {
                let cid = self.id("clip");
                let mut cp = p.clone();
                cp.apply_affine(xf);
                let _ = write!(self.defs, "<clipPath id=\"{cid}\"><path d=\"{}\"/></clipPath>", path_d(&cp));
                let _ = write!(self.body, "<g clip-path=\"url(#{cid})\">");
            }
            let kids = n.children.clone();
            let mut i = 0;
            while i < kids.len() {
                let Some(&c) = kids.get(i) else { break };
                if self.doc.node(c).is_some_and(|cn| cn.is_mask && cn.visible) {
                    let kind = self.doc.node(c).map(|mn| mn.mask_type).unwrap_or_default();
                    if kind == figcraft_doc::MaskType::Vector {
                        // A vector mask is a clip path: its outline.
                        let cid = self.id("clip");
                        let outline =
                            self.doc.node(c).and_then(|mn| Some((xf * mn.transform, figcraft_render::subtree_outline(self.doc, mn, depth + 1)?)));
                        let (shape, cr) = match &outline {
                            Some((mxf, (mp, rule))) => (
                                format!(" transform=\"{}\" d=\"{}\"", matrix(*mxf), path_d(mp)),
                                if *rule == figcraft_render::FillRule::EvenOdd { " clip-rule=\"evenodd\"" } else { "" },
                            ),
                            None => (" d=\"\"".to_string(), ""),
                        };
                        let _ = write!(self.defs, "<clipPath id=\"{cid}\"><path{shape}{cr}/></clipPath>");
                        let _ = write!(self.body, "<g clip-path=\"url(#{cid})\">");
                    } else {
                        // The layer as drawn: its alpha or luminance masks.
                        let mid = self.id("mask");
                        let saved = std::mem::take(&mut self.body);
                        let outer = self.mask.replace(c);
                        self.node(c, xf, depth + 1);
                        self.mask = outer;
                        let mask_body = std::mem::replace(&mut self.body, saved);
                        let alpha = if kind == figcraft_doc::MaskType::Alpha { " style=\"mask-type:alpha\"" } else { "" };
                        let _ = write!(self.defs, "<mask id=\"{mid}\" maskUnits=\"userSpaceOnUse\"{alpha}>{mask_body}</mask>");
                        let _ = write!(self.body, "<g mask=\"url(#{mid})\">");
                    }
                    let mut j = i + 1;
                    while let Some(&k) = kids.get(j) {
                        if self.doc.node(k).is_some_and(|kn| kn.is_mask) {
                            break;
                        }
                        self.node(k, xf, depth + 1);
                        j += 1;
                    }
                    self.body.push_str("</g>");
                    i = j;
                    continue;
                }
                self.node(c, xf, depth + 1);
                i += 1;
            }
            if clip && outline.is_some() {
                self.body.push_str("</g>");
            }
        }
        if n.has_visible_stroke() {
            let geom = match &n.kind {
                NodeKind::BooleanOp { .. } | NodeKind::Text(_) => outline.as_ref().map(|(p, _)| p.clone()),
                _ => figcraft_doc::geometry::stroke_geometry(n),
            };
            if let Some(g) = geom {
                let area = figcraft_render::draw::stroke_area(n, &g, closed);
                let even = n.stroke.sides.is_some() || (n.stroke.dashes.is_empty() && closed && n.stroke.align != figcraft_doc::StrokeAlign::Center);
                let fr = if even { " fill-rule=\"evenodd\"" } else { "" };
                for p in &n.strokes {
                    if let Some(a) = self.paint_attr(p, w, h) {
                        let _ = write!(self.body, "<path transform=\"{}\" d=\"{}\" {a}{fr}/>", matrix(xf), path_d(&area));
                    }
                }
            }
        }
        self.body.push_str(&grains);
        self.body.push_str("</g>");
    }

    /// `<image>` markup of `pix`, a render of `view`'s page area, with `attrs`; `None` when it is
    /// empty.
    fn image(&self, pix: &Pixmap, view: &View, attrs: &str) -> Option<String> {
        if pix.data().iter().skip(3).step_by(4).all(|a| *a == 0) {
            return None;
        }
        let png = figcraft_render::encode_png(pix).ok()?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let t = self.page * Affine::translate(view.origin) * Affine::scale(1.0 / view.scale.max(1e-9));
        Some(format!(
            "<image transform=\"{}\" width=\"{}\" height=\"{}\"{attrs} href=\"data:image/png;base64,{b64}\"/>",
            matrix(t),
            pix.width(),
            pix.height()
        ))
    }

    /// Node `id`'s background blurs and glass as an image over its shape, drawn by the renderer
    /// from what lies under it among the exported layers (`opacity`: the layer's, when the image
    /// isn't inside its group).
    fn backdrop_image(&self, id: NodeId, opacity: Option<f32>) -> Option<String> {
        let view = raster_view(backdrop::shape_bounds(self.doc, id)?, RASTER_SCALE)?;
        let pix = backdrop::effects_beneath(self.doc, self.roots, id, &view, None)?;
        let op = opacity.filter(|o| *o < 0.999).map(|o| format!(" opacity=\"{}\"", num(f64::from(o)))).unwrap_or_default();
        self.image(&pix, &view, &op)
    }

    /// Layers whose look SVG can't build (texture scatters their pixels, a progressive blur varies
    /// across them, noise over a blur depends on their order) are drawn by the renderer and
    /// embedded whole, over their backdrop effects. False when the layer isn't one of them (or
    /// can't be rendered): it is written as vectors.
    fn raster_whole(&mut self, id: NodeId, n: &Node, id_attr: &str) -> bool {
        let has = |f: &dyn Fn(&EffectKind) -> bool| n.effects.iter().any(|e| e.visible && f(&e.kind));
        let whole = has(&|k| matches!(k, EffectKind::Texture(_)))
            || has(&|k| matches!(k, EffectKind::LayerBlur(b) if b.is_progressive()))
            || (has(&|k| matches!(k, EffectKind::Noise(_))) && has(&|k| matches!(k, EffectKind::LayerBlur(_))));
        if !whole {
            return false;
        }
        let Some(view) = figcraft_render::raster_bounds(self.doc, id).and_then(|b| raster_view(b, RASTER_SCALE)) else { return false };
        // The renderer applies the layer's opacity; its blend mode applies to the SVG beneath.
        let Some(img) = figcraft_render::render_alone(self.doc, id, &view, Alone::WithEffects).and_then(|p| self.image(&p, &view, "")) else {
            return false;
        };
        let back = if backdrop::has_effects(n) { self.backdrop_image(id, Some(n.opacity.clamp(0.0, 1.0))) } else { None };
        let _ = write!(self.body, "<g{id_attr}{}>{}{img}</g>", blend_style(n.blend), back.unwrap_or_default());
        true
    }

    /// Node `id`'s noise grains as images over the layer (crisp pixels, the noise's blend mode),
    /// drawn by the renderer from the layer's content (empty without noise).
    fn grain_images(&self, id: NodeId, n: &Node) -> String {
        let noises: Vec<&figcraft_doc::Noise> = n
            .effects
            .iter()
            .filter(|e| e.visible)
            .filter_map(|e| match &e.kind {
                EffectKind::Noise(nz) => Some(nz),
                _ => None,
            })
            .collect();
        if noises.is_empty() {
            return String::new();
        }
        // At least two pixels a grain (grains are twice the noise size across), up to 4 per unit.
        let world = self.doc.world_transform(id);
        let sc = world.determinant().abs().sqrt();
        let cell = noises.iter().map(|nz| nz.grain().0.min(nz.grain().1) * 2.0).fold(f64::INFINITY, f64::min) * sc;
        let k = if cell.is_finite() && cell > 0.0 { (2.0 / cell).clamp(RASTER_SCALE, 4.0) } else { RASTER_SCALE };
        let Some(view) = self.doc.world_render_bounds(id).and_then(|b| raster_view(b, k)) else { return String::new() };
        let Some(layer) = figcraft_render::render_alone(self.doc, id, &view, Alone::Plain) else { return String::new() };
        let xf = view.transform() * world;
        let mut out = String::new();
        for nz in noises {
            let blend = blend_style(nz.blend);
            let blend = blend.strip_prefix(" style=\"").and_then(|b| b.strip_suffix('"')).map(|b| format!(";{b}")).unwrap_or_default();
            let attrs = format!(" style=\"image-rendering:pixelated{blend}\"");
            if let Some(img) = figcraft_render::grain::grains(&layer, nz, xf, id.0 as u32).and_then(|g| self.image(&g, &view, &attrs)) {
                out.push_str(&img);
            }
        }
        out
    }

    /// Markup painting `outlines` (local, through `xf`) grown (`spread > 0`) or shrunk by `spread`
    /// local units in opaque `color`: the outline filled and stroked twice the spread wide with
    /// mitred joins (shrinking: that band masked away), like the renderer's spread.
    fn spread_paths(&mut self, outlines: &[Outline], spread: f64, xf: Affine, color: &str) -> String {
        let paths = |fill: &str, stroke: Option<(&str, f64)>| -> String {
            let mut s = String::new();
            for (p, rule) in outlines {
                let fr = if *rule == figcraft_render::FillRule::EvenOdd { " fill-rule=\"evenodd\"" } else { "" };
                let _ = write!(s, "<path transform=\"{}\" d=\"{}\" fill=\"{fill}\"{fr}/>", matrix(xf), path_d(p));
            }
            if let Some((c, half)) = stroke {
                for (p, _) in outlines {
                    let _ = write!(
                        s,
                        "<path transform=\"{}\" d=\"{}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"{}\" stroke-linejoin=\"miter\" stroke-miterlimit=\"{}\"/>",
                        matrix(xf),
                        path_d(p),
                        num(half * 2.0),
                        num(SPREAD_MITER_LIMIT)
                    );
                }
            }
            s
        };
        let half = if spread.is_finite() { spread.abs() } else { 0.0 };
        if spread >= 0.0 || half <= 1e-6 {
            return paths(color, (half > 1e-6).then_some((color, half)));
        }
        let bounds =
            outlines.iter().map(|(p, _)| figcraft_geom::transform_rect_bbox(xf, figcraft_geom::Shape::bounding_box(p))).reduce(|a, b| a.union(b));
        let Some(b) = bounds.map(|b| b.inflate(1.0, 1.0)) else { return String::new() };
        let mid = self.id("mask");
        let _ = write!(
            self.defs,
            "<mask id=\"{mid}\" maskUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}</mask>",
            num(b.x0),
            num(b.y0),
            num(b.width()),
            num(b.height()),
            paths("white", Some(("black", half)))
        );
        format!("<g mask=\"url(#{mid})\">{}</g>", paths(color, None))
    }

    /// A Gaussian blur filter over `region` (page units); `None` for no blur.
    fn blur_filter(&mut self, radius: f64, region: Rect) -> Option<String> {
        if !(radius.is_finite() && radius > 0.0) {
            return None;
        }
        let fid = self.id("filter");
        let _ = write!(
            self.defs,
            "<filter id=\"{fid}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
            num(region.x0),
            num(region.y0),
            num(region.width()),
            num(region.height()),
            num(radius / 2.0)
        );
        Some(fid)
    }

    /// Drop shadows with spread, as geometry under the layer: its silhouette offset by the
    /// spread (exactly, like the renderer), moved, blurred and faded.
    fn drop_shadows(&mut self, n: &Node, sil: &[Outline], xf: Affine) {
        for e in n.effects.iter().filter(|e| e.visible) {
            let EffectKind::DropShadow(s) = &e.kind else { continue };
            if s.spread == 0.0 || !(s.offset.0.is_finite() && s.offset.1.is_finite()) {
                continue;
            }
            let at = Affine::translate(s.offset) * xf;
            let Some(b) =
                sil.iter().map(|(p, _)| figcraft_geom::transform_rect_bbox(at, figcraft_geom::Shape::bounding_box(p))).reduce(|a, b| a.union(b))
            else {
                continue;
            };
            let reach = s.spread.abs() * SPREAD_MITER_LIMIT + s.radius.abs() * 1.5 + 2.0;
            let filter = self.blur_filter(s.radius, b.inflate(reach, reach)).map(|f| format!(" filter=\"url(#{f})\"")).unwrap_or_default();
            let op = f64::from(s.color.a * n.opacity.clamp(0.0, 1.0));
            let blend = if matches!(s.blend, figcraft_color::BlendMode::Normal | figcraft_color::BlendMode::PassThrough) {
                String::new()
            } else {
                format!(" style=\"mix-blend-mode:{}\"", s.blend.css())
            };
            let body = self.spread_paths(sil, s.spread, at, &hex(s.color));
            let _ = write!(self.body, "<g opacity=\"{}\"{blend}><g{filter}>{body}</g></g>", num(op));
        }
    }

    /// An inner shadow over the layer: everything outside its outline shrunk by the spread and
    /// moved, blurred, tinted and clipped to the outline (the renderer's inner shadow).
    fn inner_shadow(&mut self, o: &Outline, s: &Shadow, xf: Affine) {
        if !(s.offset.0.is_finite() && s.offset.1.is_finite()) {
            return;
        }
        let b = figcraft_geom::transform_rect_bbox(xf, figcraft_geom::Shape::bounding_box(&o.0));
        let reach = s.spread.abs() * SPREAD_MITER_LIMIT + s.radius.abs() * 1.5 + s.offset.0.abs().max(s.offset.1.abs()) + 2.0;
        let r = b.inflate(reach, reach);
        let cr = if o.1 == figcraft_render::FillRule::EvenOdd { " clip-rule=\"evenodd\"" } else { "" };
        let cid = self.id("clip");
        let _ = write!(self.defs, "<clipPath id=\"{cid}\"><path transform=\"{}\" d=\"{}\"{cr}/></clipPath>", matrix(xf), path_d(&o.0));
        let hole = self.spread_paths(std::slice::from_ref(o), -s.spread, Affine::translate(s.offset) * xf, "black");
        let mid = self.id("mask");
        let rect = |fill: &str| {
            format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\"/>", num(r.x0), num(r.y0), num(r.width()), num(r.height()))
        };
        let _ = write!(
            self.defs,
            "<mask id=\"{mid}\" maskUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}{hole}</mask>",
            num(r.x0),
            num(r.y0),
            num(r.width()),
            num(r.height()),
            rect("white")
        );
        let filter = self.blur_filter(s.radius, r).map(|f| format!(" filter=\"url(#{f})\"")).unwrap_or_default();
        let _ = write!(
            self.body,
            "<g clip-path=\"url(#{cid})\"><g opacity=\"{}\"{filter}><g mask=\"url(#{mid})\">{}</g></g></g>",
            num(f64::from(s.color.a)),
            rect(&hex(s.color))
        );
    }
}

fn sniff_mime(b: &[u8]) -> &'static str {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if b.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if b.starts_with(b"GIF") {
        "image/gif"
    } else if b.len() > 12 && b.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

/// Export `ids` (in z-order) as one SVG document covering their export bounds.
pub fn export_svg(doc: &Document, ids: &[NodeId], opts: &SvgOptions) -> Option<String> {
    let bounds: Rect = ids.iter().filter_map(|i| figcraft_render::export_bounds(doc, *i)).reduce(|a, b| a.union(b))?;
    let origin = Affine::translate((-bounds.x0, -bounds.y0));
    let mut w = W { doc, roots: ids, page: origin, defs: String::new(), body: String::new(), next: 0, opts, mask: None };
    for id in ids {
        let parent_xf = doc.node(*id).and_then(|n| n.parent).map(|p| doc.world_transform(p)).unwrap_or(Affine::IDENTITY);
        w.node(*id, origin * parent_xf, 0);
    }
    let (bw, bh) = (bounds.width().max(0.0), bounds.height().max(0.0));
    let defs = if w.defs.is_empty() { String::new() } else { format!("<defs>{}</defs>", w.defs) };
    Some(format!(
        "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" fill=\"none\" xmlns=\"http://www.w3.org/2000/svg\">{}{}</svg>\n",
        num(bw),
        num(bh),
        num(bw),
        num(bh),
        w.body,
        defs
    ))
}

#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod mask_export_tests;
#[cfg(test)]
mod raster_fx_tests;
#[cfg(test)]
mod tests;
