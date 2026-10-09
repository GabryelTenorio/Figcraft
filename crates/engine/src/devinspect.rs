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

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
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
    )]
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

/// The variable bound to `field` on `n`, by name.
fn variable(doc: &Document, n: &Node, field: &str) -> Option<String> {
    n.bound.get(field).and_then(|v| doc.variables.get(v)).map(|v| v.name.clone())
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
    let paint_rows = |paints: &[figcraft_color::Paint]| -> Vec<Prop> {
        paints
            .iter()
            .filter(|p| p.visible)
            .map(|p| match &p.kind {
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
            })
            .collect()
    };
    let fills = paint_rows(&n.fills);
    if !fills.is_empty() {
        let token = style_name(doc, n.styles.fill.as_ref()).or_else(|| variable(doc, n, "fill"));
        out.push(Section { title: "Fills", rows: tag_first(fills, token) });
    }
    let mut strokes = paint_rows(&n.strokes);
    if !strokes.is_empty() {
        let token = style_name(doc, n.styles.stroke.as_ref()).or_else(|| variable(doc, n, "stroke"));
        strokes = tag_first(strokes, token);
        let align = match n.stroke.align {
            StrokeAlign::Inside => "inside",
            StrokeAlign::Center => "center",
            StrokeAlign::Outside => "outside",
        };
        strokes.push(prop("Weight", format!("{} {align}", num(n.stroke.weight))).token(variable(doc, n, "strokeWeight")));
        out.push(Section { title: "Strokes", rows: strokes });
    }
    let effects: Vec<Prop> = n
        .effects
        .iter()
        .filter(|e| e.visible)
        .filter_map(|e| match &e.kind {
            EffectKind::DropShadow(s) | EffectKind::InnerShadow(s) => {
                let label = if matches!(e.kind, EffectKind::DropShadow(_)) { "Drop shadow" } else { "Inner shadow" };
                let v = format!("{} {} {} {} {}", num(s.offset.0), num(s.offset.1), num(s.radius), num(s.spread), color_text(s.color));
                Some(prop(label, v).color(s.color))
            }
            EffectKind::LayerBlur { radius } => Some(prop("Layer blur", num(*radius))),
            EffectKind::BackgroundBlur { radius } => Some(prop("Background blur", num(*radius))),
            _ => None,
        })
        .collect();
    if !effects.is_empty() {
        out.push(Section { title: "Effects", rows: tag_first(effects, style_name(doc, n.styles.effect.as_ref())) });
    }
    out
}

/// A style or variable applies to the whole list: show it on the first row.
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
        (LayoutMode::Vertical, _) => "Vertical",
        (LayoutMode::Grid, _) => "Grid",
    };
    let mut rows = vec![prop("Direction", dir)];
    if l.mode == LayoutMode::Grid {
        rows.push(prop("Grid", format!("{} × {}", l.grid_columns, l.grid_rows)));
        rows.push(prop("Gap", format!("{} {}", num(l.gap), num(l.cross_gap))));
    } else {
        let gap = if l.main_align == MainAlign::SpaceBetween { "Auto (space between)".to_string() } else { num(l.gap) };
        rows.push(prop("Gap", gap).token(variable(doc, n, "gap")));
        if l.wrap {
            rows.push(prop("Row gap", num(l.cross_gap)).token(variable(doc, n, "crossGap")));
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
