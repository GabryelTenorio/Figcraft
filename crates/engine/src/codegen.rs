//! Code for a layer, as shown by the Inspect panel and returned by `inspect.code`: CSS for the
//! layer itself, and SwiftUI or Jetpack Compose for its subtree (stacks for auto layout, absolute
//! offsets otherwise). The output is a starting point, not a pixel-exact translation.

use figcraft_color::{Color, PaintKind};
use figcraft_doc::*;
use serde_json::json;

use crate::cmd::CommandSpec;
use crate::params::*;
use crate::{bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!("inspect.code", "Code for layer", "{id?, language?: css|swiftui|compose (default css)} → {code}", false, |s, p| {
        let id = targets(s, p).first().copied().ok_or_else(|| bad("inspect.code", "select a layer"))?;
        let lang = match string(p, "language") {
            Some(l) => CodeLang::from_id(&l).ok_or_else(|| bad("inspect.code", "language: css|swiftui|compose"))?,
            None => CodeLang::Css,
        };
        let doc = s.doc()?;
        if !doc.contains(id) {
            return Err(bad("inspect.code", "no such layer"));
        }
        Ok(json!({"code": code_for(doc, id, lang), "language": lang.id()}))
    })]
}

/// Languages the Inspect panel offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CodeLang {
    #[default]
    Css,
    SwiftUi,
    Compose,
}

impl CodeLang {
    pub const ALL: [CodeLang; 3] = [CodeLang::Css, CodeLang::SwiftUi, CodeLang::Compose];
    pub fn id(self) -> &'static str {
        match self {
            CodeLang::Css => "css",
            CodeLang::SwiftUi => "swiftui",
            CodeLang::Compose => "compose",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CodeLang::Css => "CSS",
            CodeLang::SwiftUi => "SwiftUI",
            CodeLang::Compose => "Compose",
        }
    }
    pub fn from_id(s: &str) -> Option<CodeLang> {
        CodeLang::ALL.into_iter().find(|l| l.id().eq_ignore_ascii_case(s) || l.label().eq_ignore_ascii_case(s))
    }
}

/// Layers deeper than this (or past this many children) are summarised.
const MAX_DEPTH: usize = 12;
const MAX_CHILDREN: usize = 200;

/// `v` without trailing zeros (2 decimals).
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r}") }
}

pub fn code_for(doc: &Document, id: NodeId, lang: CodeLang) -> String {
    match lang {
        CodeLang::Css => css(doc, id),
        CodeLang::SwiftUi => {
            let mut out = String::new();
            swift(doc, id, 0, &mut out);
            out
        }
        CodeLang::Compose => {
            let mut out = String::new();
            compose(doc, id, 0, &mut out);
            out
        }
    }
}

fn first_fill(n: &Node) -> Option<Color> {
    n.fills.iter().filter(|p| p.visible).find_map(|p| match &p.kind {
        PaintKind::Solid { color } => Some(color.with_alpha(color.a * p.opacity)),
        PaintKind::Gradient(g) => g.sorted_stops().first().map(|s| s.color),
        PaintKind::Image(_) => None,
    })
}

fn stroke_color(n: &Node) -> Option<Color> {
    if !n.has_visible_stroke() {
        return None;
    }
    n.strokes.iter().filter(|p| p.visible).find_map(|p| p.solid_color().map(|c| c.with_alpha(c.a * p.opacity)))
}

fn drop_shadow(n: &Node) -> Option<&Shadow> {
    n.effects.iter().filter(|e| e.visible).find_map(|e| match &e.kind {
        EffectKind::DropShadow(s) => Some(s),
        _ => None,
    })
}

fn radius(n: &Node) -> f64 {
    n.corners.uniform().unwrap_or_else(|| n.corners.radii.iter().copied().fold(0.0, f64::max))
}

