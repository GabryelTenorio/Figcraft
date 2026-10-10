//! Text node data: characters plus a base style and styled runs.

use figcraft_color::Paint;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "unit", content = "value", rename_all = "camelCase")]
#[derive(Default)]
pub enum LineHeight {
    #[default]
    Auto,
    Pixels(f64),
    Percent(f64),
}

impl LineHeight {
    /// Resolved line height in px for `font_size` (`auto` uses the font's natural ratio).
    pub fn resolve(self, font_size: f64, natural_ratio: f64) -> f64 {
        match self {
            LineHeight::Auto => font_size * natural_ratio,
            LineHeight::Pixels(v) => v.max(0.0),
            LineHeight::Percent(p) => font_size * p.max(0.0) / 100.0,
        }
    }
    pub fn label(self) -> String {
        match self {
            LineHeight::Auto => "Auto".into(),
            LineHeight::Pixels(v) => trim_num(v),
            LineHeight::Percent(p) => format!("{}%", trim_num(p)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "unit", content = "value", rename_all = "camelCase")]
pub enum LetterSpacing {
    Pixels(f64),
    Percent(f64),
}

impl Default for LetterSpacing {
    fn default() -> Self {
        LetterSpacing::Percent(0.0)
    }
}

impl LetterSpacing {
    pub fn resolve(self, font_size: f64) -> f64 {
        match self {
            LetterSpacing::Pixels(v) => v,
            LetterSpacing::Percent(p) => font_size * p / 100.0,
        }
    }
    pub fn label(self) -> String {
        match self {
            LetterSpacing::Pixels(v) => trim_num(v),
            LetterSpacing::Percent(p) => format!("{}%", trim_num(p)),
        }
    }
}

pub fn trim_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract().abs() < 1e-9 { format!("{}", r as i64) } else { format!("{r}") }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    Strikethrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextCase {
    #[default]
    Original,
    Upper,
    Lower,
    Title,
    SmallCaps,
}

impl TextCase {
    pub fn apply(self, s: &str) -> String {
        self.apply_after(s, None)
    }

    /// Apply the case to `s`, which follows `prev` in the text (so title case only capitalises
    /// the first letter of each word even when a word spans several styled runs).
    pub fn apply_after(self, s: &str, prev: Option<char>) -> String {
        match self {
            TextCase::Original | TextCase::SmallCaps => s.to_string(),
            TextCase::Upper => s.to_uppercase(),
            TextCase::Lower => s.to_lowercase(),
            TextCase::Title => {
                let mut out = String::with_capacity(s.len());
                let mut start = prev.is_none_or(|c| !is_word_char(c));
                for ch in s.chars() {
                    if start && ch.is_alphanumeric() {
                        out.extend(ch.to_uppercase());
                        start = false;
                    } else {
                        out.push(ch);
                        start = !is_word_char(ch);
                    }
                }
                out
            }
        }
    }

    /// Whether the char at a position preceded by `prev` starts a word (for title case).
    pub fn starts_word(prev: Option<char>) -> bool {
        prev.is_none_or(|c| !is_word_char(c))
    }
}

/// Letters, digits and apostrophes continue a word (so "don't" stays "Don't").
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '\u{2019}')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextAlignH {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextAlignV {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// How a text box resizes as its content changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextAutoResize {
    /// Auto width: one line per paragraph, box grows both ways.
    #[default]
    WidthAndHeight,
    /// Fixed width, height grows.
    Height,
    /// Fixed size.
    None,
    /// Fixed size, truncate with an ellipsis.
    Truncate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    #[default]
    None,
    Bulleted,
    Numbered,
}

/// Character style. In runs, every field is an override; `None` means "inherit the base".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub font_family: String,
    /// The style name, e.g. "Regular", "Bold", "Semi Bold Italic".
    pub font_style: String,
    pub font_size: f64,
    pub line_height: LineHeight,
    pub letter_spacing: LetterSpacing,
    pub decoration: TextDecoration,
    pub case: TextCase,
    /// Run fills (None = node fills).
    pub fills: Option<Vec<Paint>>,
    /// Hyperlink.
    pub link: Option<String>,
    /// Variable font axis values in user space, keyed by 4-char OpenType axis tag (e.g. `wght`:
    /// 550, `wdth`: 85). Empty = what the style name implies; axes the face lacks are ignored.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub variations: std::collections::BTreeMap<String, f64>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "Inter".into(),
            font_style: "Regular".into(),
            font_size: 12.0,
            line_height: LineHeight::Auto,
            letter_spacing: LetterSpacing::default(),
            decoration: TextDecoration::None,
            case: TextCase::Original,
            fills: None,
            link: None,
            variations: std::collections::BTreeMap::new(),
        }
    }
}

