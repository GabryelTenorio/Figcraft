//! Dev-mode inspection: a layer's properties grouped for handoff (layout, auto layout,
//! typography, fills, strokes, effects), each with the style or variable behind it when there is
//! one. The Inspect tab shows these, and `inspect.properties` returns them.

use figcraft_color::{Color, GradientKind, PaintKind};
use figcraft_doc::*;
use serde::Serialize;
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::*;
use crate::spec;

/// One group of properties ("Layout", "Typography"…).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Section {
    pub title: &'static str,
    pub rows: Vec<Prop>,
}

/// One property: its label and value as shown, an optional colour swatch (hex with alpha) and
/// the style or variable name it comes from.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Prop {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// A layer in the inspected selection that has export settings.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Asset {
    pub id: NodeId,
    pub name: String,
    /// Its export settings as shown ("PNG 2x", "SVG 1x"…).
    pub exports: Vec<String>,
}

/// Most assets listed for one selection.
const MAX_ASSETS: usize = 200;

/// The layers in and under `ids` that have export settings, in layer order.
pub fn assets(doc: &Document, ids: &[NodeId]) -> Vec<Asset> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = ids.iter().rev().copied().collect();
    // Bounded: the Inspect tab asks every frame.
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited += 1;
        if out.len() >= MAX_ASSETS || visited > 20_000 {
            break;
        }
        let Some(n) = doc.node(id) else { continue };
        if !n.exports.is_empty() && !out.iter().any(|a: &Asset| a.id == id) {
            let exports = n.exports.iter().map(|e| format!("{} {}", e.format.label(), e.size.label())).collect();
            out.push(Asset { id, name: n.name.clone(), exports });
        }
        stack.extend(n.children.iter().rev().copied());
    }
    out
}

/// A frame or section with a handoff status, for Dev Mode's list of what's ready.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DevItem {
    pub id: NodeId,
    pub name: String,
    pub page: NodeId,
    pub status: DevStatus,
}

/// Most items listed.
const MAX_DEV_ITEMS: usize = 500;

/// Whether `id` can carry a handoff status: a section, or a frame at the top of a page or section.
pub fn has_status(doc: &Document, id: NodeId) -> bool {
    let section = |n: &Node| matches!(n.kind, NodeKind::Section { .. });
    doc.node(id).is_some_and(|n| {
        section(n)
            || (n.kind.is_frame_like()
                && !matches!(n.kind, NodeKind::Instance { .. })
                && n.parent.and_then(|p| doc.node(p)).is_some_and(|p| p.is_page() || section(p)))
    })
}

/// The layers with handoff status `status` (`None`: any status) on `page` (`None`: every page),
/// in page and layer order (bounded).
pub fn dev_items(doc: &Document, status: Option<DevStatus>, page: Option<NodeId>) -> Vec<DevItem> {
    let mut out = Vec::new();
    let mut visited = 0usize;
    for pg in doc.pages.iter().copied().filter(|pg| page.is_none_or(|p| p == *pg)) {
        let mut stack = vec![pg];
        while let Some(id) = stack.pop() {
            visited += 1;
            if out.len() >= MAX_DEV_ITEMS || visited > 200_000 {
                return out;
            }
            let Some(n) = doc.node(id) else { continue };
            if !n.dev_status.is_none() && status.is_none_or(|s| s == n.dev_status) {
                out.push(DevItem { id, name: n.name.clone(), page: pg, status: n.dev_status });
            }
            stack.extend(n.children.iter().rev().copied());
        }
    }
    out
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "inspect.readyForDev",
            "Ready for dev",
            "{status?: readyForDev|completed|any (default readyForDev), page? (page id; default every page)} → {items: [{id, name, page, status}]}  frames and sections by handoff status (set it with `node.set {devStatus}`)",
            false,
            |s, p| {
                let cmd = "inspect.readyForDev";
                let status = match string(p, "status").as_deref() {
                    None => Some(DevStatus::ReadyForDev),
                    Some(v) if v.eq_ignore_ascii_case("any") || v.eq_ignore_ascii_case("all") => None,
                    Some(v) => Some(
                        DevStatus::ALL
                            .into_iter()
                            .find(|d| !d.is_none() && d.id().eq_ignore_ascii_case(v))
                            .ok_or_else(|| crate::bad(cmd, "status: readyForDev|completed|any"))?,
                    ),
                };
                let doc = s.doc()?;
                let page = match get(p, "page") {
                    Some(v) => Some(id_of(v).filter(|pg| doc.pages.contains(pg)).ok_or_else(|| crate::bad(cmd, "page: a page id"))?),
                    None => None,
                };
                Ok(json!({"items": serde_json::to_value(dev_items(doc, status, page)).unwrap_or(Value::Null)}))
            }
        ),
        spec!(
            "inspect.assets",
            "Inspect assets",
            "{ids? (default the selection)} → {assets: [{id, name, exports}]}  the layers in and under them with export settings (export them with `file.exportLayers`)",
            false,
            |s, p| {
                let ids = targets(s, p);
                Ok(json!({"assets": serde_json::to_value(assets(s.doc()?, &ids)).unwrap_or(Value::Null)}))
            }
        ),
        spec!(
            "inspect.properties",
            "Inspect properties",
            "{id?} → {sections: [{title, rows: [{label, value, color?, token?}]}]}  layout (relative to the parent), auto layout, typography, fills, strokes and effects, with style and variable names",
            false,
            |s, p| {
                let id = match id(p, "id") {
                    Some(i) => i,
                    None => s.selection().first().copied().ok_or_else(|| crate::bad("inspect.properties", "select a layer or pass `id`"))?,
                };
                let doc = s.doc()?;
                doc.node(id).ok_or_else(|| crate::bad("inspect.properties", "no such layer"))?;
                Ok(json!({"sections": serde_json::to_value(properties(doc, id)).unwrap_or(Value::Null)}))
            }
        ),
    ]
}

