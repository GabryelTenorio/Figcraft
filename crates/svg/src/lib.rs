//! SVG export and import.
//!
//! Export writes one `<svg>` sized to the exported nodes' bounds. Shapes become `<path>`s with
//! their fills (solid, linear/radial gradients; angular and diamond fall back to their first
//! colour... or a raster pattern), strokes are written as filled outlines so Inside/Outside
//! alignment survives exactly, text is outlined, images are embedded as data URIs, and shadows
//! and blurs become SVG filters. Shadow spread offsets the outline like the renderer (the shape
//! stroked twice the spread wide with mitred joins, cut away with a mask to shrink); inner shadows
//! are masked, blurred rectangles clipped to the shape.
//!
//! Import (via usvg) turns an SVG file into a frame of vector nodes.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod import;
pub use import::{ImportError, import_svg};

use std::fmt::Write as _;

use base64::Engine as _;
use figcraft_color::{Color, GradientKind, Paint, PaintKind};
use figcraft_doc::{Document, EffectKind, Node, NodeId, NodeKind, SPREAD_MITER_LIMIT, Shadow};
use figcraft_geom::{Affine, BezPath, Rect};
use figcraft_render::shadow::Outline;

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
    defs: String,
    body: String,
    next: u32,
    opts: &'a SvgOptions,
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
        let blend = if matches!(p.blend, figcraft_color::BlendMode::Normal | figcraft_color::BlendMode::PassThrough) {
            String::new()
        } else {
            format!(" style=\"mix-blend-mode:{}\"", p.blend.css())
        };
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
        }
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
                EffectKind::LayerBlur { radius } => {
                    let _ = write!(f, "<feGaussianBlur in=\"SourceGraphic\" stdDeviation=\"{}\" result=\"blur\"/>", num(radius / 2.0));
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
        if !n.visible || n.is_mask {
            return;
        }
        let xf = xf * n.transform;
        let mut attrs = String::new();
        if self.opts.include_ids {
            let _ = write!(attrs, " id=\"{}\"", esc(&n.name.replace(' ', "_")));
        }
        if n.opacity < 0.999 {
            let _ = write!(attrs, " opacity=\"{}\"", num(f64::from(n.opacity)));
        }
        if !matches!(n.blend, figcraft_color::BlendMode::Normal | figcraft_color::BlendMode::PassThrough) {
            let _ = write!(attrs, " style=\"mix-blend-mode:{}\"", n.blend.css());
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
                    let mid = self.id("mask");
                    let saved = std::mem::take(&mut self.body);
                    if let Some(mn) = self.doc.node(c) {
                        let mxf = xf * mn.transform;
                        if let Some((mp, _)) = figcraft_render::node_outline(self.doc, mn) {
                            let _ = write!(self.body, "<path transform=\"{}\" d=\"{}\" fill=\"white\"/>", matrix(mxf), path_d(&mp));
                        }
                    }
                    let mask_body = std::mem::replace(&mut self.body, saved);
                    let _ = write!(self.defs, "<mask id=\"{mid}\" maskUnits=\"userSpaceOnUse\">{mask_body}</mask>");
                    let _ = write!(self.body, "<g mask=\"url(#{mid})\">");
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
        if let Some(o) = &outline {
            for e in n.effects.iter().filter(|e| e.visible) {
                if let EffectKind::InnerShadow(s) = &e.kind {
                    self.inner_shadow(o, s, xf);
                }
            }
        }
        self.body.push_str("</g>");
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
    let mut w = W { doc, defs: String::new(), body: String::new(), next: 0, opts };
    let origin = Affine::translate((-bounds.x0, -bounds.y0));
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
mod tests;