/// Axes a style name implies (weight, italic, slant): picking a named style clears their custom values.
pub const STYLE_AXES: [&str; 3] = ["wght", "ital", "slnt"];

impl TextStyle {
    /// Numeric weight: a custom `wght` axis value, else the one implied by the style name.
    pub fn weight(&self) -> u16 {
        match self.variations.get("wght") {
            Some(w) if w.is_finite() => w.round().clamp(1.0, 1000.0) as u16,
            _ => style_weight(&self.font_style),
        }
    }
    pub fn italic(&self) -> bool {
        self.font_style.to_ascii_lowercase().contains("italic")
            || self.variations.get("ital").is_some_and(|v| *v >= 0.5)
            || self.variations.get("slnt").is_some_and(|v| *v != 0.0)
    }

    /// Switch to the named style, dropping custom values of the axes the name implies.
    pub fn set_font_style(&mut self, name: impl Into<String>) {
        self.font_style = name.into();
        for tag in STYLE_AXES {
            self.variations.remove(tag);
        }
    }
}

/// Weight from a style name ("Semi Bold Italic" → 600).
pub fn style_weight(style: &str) -> u16 {
    let s = style.to_ascii_lowercase().replace([' ', '-'], "");
    let table: [(&str, u16); 10] = [
        ("thin", 100),
        ("extralight", 200),
        ("ultralight", 200),
        ("light", 300),
        ("medium", 500),
        ("semibold", 600),
        ("demibold", 600),
        ("extrabold", 800),
        ("ultrabold", 800),
        ("black", 900),
    ];
    for (k, w) in table {
        if s.contains(k) {
            return w;
        }
    }
    if s.contains("bold") {
        return 700;
    }
    if s.contains("heavy") {
        return 900;
    }
    400
}

/// A styled range of characters (char indices, end exclusive).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyleRun {
    pub start: usize,
    pub end: usize,
    pub style: TextStyle,
}

/// Text node content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextData {
    pub characters: String,
    pub style: TextStyle,
    /// Ranges whose style differs from `style`; full styles (already merged), non-overlapping, sorted.
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub runs: Vec<StyleRun>,
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub auto_resize: TextAutoResize,
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub align_h: TextAlignH,
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub align_v: TextAlignV,
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub paragraph_spacing: f64,
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub paragraph_indent: f64,
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub list: ListStyle,
    /// Max lines when truncating (0 = as many as fit).
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub max_lines: u32,
    /// OpenType feature toggles (e.g. "liga": false).
    #[serde(skip_serializing_if = "crate::node::is_default")]
    pub features: std::collections::BTreeMap<String, bool>,
}