/// A number as a designer reads it: at most two decimals, no trailing zeros.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{:.2}", (v * 100.0).round() / 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// `#RRGGBB` plus the opacity when it isn't 100%.
fn color_text(c: Color) -> String {
    if c.a < 0.999 { format!("#{} · {}%", c.to_hex(), (c.a * 100.0).round()) } else { format!("#{}", c.to_hex()) }
}

fn prop(label: impl Into<String>, value: impl Into<String>) -> Prop {
    Prop { label: label.into(), value: value.into(), color: None, token: None }
}

impl Prop {
    fn token(mut self, t: Option<String>) -> Self {
        self.token = t;
        self
    }
    fn color(mut self, c: Color) -> Self {
        self.color = Some(c.to_css_hex());
        self
    }
}

/// The variable bound to property `field` of `n` ("fill" and "stroke" also find bindings stored
/// under their list forms, like "fills/0/color").
pub fn bound_variable<'d>(doc: &'d Document, n: &Node, field: &str) -> Option<&'d Variable> {
    let forms: &[&str] = match field {
        "fill" => &["fill", "fills", "fills/0", "fills/0/color"],
        "stroke" => &["stroke", "strokes/0/color"],
        _ => &[],
    };
    let found = if forms.is_empty() { n.bound.get(field) } else { forms.iter().find_map(|k| n.bound.get(*k)) };
    found.and_then(|v| doc.variables.get(v))
}

/// The variable bound to `field` on `n`, by name.
fn variable(doc: &Document, n: &Node, field: &str) -> Option<String> {
    bound_variable(doc, n, field).map(|v| v.name.clone())
}

fn style_name(doc: &Document, id: Option<&StyleId>) -> Option<String> {
    id.and_then(|s| doc.style(s)).map(|s| s.name.clone())
}