/// Text content as a string literal (escaped for Swift; Kotlin also escapes `$`).
fn literal(s: &str, kotlin: bool) -> String {
    let mut out = String::from("\"");
    for c in s.chars().take(10_000) {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\u{2028}' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            '$' if kotlin => out.push_str("\\$"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Visible children in paint order (bottom first), capped.
fn kids(doc: &Document, n: &Node) -> Vec<NodeId> {
    n.children.iter().copied().filter(|c| doc.node(*c).is_some_and(|k| k.visible)).take(MAX_CHILDREN).collect()
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

// ------------------------------------------------------------------ CSS

fn css(doc: &Document, id: NodeId) -> String {
    let Some(n) = doc.node(id) else { return String::new() };
    let mut out = format!("/* {} */\n", n.name.replace("*/", "* /"));
    out.push_str(&format!("width: {}px;\nheight: {}px;\n", num(n.width), num(n.height)));
    if let Some(l) = &n.layout {
        out.push_str("display: flex;\n");
        out.push_str(&format!("flex-direction: {};\n", if l.mode == LayoutMode::Vertical { "column" } else { "row" }));
        if l.wrap {
            out.push_str("flex-wrap: wrap;\n");
        }
        out.push_str(&format!("gap: {}px;\n", num(l.gap)));
        let [pt, pr, pb, pl] = l.padding;
        out.push_str(&format!("padding: {}px {}px {}px {}px;\n", num(pt), num(pr), num(pb), num(pl)));
        out.push_str(&format!(
            "justify-content: {};\nalign-items: {};\n",
            match l.main_align {
                MainAlign::Min => "flex-start",
                MainAlign::Center => "center",
                MainAlign::Max => "flex-end",
                MainAlign::SpaceBetween => "space-between",
            },
            match l.cross_align {
                CrossAlign::Min => "flex-start",
                CrossAlign::Center => "center",
                CrossAlign::Max => "flex-end",
                CrossAlign::Baseline => "baseline",
            }
        ));
    }
    if let Some(r) = n.corners.uniform().filter(|r| *r > 0.0) {
        out.push_str(&format!("border-radius: {}px;\n", num(r)));
    } else if n.corners.radii.iter().any(|r| *r > 0.0) {
        let [a, b, c, d] = n.corners.radii;
        out.push_str(&format!("border-radius: {}px {}px {}px {}px;\n", num(a), num(b), num(c), num(d)));
    }
    if n.opacity < 1.0 {
        out.push_str(&format!("opacity: {};\n", num(f64::from(n.opacity))));
    }
    for p in n.fills.iter().filter(|p| p.visible) {
        match &p.kind {
            PaintKind::Solid { color } => {
                let key = if matches!(n.kind, NodeKind::Text(_)) { "color" } else { "background" };
                out.push_str(&format!("{key}: {};\n", color.with_alpha(color.a * p.opacity).to_css()));
            }
            PaintKind::Gradient(g) => {
                let stops: Vec<String> = g.sorted_stops().iter().map(|s| format!("{} {}%", s.color.to_css(), (s.position * 100.0).round())).collect();
                out.push_str(&format!("background: linear-gradient({});\n", stops.join(", ")));
            }
            PaintKind::Image(_) => out.push_str("background: url(image.png) center / cover;\n"),
        }
    }
    if let Some(c) = stroke_color(n) {
        out.push_str(&format!("border: {}px solid {};\n", num(n.stroke.weight), c.to_css()));
    }
    let shadows: Vec<String> = n
        .effects
        .iter()
        .filter(|e| e.visible)
        .filter_map(|e| match &e.kind {
            EffectKind::DropShadow(s) => {
                Some(format!("{}px {}px {}px {}px {}", num(s.offset.0), num(s.offset.1), num(s.radius), num(s.spread), s.color.to_css()))
            }
            EffectKind::InnerShadow(s) => {
                Some(format!("inset {}px {}px {}px {}px {}", num(s.offset.0), num(s.offset.1), num(s.radius), num(s.spread), s.color.to_css()))
            }
            _ => None,
        })
        .collect();
    if !shadows.is_empty() {
        out.push_str(&format!("box-shadow: {};\n", shadows.join(", ")));
    }
    for e in n.effects.iter().filter(|e| e.visible) {
        match e.kind {
            EffectKind::LayerBlur { radius } => out.push_str(&format!("filter: blur({}px);\n", num(radius / 2.0))),
            EffectKind::BackgroundBlur { radius } => out.push_str(&format!("backdrop-filter: blur({}px);\n", num(radius / 2.0))),
            _ => {}
        }
    }
    if let NodeKind::Text(t) = &n.kind {
        out.push_str(&format!(
            "font-family: \"{}\";\nfont-size: {}px;\nfont-weight: {};\n",
            t.style.font_family,
            num(t.style.font_size),
            t.style.weight()
        ));
        if t.style.italic() {
            out.push_str("font-style: italic;\n");
        }
        match t.style.line_height {
            LineHeight::Pixels(v) => out.push_str(&format!("line-height: {}px;\n", num(v))),
            LineHeight::Percent(p) => out.push_str(&format!("line-height: {}%;\n", num(p))),
            LineHeight::Auto => {}
        }
        match t.style.letter_spacing {
            LetterSpacing::Pixels(v) if v != 0.0 => out.push_str(&format!("letter-spacing: {}px;\n", num(v))),
            LetterSpacing::Percent(p) if p != 0.0 => out.push_str(&format!("letter-spacing: {}em;\n", num(p / 100.0))),
            _ => {}
        }
    }
    out
}

// ------------------------------------------------------------------ SwiftUI

fn swift_color(c: Color) -> String {
    format!("Color(red: {}, green: {}, blue: {}, opacity: {})", num(f64::from(c.r)), num(f64::from(c.g)), num(f64::from(c.b)), num(f64::from(c.a)))
}

fn swift_weight(w: u16) -> &'static str {
    match w {
        0..=150 => ".thin",
        151..=250 => ".ultraLight",
        251..=350 => ".light",
        351..=450 => ".regular",
        451..=550 => ".medium",
        551..=650 => ".semibold",
        651..=750 => ".bold",
        751..=850 => ".heavy",
        _ => ".black",
    }
}

fn swift(doc: &Document, id: NodeId, depth: usize, out: &mut String) {
    let Some(n) = doc.node(id) else { return };
    let pad = indent(depth);
    if depth > MAX_DEPTH {
        out.push_str(&format!("{pad}// {} …\n", n.name));
        return;
    }
    let mut mods: Vec<String> = Vec::new();
    match &n.kind {
        NodeKind::Text(t) => {
            out.push_str(&format!("{pad}Text({})\n", literal(&t.characters, false)));
            mods.push(format!(
                ".font(.custom({}, size: {}).weight({}))",
                literal(&t.style.font_family, false),
                num(t.style.font_size),
                swift_weight(t.style.weight())
            ));
            if let Some(c) = first_fill(n) {
                mods.push(format!(".foregroundColor({})", swift_color(c)));
            }
        }
        NodeKind::Ellipse { .. } => {
            out.push_str(&format!("{pad}Ellipse()\n"));
            if let Some(c) = first_fill(n) {
                mods.push(format!(".fill({})", swift_color(c)));
            }
            mods.push(format!(".frame(width: {}, height: {})", num(n.width), num(n.height)));
        }
        NodeKind::Rectangle => {
            out.push_str(&format!("{pad}RoundedRectangle(cornerRadius: {})\n", num(radius(n))));
            if let Some(c) = first_fill(n) {
                mods.push(format!(".fill({})", swift_color(c)));
            }
            mods.push(format!(".frame(width: {}, height: {})", num(n.width), num(n.height)));
        }
        k if k.is_frame_like() || matches!(k, NodeKind::Group) => {
            let children = kids(doc, n);
            let (open, close) = match n.layout.as_ref().map(|l| (l.mode, l)) {
                Some((LayoutMode::Vertical, l)) => (format!("VStack(alignment: .leading, spacing: {}) {{", num(l.gap)), "}"),
                Some((_, l)) => (format!("HStack(alignment: .center, spacing: {}) {{", num(l.gap)), "}"),
                None => ("ZStack(alignment: .topLeading) {".to_string(), "}"),
            };
            out.push_str(&format!("{pad}{open}\n"));
            for c in &children {
                swift(doc, *c, depth + 1, out);
                // Free children sit at their offset inside a ZStack.
                if n.layout.is_none()
                    && let Some(cn) = doc.node(*c)
                {
                    let [_, _, _, _, x, y] = cn.transform.as_coeffs();
                    out.push_str(&format!("{}    .offset(x: {}, y: {})\n", indent(depth + 1), num(x), num(y)));
                }
            }
            out.push_str(&format!("{pad}{close}\n"));
            if let Some(l) = &n.layout {
                let [t, r, b, lft] = l.padding;
                if [t, r, b, lft].iter().any(|v| *v > 0.0) {
                    mods.push(format!(".padding(EdgeInsets(top: {}, leading: {}, bottom: {}, trailing: {}))", num(t), num(lft), num(b), num(r)));
                }
            }
            mods.push(format!(".frame(width: {}, height: {}, alignment: .topLeading)", num(n.width), num(n.height)));
            if let Some(c) = first_fill(n) {
                mods.push(format!(".background({})", swift_color(c)));
            }
            if radius(n) > 0.0 {
                mods.push(format!(".clipShape(RoundedRectangle(cornerRadius: {}))", num(radius(n))));
            }
        }
        _ => {
            out.push_str(&format!("{pad}// {}: export as an image asset\n{pad}Image({})\n", n.kind.label(), literal(&n.name, false)));
            mods.push(".resizable()".into());
            mods.push(format!(".frame(width: {}, height: {})", num(n.width), num(n.height)));
        }
    }
    if let Some(c) = stroke_color(n) {
        mods.push(format!(
            ".overlay(RoundedRectangle(cornerRadius: {}).stroke({}, lineWidth: {}))",
            num(radius(n)),
            swift_color(c),
            num(n.stroke.weight)
        ));
    }
    if let Some(s) = drop_shadow(n) {
        mods.push(format!(
            ".shadow(color: {}, radius: {}, x: {}, y: {})",
            swift_color(s.color),
            num(s.radius / 2.0),
            num(s.offset.0),
            num(s.offset.1)
        ));
    }
    if n.opacity < 1.0 {
        mods.push(format!(".opacity({})", num(f64::from(n.opacity))));
    }
    for m in mods {
        out.push_str(&format!("{pad}    {m}\n"));
    }
}

// ------------------------------------------------------------------ Jetpack Compose

fn kotlin_color(c: Color) -> String {
    let [r, g, b, a] = c.to_rgba8();
    format!("Color(0x{a:02X}{r:02X}{g:02X}{b:02X})")
}

/// The Modifier chain for a layer (size, shadow, background, border, alpha, padding).
fn modifier(n: &Node, shape: &str, padding: bool) -> String {
    let mut m = format!("Modifier.size({}.dp, {}.dp)", num(n.width), num(n.height));
    if let Some(s) = drop_shadow(n) {
        m.push_str(&format!(".shadow({}.dp, {shape})", num(s.radius / 2.0)));
    }
    if let Some(c) = first_fill(n) {
        m.push_str(&format!(".background({}, {shape})", kotlin_color(c)));
    }
    if let Some(c) = stroke_color(n) {
        m.push_str(&format!(".border({}.dp, {}, {shape})", num(n.stroke.weight), kotlin_color(c)));
    }
    if n.opacity < 1.0 {
        m.push_str(&format!(".alpha({}f)", num(f64::from(n.opacity))));
    }
    if padding && let Some(l) = &n.layout {
        let [t, r, b, lft] = l.padding;
        if [t, r, b, lft].iter().any(|v| *v > 0.0) {
            m.push_str(&format!(".padding(start = {}.dp, top = {}.dp, end = {}.dp, bottom = {}.dp)", num(lft), num(t), num(r), num(b)));
        }
    }
    m
}

fn compose(doc: &Document, id: NodeId, depth: usize, out: &mut String) {
    let Some(n) = doc.node(id) else { return };
    let pad = indent(depth);
    if depth > MAX_DEPTH {
        out.push_str(&format!("{pad}// {} …\n", n.name));
        return;
    }
    let shape = if radius(n) > 0.0 { format!("RoundedCornerShape({}.dp)", num(radius(n))) } else { "RectangleShape".to_string() };
    match &n.kind {
        NodeKind::Text(t) => {
            let color = first_fill(n).map(kotlin_color).unwrap_or_else(|| "Color.Unspecified".into());
            out.push_str(&format!(
                "{pad}Text(\n{pad}    text = {},\n{pad}    fontSize = {}.sp,\n{pad}    fontWeight = FontWeight.W{},\n{pad}    color = {color},\n{pad})\n",
                literal(&t.characters, true),
                num(t.style.font_size),
                (t.style.weight().clamp(100, 900) / 100) * 100
            ));
        }
        NodeKind::Ellipse { .. } => out.push_str(&format!("{pad}Box(modifier = {})\n", modifier(n, "CircleShape", false))),
        NodeKind::Rectangle => out.push_str(&format!("{pad}Box(modifier = {})\n", modifier(n, &shape, false))),
        k if k.is_frame_like() || matches!(k, NodeKind::Group) => {
            let children = kids(doc, n);
            let open = match n.layout.as_ref().map(|l| (l.mode, l)) {
                Some((LayoutMode::Vertical, l)) => format!("Column(\n{pad}    modifier = {},\n{pad}    verticalArrangement = Arrangement.spacedBy({}.dp),\n{pad}) {{", modifier(n, &shape, true), num(l.gap)),
                Some((_, l)) => format!(
                    "Row(\n{pad}    modifier = {},\n{pad}    horizontalArrangement = Arrangement.spacedBy({}.dp),\n{pad}    verticalAlignment = Alignment.CenterVertically,\n{pad}) {{",
                    modifier(n, &shape, true),
                    num(l.gap)
                ),
                None => format!("Box(modifier = {}) {{", modifier(n, &shape, false)),
            };
            out.push_str(&format!("{pad}{open}\n"));
            for c in &children {
                if n.layout.is_none()
                    && let Some(cn) = doc.node(*c)
                {
                    let [_, _, _, _, x, y] = cn.transform.as_coeffs();
                    out.push_str(&format!("{}Box(modifier = Modifier.offset({}.dp, {}.dp)) {{\n", indent(depth + 1), num(x), num(y)));
                    compose(doc, *c, depth + 2, out);
                    out.push_str(&format!("{}}}\n", indent(depth + 1)));
                } else {
                    compose(doc, *c, depth + 1, out);
                }
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        _ => out.push_str(&format!(
            "{pad}// {}: export as an image asset\n{pad}Image(painter = painterResource(R.drawable.asset), contentDescription = {}, modifier = Modifier.size({}.dp, {}.dp))\n",
            n.kind.label(),
            literal(&n.name, true),
            num(n.width),
            num(n.height)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::params::id_of;
    use serde_json::json;

    fn card(s: &mut Session) -> NodeId {
        let f = id_of(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 200, "height": 100, "name": "Card"})).unwrap()["id"]).unwrap();
        s.execute("node.set", &json!({"id": f, "props": {"fills": ["#FFFFFF"], "cornerRadius": 12}})).unwrap();
        s.execute("text.create", &json!({"text": "Say \"hi\" $5\nnow", "x": 0, "y": 0, "parent": f})).unwrap();
        s.execute("shape.ellipse", &json!({"x": 0, "y": 0, "width": 24, "height": 24, "parent": f})).unwrap();
        s.execute("layout.add", &json!({"ids": [f]})).unwrap();
        f
    }

    #[test]
    fn swiftui_and_compose_for_an_auto_layout_card() {
        let mut s = Session::new();
        let f = card(&mut s);
        let doc = s.doc().unwrap();
        let sw = code_for(doc, f, CodeLang::SwiftUi);
        assert!(sw.contains("Stack(alignment:"), "{sw}");
        assert!(sw.contains("Text(\"Say \\\"hi\\\" $5\\nnow\")"), "escaped: {sw}");
        assert!(sw.contains("Ellipse()"));
        assert!(sw.contains(".clipShape(RoundedRectangle(cornerRadius: 12))"));
        let kt = code_for(doc, f, CodeLang::Compose);
        assert!(kt.contains("Arrangement.spacedBy("), "{kt}");
        assert!(kt.contains("\\$5"), "Kotlin escapes $: {kt}");
        assert!(kt.contains("CircleShape"));
        assert!(kt.contains("RoundedCornerShape(12.dp)"));
        let css = code_for(doc, f, CodeLang::Css);
        assert!(css.contains("display: flex;") && css.contains("border-radius: 12px;"));
        // Through the command.
        let v = s.execute("inspect.code", &json!({"id": f, "language": "compose"})).unwrap();
        assert_eq!(v["code"].as_str().unwrap(), kt);
        assert!(s.execute("inspect.code", &json!({"id": f, "language": "cobol"})).is_err());
    }

    #[test]
    fn free_children_get_offsets_and_deep_trees_are_capped() {
        let mut s = Session::new();
        let mut parent = id_of(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 50, "height": 50})).unwrap()["id"]).unwrap();
        let root = parent;
        s.execute("shape.rectangle", &json!({"x": 7, "y": 9, "width": 5, "height": 5, "parent": root})).unwrap();
        for _ in 0..20 {
            parent = id_of(&s.execute("frame.create", &json!({"x": 1, "y": 1, "width": 40, "height": 40, "parent": parent})).unwrap()["id"]).unwrap();
        }
        let doc = s.doc().unwrap();
        let sw = code_for(doc, root, CodeLang::SwiftUi);
        assert!(sw.contains(".offset(x: 7, y: 9)"), "{sw}");
        assert!(sw.contains("…"), "depth capped");
        let kt = code_for(doc, root, CodeLang::Compose);
        assert!(kt.contains("Modifier.offset(7.dp, 9.dp)"));
        assert!(code_for(doc, NodeId(999_999), CodeLang::Css).is_empty());
    }
}