/// Whether `tag` is a valid OpenType feature tag (four printable ASCII chars).
pub fn valid_feature_tag(tag: &str) -> bool {
    tag.len() == 4 && tag.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// Whether `tag` is a valid OpenType variation axis tag (same shape as a feature tag).
pub fn valid_axis_tag(tag: &str) -> bool {
    valid_feature_tag(tag)
}

impl TextData {
    pub fn new(characters: impl Into<String>) -> Self {
        Self { characters: characters.into(), ..Default::default() }
    }

    pub fn char_count(&self) -> usize {
        self.characters.chars().count()
    }

    /// The style at char index `i`.
    pub fn style_at(&self, i: usize) -> &TextStyle {
        self.runs.iter().find(|r| r.start <= i && i < r.end).map(|r| &r.style).unwrap_or(&self.style)
    }

    /// Spans of uniform style covering the whole text: (start, end, style).
    pub fn spans(&self) -> Vec<(usize, usize, &TextStyle)> {
        let n = self.char_count();
        let mut out = Vec::new();
        let mut pos = 0;
        let mut runs: Vec<&StyleRun> = self.runs.iter().filter(|r| r.start < r.end && r.start < n).collect();
        runs.sort_by_key(|r| r.start);
        for r in runs {
            let start = r.start.max(pos);
            let end = r.end.min(n);
            if start >= end {
                continue;
            }
            if start > pos {
                out.push((pos, start, &self.style));
            }
            out.push((start, end, &r.style));
            pos = end;
        }
        if pos < n || out.is_empty() {
            out.push((pos, n, &self.style));
        }
        out
    }

    /// Apply `f` to the style of chars `start..end` (all text when the range is the whole text or
    /// empty text), splitting runs as needed and merging equal neighbours.
    pub fn restyle(&mut self, start: usize, end: usize, f: impl Fn(&mut TextStyle)) {
        let n = self.char_count();
        let (start, end) = (start.min(n), end.min(n));
        if n == 0 || (start == 0 && end >= n) {
            f(&mut self.style);
            for r in &mut self.runs {
                f(&mut r.style);
            }
            self.normalize();
            return;
        }
        if start >= end {
            return;
        }
        // Expand to per-char styles, modify, re-compress.
        let mut per: Vec<TextStyle> = (0..n).map(|i| self.style_at(i).clone()).collect();
        for s in per.iter_mut().take(end).skip(start) {
            f(s);
        }
        self.runs.clear();
        let mut i = 0;
        while i < n {
            let Some(s) = per.get(i) else { break };
            let mut j = i + 1;
            while per.get(j).is_some_and(|t| t == s) {
                j += 1;
            }
            if *s != self.style {
                self.runs.push(StyleRun { start: i, end: j, style: s.clone() });
            }
            i = j;
        }
    }

    /// Drop empty/out-of-range runs and runs equal to the base style.
    pub fn normalize(&mut self) {
        let n = self.char_count();
        let base = self.style.clone();
        self.runs.retain(|r| r.start < r.end && r.start < n && r.style != base);
        for r in &mut self.runs {
            r.end = r.end.min(n);
        }
        self.runs.sort_by_key(|r| r.start);
    }

    /// Replace chars `start..end` with `text`, shifting runs (inserted text takes the style at `start`).
    pub fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        let n = self.char_count();
        let (start, end) = (start.min(n), end.min(n).max(start.min(n)));
        let byte = |i: usize| self.characters.char_indices().nth(i).map(|(b, _)| b).unwrap_or(self.characters.len());
        let (bs, be) = (byte(start), byte(end));
        let mut s = String::with_capacity(self.characters.len() + text.len());
        s.push_str(self.characters.get(..bs).unwrap_or(""));
        s.push_str(text);
        s.push_str(self.characters.get(be..).unwrap_or(""));
        self.characters = s;
        let inserted = text.chars().count();
        let removed = end - start;
        for r in &mut self.runs {
            let shift = |p: usize| -> usize {
                if p <= start {
                    p
                } else if p >= end {
                    p - removed + inserted
                } else {
                    start + inserted
                }
            };
            // A run touching the insertion point at its end grows with the typed text.
            let new_start = shift(r.start);
            let new_end = if r.end == start && r.start < start { start + inserted } else { shift(r.end) };
            r.start = new_start;
            r.end = new_end;
        }
        self.normalize();
    }

    /// Whether every char shares one style (the panel shows "Mixed" otherwise).
    pub fn is_uniform(&self) -> bool {
        self.runs.is_empty()
    }
}

#[cfg(test)]
mod case_tests {
    use super::*;

    #[test]
    fn title_case_respects_context() {
        assert_eq!(TextCase::Title.apply("hello wORLD don't 3d x-ray"), "Hello WORLD Don't 3d X-Ray");
        // Continuing a word from a previous run: no capital.
        assert_eq!(TextCase::Title.apply_after("lo world", Some('l')), "lo World");
        assert_eq!(TextCase::Title.apply_after("lo", Some(' ')), "Lo");
        assert_eq!(TextCase::Upper.apply_after("ß", Some('a')), "SS");
        assert!(valid_feature_tag("liga") && valid_feature_tag("ss01"));
        assert!(!valid_feature_tag("lig") && !valid_feature_tag("ligatures") && !valid_feature_tag("li\u{e9}"));
    }

    #[test]
    fn variations_are_backward_compatible_and_imply_weight() {
        // Files written before variable fonts load with no custom axes, and stay byte-identical.
        let old = r#"{"font_family":"Inter","font_style":"Bold","font_size":12.0}"#;
        let st: TextStyle = serde_json::from_str(old).unwrap();
        assert!(st.variations.is_empty());
        assert!(!serde_json::to_string(&st).unwrap().contains("variations"));
        let mut st = TextStyle { font_style: "Regular".into(), ..Default::default() };
        st.variations.insert("wght".into(), 640.0);
        st.variations.insert("wdth".into(), 80.0);
        assert_eq!(st.weight(), 640);
        assert!(!st.italic());
        st.variations.insert("slnt".into(), -8.0);
        assert!(st.italic());
        let round: TextStyle = serde_json::from_str(&serde_json::to_string(&st).unwrap()).unwrap();
        assert_eq!(round, st);
        // Picking a named style drops the axes its name implies, keeps the others.
        st.set_font_style("Bold");
        assert_eq!(st.weight(), 700);
        assert_eq!(st.variations.keys().collect::<Vec<_>>(), ["wdth"]);
        assert!(valid_axis_tag("wght") && !valid_axis_tag("weight"));
    }
}