/// The properties of `id`, grouped; empty when it doesn't exist.
pub fn properties(doc: &Document, id: NodeId) -> Vec<Section> {
    let Some(n) = doc.node(id) else { return Vec::new() };
    let mut out = vec![layout(doc, id, n)];
    if let Some(l) = &n.layout {
        out.push(auto_layout(doc, n, l));
    }
    if let Some(t) = n.kind.text() {
        out.push(typography(doc, n, t));
    }
    // Each paint's row names the variable bound to its colour; a style names the first row.
    let paint_rows = |fills: bool, style: Option<&StyleId>| -> Vec<Prop> {
        let paints = if fills { &n.fills } else { &n.strokes };
        let rows: Vec<Prop> = paints
            .iter()
            .enumerate()
            .filter(|(_, p)| p.visible)
            .map(|(i, p)| {
                let token = crate::cmd::lists::paint_binding_key(n, fills, i).and_then(|k| variable(doc, n, k));
                paint_row(p).token(token)
            })
            .collect();
        match style_name(doc, style) {
            Some(name) => tag_first(rows, Some(name)),
            None => rows,
        }
    };
    let fills = paint_rows(true, n.styles.fill.as_ref());
    if !fills.is_empty() {
        out.push(Section { title: "Fills", rows: fills });
    }
    let mut strokes = paint_rows(false, n.styles.stroke.as_ref());
    if !strokes.is_empty() {
        let align = match n.stroke.align {
            StrokeAlign::Inside => "inside",
            StrokeAlign::Center => "center",
            StrokeAlign::Outside => "outside",
        };
        strokes.push(prop("Weight", format!("{} {align}", num(n.stroke.weight))).token(variable(doc, n, "strokeWeight")));
        out.push(Section { title: "Strokes", rows: strokes });
    }
    // A grain size: one number, or across × down.
    let grain = |(sx, sy): (f64, f64)| if sx == sy { num(sx) } else { format!("{}×{}", num(sx), num(sy)) };
    let pct = |v: f64| (v * 100.0).round();
    let effects: Vec<Prop> = n
        .effects
        .iter()
        .filter(|e| e.visible)
        .map(|e| match &e.kind {
            EffectKind::DropShadow(s) | EffectKind::InnerShadow(s) => {
                let label = if matches!(e.kind, EffectKind::DropShadow(_)) { "Drop shadow" } else { "Inner shadow" };
                let v = format!("{} {} {} {} {}", num(s.offset.0), num(s.offset.1), num(s.radius), num(s.spread), color_text(s.color));
                prop(label, v).color(s.color)
            }
            EffectKind::LayerBlur(b) | EffectKind::BackgroundBlur(b) => {
                let label = if matches!(e.kind, EffectKind::LayerBlur(_)) { "Layer blur" } else { "Background blur" };
                let at = |(x, y): (f64, f64)| format!("{}% {}%", pct(x), pct(y));
                let v = if b.is_progressive() {
                    format!("Progressive {} → {} from {} to {}", num(b.start_radius), num(b.radius), at(b.start), at(b.end))
                } else {
                    num(b.radius)
                };
                prop(label, v)
            }
            EffectKind::Noise(nz) => {
                let colors = match nz.mode {
                    NoiseMode::Mono => color_text(nz.color),
                    NoiseMode::Duo => format!("{} {}", color_text(nz.color), color_text(nz.secondary)),
                    NoiseMode::Multi => format!("{}%", pct(f64::from(nz.opacity))),
                };
                let row = prop("Noise", format!("{} {} {}% {colors}", nz.mode.label(), grain(nz.grain()), pct(nz.density)));
                if nz.mode == NoiseMode::Multi { row } else { row.color(nz.color) }
            }
            EffectKind::Texture(t) => {
                prop("Texture", format!("{} {}{}", grain(t.grain()), num(t.radius), if t.clip_to_shape { " clipped" } else { "" }))
            }
            EffectKind::Glass(g) => prop(
                "Glass",
                format!(
                    "{}° {}% refraction {}% depth {} dispersion {}% frost {} splay {}%",
                    num(g.light_angle),
                    pct(g.light_intensity),
                    pct(g.refraction),
                    num(g.depth),
                    pct(g.dispersion),
                    num(g.frost),
                    pct(g.splay)
                ),
            ),
        })
        .collect();
    if !effects.is_empty() {
        out.push(Section { title: "Effects", rows: tag_first(effects, style_name(doc, n.styles.effect.as_ref())) });
    }
    out
}

/// A style or variable applies to the whole list: show it on the first row.
/// One paint's row: its kind and colour (or stops, or opacity for images).
fn paint_row(p: &figcraft_color::Paint) -> Prop {
    match &p.kind {
        PaintKind::Solid { color } => {
            let c = color.with_alpha(color.a * p.opacity);
            prop("Solid", color_text(c)).color(c)
        }
        PaintKind::Gradient(g) => {
            let kind = match g.kind {
                GradientKind::Linear => "Linear",
                GradientKind::Radial => "Radial",
                GradientKind::Angular => "Angular",
                GradientKind::Diamond => "Diamond",
            };
            let stops: Vec<String> = g.stops.iter().map(|s| format!("#{} {}%", s.color.to_hex(), (s.position * 100.0).round())).collect();
            let r = prop(kind, stops.join(", "));
            match g.stops.first() {
                Some(s) => r.color(s.color),
                None => r,
            }
        }
        PaintKind::Image(_) => prop("Image", format!("{}%", (p.opacity * 100.0).round())),
        PaintKind::Pattern(_) => prop("Pattern", format!("{}%", (p.opacity * 100.0).round())),
    }
}

fn tag_first(mut rows: Vec<Prop>, token: Option<String>) -> Vec<Prop> {
    if let Some(first) = rows.first_mut() {
        first.token = token;
    }
    rows
}

fn layout(doc: &Document, id: NodeId, n: &Node) -> Section {
    let mut rows =
        vec![prop("Width", num(n.width)).token(variable(doc, n, "width")), prop("Height", num(n.height)).token(variable(doc, n, "height"))];
    // Position from the parent frame's top-left (from the page origin at the top level).
    if let Some(b) = doc.world_bounds(id) {
        let origin = n
            .parent
            .filter(|p| !doc.node(*p).is_some_and(|pn| pn.is_page()))
            .and_then(|p| doc.world_bounds(p))
            .map_or((0.0, 0.0), |pb| (pb.x0, pb.y0));
        rows.push(prop("Top", num(b.y0 - origin.1)));
        rows.push(prop("Left", num(b.x0 - origin.0)));
    }
    let rot = figcraft_geom::rotation_degrees(n.transform);
    if rot.abs() > 0.01 {
        rows.push(prop("Rotation", format!("{}°", num(rot))));
    }
    if n.opacity < 0.999 {
        rows.push(prop("Opacity", format!("{}%", (f64::from(n.opacity) * 100.0).round())).token(variable(doc, n, "opacity")));
    }
    if n.kind.has_corner_radius() {
        let r = match n.corners.uniform() {
            Some(r) if r > 0.0 => Some(num(r)),
            Some(_) => None,
            None => Some(n.corners.radii.iter().map(|r| num(*r)).collect::<Vec<_>>().join(" ")),
        };
        if let Some(r) = r {
            rows.push(prop("Radius", r).token(variable(doc, n, "cornerRadius")));
        }
    }
    Section { title: "Layout", rows }
}

fn auto_layout(doc: &Document, n: &Node, l: &AutoLayout) -> Section {
    let dir = match (l.mode, l.wrap) {
        (LayoutMode::Horizontal, true) => "Horizontal, wrap",
        (LayoutMode::Horizontal, false) => "Horizontal",
        (LayoutMode::Vertical, true) => "Vertical, wrap",
        (LayoutMode::Vertical, false) => "Vertical",
        (LayoutMode::Grid, _) => "Grid",
    };
    let mut rows = vec![prop("Direction", dir)];
    if l.mode == LayoutMode::Grid {
        rows.push(prop("Grid", format!("{} × {}", l.grid_columns, l.grid_rows)));
        rows.push(prop("Gap", format!("{} {}", num(l.gap), num(l.cross_gap))));
    } else {
        let gap = match l.main_align {
            MainAlign::SpaceBetween => "Auto (space between)".to_string(),
            MainAlign::SpaceAround => "Auto (space around)".to_string(),
            MainAlign::SpaceEvenly => "Auto (space evenly)".to_string(),
            _ => num(l.gap),
        };
        rows.push(prop("Gap", gap).token(variable(doc, n, "gap")));
        if l.wrap {
            let label = if l.mode == LayoutMode::Vertical { "Column gap" } else { "Row gap" };
            rows.push(prop(label, num(l.cross_gap)).token(variable(doc, n, "crossGap")));
        }
    }
    let [t, r, b, lft] = l.padding;
    let padding = if t == r && r == b && b == lft {
        num(t)
    } else if t == b && r == lft {
        format!("{} {}", num(t), num(r))
    } else {
        format!("{} {} {} {}", num(t), num(r), num(b), num(lft))
    };
    let pad_token = variable(doc, n, "padding").or_else(|| variable(doc, n, "paddingHorizontal")).or_else(|| variable(doc, n, "paddingVertical"));
    rows.push(prop("Padding", padding).token(pad_token));
    let main = match l.main_align {
        MainAlign::Min => "start",
        MainAlign::Center => "center",
        MainAlign::Max => "end",
        MainAlign::SpaceBetween => "space between",
        MainAlign::SpaceAround => "space around",
        MainAlign::SpaceEvenly => "space evenly",
    };
    let cross = match l.cross_align {
        CrossAlign::Min => "start",
        CrossAlign::Center => "center",
        CrossAlign::Max => "end",
        CrossAlign::Baseline => "baseline",
    };
    rows.push(prop("Align", format!("{main}, {cross}")));
    Section { title: "Auto layout", rows }
}

fn typography(doc: &Document, n: &Node, t: &TextData) -> Section {
    let s = &t.style;
    let token = style_name(doc, n.styles.text.as_ref());
    let mut rows = vec![
        prop("Font", s.font_family.clone()).token(token),
        prop("Style", s.font_style.clone()),
        prop("Size", num(s.font_size)).token(variable(doc, n, "fontSize")),
        prop("Line height", s.line_height.label()),
        prop("Letter spacing", s.letter_spacing.label()),
    ];
    if t.paragraph_spacing > 0.0 {
        rows.push(prop("Paragraph spacing", num(t.paragraph_spacing)));
    }
    if t.leading_trim == figcraft_doc::LeadingTrim::CapHeight {
        rows.push(prop("Vertical trim", "cap height to baseline"));
    }
    let align = match t.align_h {
        TextAlignH::Left => "left",
        TextAlignH::Center => "center",
        TextAlignH::Right => "right",
        TextAlignH::Justified => "justified",
    };
    rows.push(prop("Align", align));
    if s.decoration != TextDecoration::None {
        rows.push(prop("Decoration", if s.decoration == TextDecoration::Underline { "underline" } else { "strikethrough" }));
    }
    if variable(doc, n, "characters").is_some() {
        rows.push(prop("Content", t.characters.chars().take(80).collect::<String>()).token(variable(doc, n, "characters")));
    }
    Section { title: "Typography", rows }
}
