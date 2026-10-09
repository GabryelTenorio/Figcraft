//! Typography section of the Design panel: font family (searchable, previewed, recent first),
//! style, size (with common sizes), line height, letter spacing, paragraph spacing, resizing,
//! alignment, and the Type settings popover (decoration, case, lists, indent, truncation, OpenType
//! features and variable font axes). While text is being edited with a range selected, character
//! edits apply to the range (`text.style`, `text.setVariations`); otherwise they apply to whole
//! layers. Differing values show "Mixed".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_color::{Color, Paint};
use figcraft_doc::*;
use serde_json::{Value, json};

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::right::{common, menu_item, row};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, FIELD_H, FieldEdit, Lead, PAD};

/// Font sizes offered by the size dropdown.
const SIZES: [f64; 15] = [10.0, 11.0, 12.0, 13.0, 14.0, 16.0, 20.0, 24.0, 32.0, 36.0, 40.0, 48.0, 64.0, 96.0, 128.0];

/// OpenType features offered in Type settings ▸ Details: (tag, label, on by default in fonts).
const FEATURES: [(&str, &str, bool); 12] = [
    ("liga", "Standard ligatures", true),
    ("calt", "Contextual alternates", true),
    ("tnum", "Tabular numbers", false),
    ("frac", "Fractions", false),
    ("case", "Case-sensitive forms", false),
    ("zero", "Slashed zero", false),
    ("ss01", "Stylistic set 1", false),
    ("ss02", "Stylistic set 2", false),
    ("ss03", "Stylistic set 3", false),
    ("ss04", "Stylistic set 4", false),
    ("ss05", "Stylistic set 5", false),
    ("smcp", "Small capitals", false),
];

const MIXED: &str = "Mixed";

/// Horizontal space a dropdown takes besides its label (padding and chevron).
const DROPDOWN_CHROME: f32 = 30.0;

/// A dropdown whose label stays on one line: long names (variable font styles like "SemiBold
/// SemiCondensed") end with an ellipsis and show in full on hover.
fn elided_dropdown(ui: &mut Ui, label: &str, width: f32) -> egui::Response {
    let shown = elide(ui, label, width - DROPDOWN_CHROME);
    let resp = widgets::dropdown(ui, &shown, width, None);
    if shown != label { resp.on_hover_text(label) } else { resp }
}

/// `s` shortened with an ellipsis to fit `max_w` on one line.
fn elide(ui: &Ui, s: &str, max_w: f32) -> String {
    let fits = |s: &str| ui.painter().layout_no_wrap(s.to_string(), regular(11.0), Color32::WHITE).size().x <= max_w;
    if fits(s) {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    (1..chars.len())
        .rev()
        .map(|n| format!("{}…", chars.iter().take(n).collect::<String>().trim_end()))
        .find(|c| fits(c))
        .unwrap_or_else(|| "…".into())
}

/// The value shared by every style, or None when they differ.
fn uniform<T: PartialEq + Clone>(styles: &[TextStyle], f: impl Fn(&TextStyle) -> T) -> Option<T> {
    let mut it = styles.iter();
    let first = f(it.next()?);
    if it.all(|s| f(s) == first) { Some(first) } else { None }
}

/// The character styles the panel shows: the edited range's spans, else every span of every
/// selected text layer.
fn target_styles(doc: &Document, ids: &[NodeId], range: Option<(NodeId, usize, usize)>) -> Vec<TextStyle> {
    let mut out: Vec<TextStyle> = Vec::new();
    let mut push = |s: &TextStyle| {
        if !out.contains(s) {
            out.push(s.clone());
        }
    };
    for id in ids {
        let Some(t) = doc.node(*id).and_then(|n| n.kind.text()) else { continue };
        match range {
            Some((rid, a, b)) if rid == *id && a < b => {
                for (s, e, st) in t.spans() {
                    if s < b && e > a {
                        push(st);
                    }
                }
            }
            _ => {
                for (_, _, st) in t.spans() {
                    push(st);
                }
            }
        }
    }
    out
}

pub fn typography(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId]) {
    let t = Tokens::get(ui.ctx());
    let ids: Vec<NodeId> = sel.iter().copied().filter(|i| matches!(doc.node(*i).map(|n| &n.kind), Some(NodeKind::Text(_)))).collect();
    if ids.is_empty() {
        return;
    }
    // While editing a range, character edits apply to it.
    let range = figcraft_engine::textedit::selected_range(&app.session);
    let editing = range.is_some();
    let styles = target_styles(doc, &ids, range);
    let apply_style = |app: &mut FigApp, key: &str, v: Value| {
        if let Some((id, a, b)) = range
            && a != b
        {
            app.run("text.style", json!({"id": id, "start": a, "end": b, key: v}));
        } else {
            app.set_props(&ids, json!({ key: v }));
        }
    };

    widgets::section_header(ui, "Typography", &[(Icon::Styles, "Text styles")], false);

    // Font family.
    let family = uniform(&styles, |s| s.font_family.clone());
    let fstyle = uniform(&styles, |s| s.font_style.clone());
    row(ui, 32.0, |ui| {
        let w = ui.available_width();
        let resp = elided_dropdown(ui, family.as_deref().unwrap_or(MIXED), w);
        egui::Popup::menu(&resp).width(w.max(240.0)).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            if let Some(f) = font_picker(ui, family.as_deref(), resp.clicked()) {
                remember_family(ui.ctx(), &f);
                apply_style(app, "fontFamily", json!(f));
                ui.close();
            }
        });
    });

    // Style + size.
    let half = (ui.available_width() - PAD * 2.0 - 8.0) / 2.0;
    row(ui, 32.0, |ui| {
        let resp = elided_dropdown(ui, fstyle.as_deref().unwrap_or(MIXED), half);
        let fam = family.clone();
        egui::Popup::menu(&resp).width(200.0).show(|ui| match &fam {
            Some(fam) => {
                let list = figcraft_text::family_styles(fam);
                if list.is_empty() {
                    widgets::label(ui, "Font not installed", t.text_tertiary);
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for s in list {
                        let mark = if fstyle.as_deref() == Some(s.as_str()) { "✓ " } else { "   " };
                        if menu_item(ui, &format!("{mark}{s}"), "") {
                            apply_style(app, "fontStyle", json!(s));
                            ui.close();
                        }
                    }
                });
            }
            None => widgets::label(ui, "Pick one font family first", t.text_tertiary),
        });
        let size = uniform(&styles, |s| s.font_size);
        ui.spacing_mut().item_spacing.x = 0.0;
        let e = widgets::number_field(ui, Id::new("ty_size"), Lead::None, size, "", half - 20.0, 1.0);
        match e {
            Some(FieldEdit::Set(v)) => apply_style(app, "fontSize", json!(v.clamp(1.0, 5000.0))),
            // The text editing session already groups undo; just apply.
            Some(FieldEdit::Scrub(v, _)) if editing => apply_style(app, "fontSize", json!(v.clamp(1.0, 5000.0))),
            Some(FieldEdit::Scrub(v, done)) => {
                live(app, "Font size", "node.set", json!({"ids": ids, "props": {"fontSize": v.clamp(1.0, 5000.0)}}), done)
            }
            None => {}
        }
        let more = icons::button(ui, Icon::Chevron, 20.0, false, "Font size");
        egui::Popup::menu(&more).width(90.0).show(|ui| {
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for v in SIZES {
                    let mark = if size == Some(v) { "✓ " } else { "   " };
                    if menu_item(ui, &format!("{mark}{}", trim_num(v)), "") {
                        apply_style(app, "fontSize", json!(v));
                        ui.close();
                    }
                }
            });
        });
    });

    // Line height + letter spacing.
    row(ui, 32.0, |ui| {
        let lh = uniform(&styles, |s| s.line_height);
        let lh_label = lh.map(|l| l.label()).unwrap_or_else(|| MIXED.into());
        if let Some(v) = lead_field(ui, Id::new("ty_lh"), LeadGlyph::LineHeight, &lh_label, half, "Line height")
            && let Some(v) = parse_line_height(&v, lh)
        {
            apply_style(app, "lineHeight", v);
        }
        let ls = uniform(&styles, |s| s.letter_spacing);
        let ls_label = ls.map(|l| l.label()).unwrap_or_else(|| MIXED.into());
        if let Some(v) = lead_field(ui, Id::new("ty_ls"), LeadGlyph::LetterSpacing, &ls_label, half, "Letter spacing")
            && let Some(v) = parse_letter_spacing(&v, ls)
        {
            apply_style(app, "letterSpacing", v);
        }
    });

    // Paragraph spacing + resizing.
    let ps = common(doc, &ids, |n| n.kind.text().map(|t| t.paragraph_spacing.to_bits())).flatten().map(f64::from_bits);
    let resize = common(doc, &ids, |n| n.kind.text().map(|t| t.auto_resize)).flatten();
    row(ui, 32.0, |ui| {
        let ps_label = ps.map(trim_num).unwrap_or_else(|| MIXED.into());
        if let Some(v) = lead_field(ui, Id::new("ty_ps"), LeadGlyph::ParagraphSpacing, &ps_label, half, "Paragraph spacing")
            && let Some(v) = widgets::eval(&v, ps)
        {
            app.set_props(&ids, json!({"paragraphSpacing": v.clamp(0.0, 10_000.0)}));
        }
        let items = [(Icon::Hug, "Auto width"), (Icon::LayoutV, "Auto height"), (Icon::Fixed, "Fixed size"), (Icon::Dots, "Truncate text")];
        let active = resize.map(|r| match r {
            TextAutoResize::WidthAndHeight => 0,
            TextAutoResize::Height => 1,
            TextAutoResize::None => 2,
            TextAutoResize::Truncate => 3,
        });
        if let Some(i) = widgets::segmented(ui, &items, active, (half / 4.0).floor()) {
            let v = ["widthAndHeight", "height", "none", "truncate"].get(i).copied().unwrap_or("none");
            app.set_props(&ids, json!({"textAutoResize": v}));
        }
    });

    // Alignment + Type settings.
    let align = common(doc, &ids, |n| n.kind.text().map(|t| (t.align_h, t.align_v))).flatten();
    row(ui, 32.0, |ui| {
        let items = [
            (Icon::TextAlignLeft, "Align left"),
            (Icon::TextAlignCenter, "Align center"),
            (Icon::TextAlignRight, "Align right"),
            (Icon::TextAlignJustify, "Justify"),
        ];
        let active = align.map(|(h, _)| match h {
            TextAlignH::Left => 0,
            TextAlignH::Center => 1,
            TextAlignH::Right => 2,
            TextAlignH::Justified => 3,
        });
        if let Some(i) = widgets::segmented(ui, &items, active, 24.0) {
            let v = ["left", "center", "right", "justified"].get(i).copied().unwrap_or("left");
            app.set_props(&ids, json!({"textAlign": v}));
        }
        let items = [(Icon::TextTop, "Align top"), (Icon::TextMiddle, "Align middle"), (Icon::TextBottom, "Align bottom")];
        let active = align.map(|(_, v)| match v {
            TextAlignV::Top => 0,
            TextAlignV::Middle => 1,
            TextAlignV::Bottom => 2,
        });
        if let Some(i) = widgets::segmented(ui, &items, active, 24.0) {
            let v = ["top", "middle", "bottom"].get(i).copied().unwrap_or("top");
            app.set_props(&ids, json!({"textAlignVertical": v}));
        }
        let open_id = Id::new("ty_settings_open");
        let open = egui::Popup::is_id_open(ui.ctx(), open_id);
        let more = icons::button(ui, Icon::Dots, 24.0, open, "Type settings");
        egui::Popup::from_toggle_button_response(&more)
            .id(open_id)
            .width(264.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| type_settings(app, ui, doc, &ids, &styles, editing, &apply_style));
    });
}

/// Run `cmd` as a live preview while scrubbing; the gesture becomes one undo step when `done`.
/// (While text is edited, run commands directly: the editing session already groups undo.)
fn live(app: &mut FigApp, label: &str, cmd: &str, params: Value, done: bool) {
    let _ = app.session.begin_interaction(label);
    if let Err(e) = app.session.execute(cmd, &params) {
        app.toast(e.to_string());
    }
    if done {
        let _ = app.session.commit_interaction();
    }
}

/// The variable font the shown styles use (one family, one variable face), if any.
fn variable_face(styles: &[TextStyle]) -> Option<Arc<figcraft_text::Face>> {
    let family = uniform(styles, |s| s.font_family.clone())?;
    let face = figcraft_text::find_face(&family, &styles.first()?.font_style)?;
    (face.is_variable() && face.family.eq_ignore_ascii_case(&family)).then_some(face)
}

/// The Type settings popover: Basics (decoration, case, list, indent, truncation), Details
/// (OpenType features) and, for variable fonts, Variable (axis sliders).
fn type_settings(
    app: &mut FigApp,
    ui: &mut Ui,
    doc: &Document,
    ids: &[NodeId],
    styles: &[TextStyle],
    editing: bool,
    apply_style: &dyn Fn(&mut FigApp, &str, Value),
) {
    let t = Tokens::get(ui.ctx());
    let tab_id = Id::new("ty_settings_tab");
    let vf = variable_face(styles);
    let tabs: &[&str] = if vf.is_some() { &["Basics", "Details", "Variable"] } else { &["Basics", "Details"] };
    let mut tab: usize = ui.data(|d| d.get_temp(tab_id)).filter(|i| *i < tabs.len()).unwrap_or(0);
    ui.horizontal(|ui| {
        for (i, l) in tabs.iter().enumerate() {
            let col = if tab == i { t.text } else { t.text_secondary };
            let r = ui.add(egui::Label::new(egui::RichText::new(*l).font(semibold(11.5)).color(col)).sense(Sense::click()));
            if r.clicked() {
                tab = i;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.add_space(4.0);
    let w = ui.available_width();
    if tab == 0 {
        let deco = uniform(styles, |s| s.decoration);
        caption(ui, "Decoration");
        let active = deco.map(|d| match d {
            TextDecoration::None => 0,
            TextDecoration::Underline => 1,
            TextDecoration::Strikethrough => 2,
        });
        if let Some(i) = text_seg(ui, "deco", &[("—", "None"), ("U", "Underline"), ("S", "Strikethrough")], active, w) {
            apply_style(app, "textDecoration", json!((["none", "underline", "strikethrough"].get(i).copied().unwrap_or("none"))));
        }
        caption(ui, "Letter case");
        let case = uniform(styles, |s| s.case);
        let active = case.map(|c| match c {
            TextCase::Original => 0,
            TextCase::Upper => 1,
            TextCase::Lower => 2,
            TextCase::Title => 3,
            TextCase::SmallCaps => 4,
        });
        let items = [("—", "As typed"), ("AG", "Uppercase"), ("ag", "Lowercase"), ("Ag", "Title case"), ("Aɢ", "Small caps")];
        if let Some(i) = text_seg(ui, "case", &items, active, w) {
            apply_style(app, "textCase", json!((["original", "upper", "lower", "title", "smallCaps"].get(i).copied().unwrap_or("original"))));
        }
        caption(ui, "List style");
        let list = common(doc, ids, |n| n.kind.text().map(|t| t.list)).flatten();
        let active = list.map(|l| match l {
            ListStyle::None => 0,
            ListStyle::Bulleted => 1,
            ListStyle::Numbered => 2,
        });
        if let Some(i) = text_seg(ui, "list", &[("—", "No list"), ("•", "Bulleted list"), ("1.", "Numbered list")], active, w) {
            app.set_props(ids, json!({"listStyle": (["none", "bulleted", "numbered"].get(i).copied().unwrap_or("none"))}));
        }
        ui.add_space(6.0);
        let indent = common(doc, ids, |n| n.kind.text().map(|t| t.paragraph_indent.to_bits())).flatten().map(f64::from_bits);
        ui.horizontal(|ui| {
            ui.add_sized(vec2(110.0, FIELD_H), egui::Label::new(egui::RichText::new("Paragraph indent").font(regular(11.0)).color(t.text)));
            if let Some(FieldEdit::Set(v) | FieldEdit::Scrub(v, true)) =
                widgets::number_field(ui, Id::new("ty_indent"), Lead::None, indent, "", w - 118.0, 1.0)
            {
                app.set_props(ids, json!({"paragraphIndent": v.clamp(0.0, 10_000.0)}));
            }
        });
        let resize = common(doc, ids, |n| n.kind.text().map(|t| t.auto_resize)).flatten();
        caption(ui, "Truncate text");
        let active = resize.map(|r| usize::from(r == TextAutoResize::Truncate));
        match text_seg(ui, "trunc", &[("Off", "Don't truncate"), ("…", "Truncate with an ellipsis")], active, w) {
            Some(1) => app.set_props(ids, json!({"textAutoResize": "truncate"})),
            Some(_) if resize != Some(TextAutoResize::Height) && resize != Some(TextAutoResize::WidthAndHeight) => {
                app.set_props(ids, json!({"textAutoResize": "none"}))
            }
            _ => {}
        }
        let max = common(doc, ids, |n| n.kind.text().map(|t| t.max_lines)).flatten();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_sized(vec2(110.0, FIELD_H), egui::Label::new(egui::RichText::new("Max lines").font(regular(11.0)).color(t.text)));
            let shown = max.map(f64::from).filter(|m| *m > 0.0);
            if let Some(FieldEdit::Set(v) | FieldEdit::Scrub(v, true)) =
                widgets::number_field(ui, Id::new("ty_maxlines"), Lead::None, shown, "", w - 118.0, 1.0)
            {
                app.run("text.maxLines", json!({"ids": ids, "lines": v.round().clamp(0.0, 10_000.0)}));
            }
        });
    } else if let (2, Some(face)) = (tab, &vf) {
        variable_axes(app, ui, ids, styles, face, editing);
    } else {
        // Which features the font has (first style shown).
        let face = uniform(styles, |s| (s.font_family.clone(), s.font_style.clone())).and_then(|(f, s)| figcraft_text::find_face(&f, &s));
        let face_name = face.as_ref().map(|f| format!("{} {}", f.family, f.style)).unwrap_or_else(|| "Mixed fonts".into());
        widgets::label(ui, &face_name, t.text_secondary);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (tag, label, default_on) in FEATURES {
                let state = common(doc, ids, |n| n.kind.text().map(|t| t.features.get(tag).copied())).flatten();
                let on = state.map(|s| s.unwrap_or(default_on));
                let supported = face.as_ref().is_none_or(|f| f.has_feature(tag.as_bytes().try_into().unwrap_or(b"    ")));
                if feature_row(ui, tag, label, on, supported) {
                    let want = !on.unwrap_or(false);
                    // Back to the font's default clears the override.
                    let v = if want == default_on { Value::Null } else { json!(want) };
                    app.run("text.setFeatures", json!({"ids": ids, "features": {tag: v}}));
                }
            }
        });
    }
}

/// Type settings ▸ Variable: a slider and a field per visible axis of the variable font (values
/// the style name implies show until changed; "Mixed" when the shown styles differ).
fn variable_axes(app: &mut FigApp, ui: &mut Ui, ids: &[NodeId], styles: &[TextStyle], face: &figcraft_text::Face, editing: bool) {
    let t = Tokens::get(ui.ctx());
    widgets::label(ui, &face.family, t.text_secondary);
    // Each style's axis values; a style drawn by another face (e.g. a separate italic file) is mixed.
    let per_style: Vec<Option<Vec<f64>>> = styles
        .iter()
        .map(|s| figcraft_text::find_face(&s.font_family, &s.font_style).filter(|f| f.id == face.id).map(|_| face.axis_values(s)))
        .collect();
    let w = ui.available_width();
    let field_w = 52.0;
    let label_w = 84.0;
    let slider_w = (w - label_w - field_w - 12.0).max(40.0);
    for (i, axis) in face.axes.iter().enumerate().filter(|(_, a)| !a.hidden) {
        let first = per_style.first().cloned().flatten().and_then(|v| v.get(i).copied());
        let value = first.filter(|v0| per_style.iter().all(|vs| vs.as_ref().and_then(|v| v.get(i)).is_some_and(|v| (v - v0).abs() < 1e-9)));
        let value = value.map(|v| round_axis(axis, v));
        // (value, gesture ended, live preview while dragging)
        let mut set: Option<(Value, bool, bool)> = None;
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (lr, _) = ui.allocate_exact_size(vec2(label_w, FIELD_H), Sense::hover());
            let name = ui.painter().layout(axis.name.clone(), regular(11.0), t.text, label_w);
            let tip = format!(
                "{} ({}): {} – {}, default {}. Double-click the slider to go back to the style's value.",
                axis.name,
                axis.tag,
                trim_num(axis.min),
                trim_num(axis.max),
                trim_num(axis.default)
            );
            ui.interact(lr, Id::new(("ty_axis_label", &axis.tag)), Sense::hover()).on_hover_text(tip);
            ui.painter().galley(pos2(lr.min.x, lr.center().y - name.size().y / 2.0), name, t.text);
            match axis_slider(ui, Id::new(("ty_axis_slider", &axis.tag)), axis, value, slider_w) {
                Some((Some(v), done)) => set = Some((json!(round_axis(axis, v)), done, true)),
                Some((None, _)) => set = Some((Value::Null, true, false)),
                None => {}
            }
            let step = if axis.max - axis.min > 20.0 { 1.0 } else { 0.1 };
            match widgets::number_field(ui, Id::new(("ty_axis_field", &axis.tag)), Lead::None, value, "", field_w, step) {
                Some(FieldEdit::Set(v)) => set = Some((json!(axis.clamp(round_axis(axis, v))), true, false)),
                Some(FieldEdit::Scrub(v, done)) => set = Some((json!(axis.clamp(round_axis(axis, v))), done, true)),
                None => {}
            }
        });
        if let Some((v, done, preview)) = set {
            let params = json!({"ids": ids, "variations": {axis.tag.clone(): v}});
            if preview && !editing {
                live(app, "Variable font axis", "text.setVariations", params, done);
            } else {
                app.run("text.setVariations", params);
            }
        }
    }
    if styles.iter().any(|s| !s.variations.is_empty()) {
        ui.add_space(8.0);
        let r = ui.add(egui::Label::new(egui::RichText::new("Reset to style").font(regular(11.0)).color(t.accent)).sense(Sense::click()));
        if r.on_hover_text("Back to the values the font style implies").clicked() {
            app.run("text.setVariations", json!({"ids": ids, "variations": {}, "replace": true}));
        }
    }
}

/// Axis values snap to whole numbers on wide axes (weight, width…) and tenths on narrow ones.
fn round_axis(axis: &figcraft_text::Axis, v: f64) -> f64 {
    if axis.max - axis.min > 20.0 { v.round() } else { (v * 10.0).round() / 10.0 }
}

/// A horizontal slider over an axis's range with a tick at its default. `value` None = mixed.
/// Returns the new value (None = double-click: back to the style's value) and whether the
/// gesture ended.
fn axis_slider(ui: &mut Ui, id: Id, axis: &figcraft_text::Axis, value: Option<f64>, width: f32) -> Option<(Option<f64>, bool)> {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, FIELD_H), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(width - 12.0, 4.0));
    let span = (axis.max - axis.min).max(1e-9);
    let x_of = |v: f64| track.min.x + ((v - axis.min) / span).clamp(0.0, 1.0) as f32 * track.width();
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(2), t.border);
    let dx = x_of(axis.default);
    p.line_segment([pos2(dx, track.min.y - 3.0), pos2(dx, track.max.y + 3.0)], Stroke::new(1.0, t.border_strong));
    if let Some(v) = value {
        let x = x_of(v);
        p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), CornerRadius::same(2), t.accent);
        p.circle_filled(pos2(x, track.center().y), 6.0, Color32::WHITE);
        let ring = if resp.hovered() || resp.dragged() { t.accent } else { t.border_strong };
        p.circle_stroke(pos2(x, track.center().y), 6.0, Stroke::new(1.0, ring));
    }
    if resp.double_clicked() {
        return Some((None, true));
    }
    if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let f = f64::from(((pos.x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0));
        return Some((Some(axis.min + f * span), resp.drag_stopped() || resp.clicked()));
    }
    if resp.drag_stopped() {
        return value.map(|v| (Some(v), true));
    }
    None
}

fn caption(ui: &mut Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(egui::RichText::new(s).font(regular(10.5)).color(t.text_secondary));
}

/// A segmented control with short text labels (decoration, case, list…). Returns the clicked index.
fn text_seg(ui: &mut Ui, salt: &str, items: &[(&str, &str)], active: Option<usize>, width: f32) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, FIELD_H), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(5), t.field);
    let cell = width / items.len().max(1) as f32;
    let mut out = None;
    for (i, (label, tip)) in items.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.min.x + i as f32 * cell, rect.min.y), vec2(cell, FIELD_H));
        let resp = ui.interact(r, Id::new(("ty_seg", salt, i)), Sense::click()).on_hover_text(*tip);
        if active == Some(i) {
            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(4), t.panel);
            ui.painter().rect_stroke(r.shrink(2.0), CornerRadius::same(4), Stroke::new(1.0, t.border), StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(4), t.hover);
        }
        let col = if active == Some(i) { t.text } else { t.text_secondary };
        let galley = ui.painter().layout_no_wrap((*label).to_string(), regular(11.5), col);
        let gr = Rect::from_center_size(r.center(), galley.size());
        let size = galley.size();
        ui.painter().galley(gr.min, galley, col);
        // Draw the decoration on its own sample letter.
        match *tip {
            "Underline" => {
                let y = gr.max.y - 1.5;
                ui.painter().line_segment([pos2(gr.min.x, y), pos2(gr.min.x + size.x, y)], Stroke::new(1.0, col));
            }
            "Strikethrough" => {
                let y = gr.center().y + 0.5;
                ui.painter().line_segment([pos2(gr.min.x - 1.0, y), pos2(gr.min.x + size.x + 1.0, y)], Stroke::new(1.0, col));
            }
            _ => {}
        }
        if resp.clicked() {
            out = Some(i);
        }
    }
    out
}

/// One OpenType feature toggle; `on` None = mixed. Returns true when clicked.
fn feature_row(ui: &mut Ui, tag: &str, label: &str, on: Option<bool>, supported: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    let resp = if supported { resp } else { resp.on_hover_text("Not in this font") };
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(4), t.hover);
    }
    let b = Rect::from_min_size(pos2(rect.min.x + 4.0, rect.center().y - 7.0), vec2(14.0, 14.0));
    match on {
        Some(true) => {
            ui.painter().rect_filled(b, CornerRadius::same(3), t.accent);
            icons::paint(ui.painter(), b.shrink(1.5), Icon::Check, Color32::WHITE);
        }
        Some(false) => {
            ui.painter().rect_stroke(b, CornerRadius::same(3), Stroke::new(1.0, t.border_strong), StrokeKind::Inside);
        }
        None => {
            ui.painter().rect_filled(b, CornerRadius::same(3), t.accent);
            ui.painter().line_segment([pos2(b.min.x + 3.5, b.center().y), pos2(b.max.x - 3.5, b.center().y)], Stroke::new(1.5, Color32::WHITE));
        }
    }
    let col = if supported { t.text } else { t.text_tertiary };
    ui.painter().text(pos2(b.max.x + 8.0, rect.center().y), Align2::LEFT_CENTER, label, regular(11.0), col);
    ui.painter().text(pos2(rect.max.x - 6.0, rect.center().y), Align2::RIGHT_CENTER, tag, regular(10.0), t.text_tertiary);
    resp.clicked()
}

#[derive(Clone, Copy)]
enum LeadGlyph {
    LineHeight,
    LetterSpacing,
    ParagraphSpacing,
}

/// Paint a small original glyph for a typography field's lead.
fn paint_lead(p: &egui::Painter, r: Rect, g: LeadGlyph, col: Color32) {
    let c = r.center();
    let s = Stroke::new(1.0, col);
    match g {
        LeadGlyph::LineHeight => {
            p.line_segment([pos2(c.x - 5.0, c.y - 6.0), pos2(c.x + 5.0, c.y - 6.0)], s);
            p.line_segment([pos2(c.x - 5.0, c.y + 6.0), pos2(c.x + 5.0, c.y + 6.0)], s);
            p.text(c, Align2::CENTER_CENTER, "A", regular(9.5), col);
        }
        LeadGlyph::LetterSpacing => {
            p.line_segment([pos2(c.x - 6.0, c.y - 5.0), pos2(c.x - 6.0, c.y + 5.0)], s);
            p.line_segment([pos2(c.x + 6.0, c.y - 5.0), pos2(c.x + 6.0, c.y + 5.0)], s);
            p.text(c, Align2::CENTER_CENTER, "A", regular(9.5), col);
        }
        LeadGlyph::ParagraphSpacing => {
            for dy in [-5.0, -2.5, 2.5, 5.0] {
                p.line_segment([pos2(c.x - 5.0, c.y + dy), pos2(c.x + 5.0, c.y + dy)], s);
            }
        }
    }
}

/// A text field with a leading glyph (line height, letter spacing…). Returns the committed text.
fn lead_field(ui: &mut Ui, id: Id, glyph: LeadGlyph, value: &str, width: f32, tip: &str) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, FIELD_H), Sense::hover());
    let lead = Rect::from_min_size(rect.min, vec2(24.0, FIELD_H));
    let text_rect = Rect::from_min_max(pos2(lead.max.x, rect.min.y), rect.max);
    let editing_id = id.with("editing");
    let mut editing: Option<String> = ui.data(|d| d.get_temp(editing_id));
    let resp = ui.interact(rect, id.with("click"), Sense::click()).on_hover_text(tip);
    ui.painter().rect_filled(rect, CornerRadius::same(5), t.field);
    if editing.is_some() {
        ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, t.accent), StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, t.field_hover_border), StrokeKind::Inside);
    }
    paint_lead(ui.painter(), lead, glyph, t.icon_secondary);
    if editing.is_none() && resp.clicked() {
        editing = Some(if value == MIXED { String::new() } else { value.to_string() });
        ui.memory_mut(|m| m.request_focus(id.with("te")));
    }
    let mut out = None;
    match editing {
        Some(mut text) => {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text_rect.shrink2(vec2(2.0, 4.0))));
            let r = child.add(
                egui::TextEdit::singleline(&mut text)
                    .id(id.with("te"))
                    .frame(egui::Frame::NONE)
                    .font(regular(11.0))
                    .desired_width(text_rect.width() - 6.0),
            );
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if esc {
                ui.data_mut(|d| d.remove::<String>(editing_id));
            } else if r.lost_focus() || enter {
                out = Some(text);
                ui.data_mut(|d| d.remove::<String>(editing_id));
            } else {
                ui.data_mut(|d| d.insert_temp(editing_id, text));
            }
        }
        None => {
            let col = if value == MIXED { t.text_tertiary } else { t.text };
            ui.painter().text(pos2(text_rect.min.x + 2.0, rect.center().y), Align2::LEFT_CENTER, value, regular(11.0), col);
        }
    }
    out
}

/// "Auto", "24", "150%" → the `lineHeight` value. A bare number keeps the current unit when it is
/// a percentage, else means pixels.
fn parse_line_height(s: &str, cur: Option<LineHeight>) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        return Some(json!("auto"));
    }
    if let Some(p) = s.strip_suffix('%') {
        let cur_pct = match cur {
            Some(LineHeight::Percent(p)) => Some(p),
            _ => None,
        };
        return widgets::eval(p.trim(), cur_pct).filter(|v| v.is_finite()).map(|v| json!(format!("{}%", trim_num(v.max(0.0)))));
    }
    if let Some(px) = s.strip_suffix("px") {
        return widgets::eval(px.trim(), None).filter(|v| v.is_finite()).map(|v| json!(v.max(0.0)));
    }
    match cur {
        Some(LineHeight::Percent(p)) => widgets::eval(s, Some(p)).filter(|v| v.is_finite()).map(|v| json!(format!("{}%", trim_num(v.max(0.0))))),
        Some(LineHeight::Pixels(px)) => widgets::eval(s, Some(px)).filter(|v| v.is_finite()).map(|v| json!(v.max(0.0))),
        _ => widgets::eval(s, None).filter(|v| v.is_finite()).map(|v| json!(v.max(0.0))),
    }
}

/// "2%", "1.5px", "-0.5" → the `letterSpacing` value. A bare number keeps the current unit
/// (percent by default).
fn parse_letter_spacing(s: &str, cur: Option<LetterSpacing>) -> Option<Value> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        return widgets::eval(p.trim(), None).filter(|v| v.is_finite()).map(|v| json!(format!("{}%", trim_num(v))));
    }
    if let Some(p) = s.strip_suffix("px") {
        return widgets::eval(p.trim(), None).filter(|v| v.is_finite()).map(|v| json!(v));
    }
    match cur {
        Some(LetterSpacing::Pixels(px)) => widgets::eval(s, Some(px)).filter(|v| v.is_finite()).map(|v| json!(v)),
        Some(LetterSpacing::Percent(p)) => widgets::eval(s, Some(p)).filter(|v| v.is_finite()).map(|v| json!(format!("{}%", trim_num(v)))),
        None => widgets::eval(s, None).filter(|v| v.is_finite()).map(|v| json!(format!("{}%", trim_num(v)))),
    }
}

// ---- Font picker ----

const RECENT_KEY: &str = "figcraft_recent_fonts";
const ROW: f32 = 26.0;

fn recent_families(ctx: &egui::Context) -> Vec<String> {
    ctx.data_mut(|d| d.get_persisted::<Vec<String>>(Id::new(RECENT_KEY))).unwrap_or_default()
}

fn remember_family(ctx: &egui::Context, family: &str) {
    ctx.data_mut(|d| {
        let v = d.get_persisted_mut_or_default::<Vec<String>>(Id::new(RECENT_KEY));
        v.retain(|f| !f.eq_ignore_ascii_case(family));
        v.insert(0, family.to_string());
        v.truncate(8);
    });
}

enum PickRow {
    Header(&'static str),
    Family(String),
}

/// The searchable family list. Returns the family picked.
fn font_picker(ui: &mut Ui, current: Option<&str>, just_opened: bool) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let q_id = Id::new("font_query");
    let mut q: String = ui.data(|d| d.get_temp(q_id)).unwrap_or_default();
    if just_opened {
        q.clear();
    }
    let search = ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search fonts").desired_width(ui.available_width()).font(regular(11.5)));
    if just_opened {
        search.request_focus();
    }
    ui.data_mut(|d| d.insert_temp(q_id, q.clone()));
    let ql = q.trim().to_lowercase();
    let all = figcraft_text::family_names();
    let matches: Vec<String> = all.iter().filter(|f| ql.is_empty() || f.to_lowercase().contains(&ql)).cloned().collect();
    if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        return matches.into_iter().next();
    }
    let mut rows: Vec<PickRow> = Vec::new();
    if ql.is_empty() {
        let recent: Vec<String> = recent_families(ui.ctx()).into_iter().filter(|f| all.iter().any(|a| a.eq_ignore_ascii_case(f))).collect();
        if !recent.is_empty() {
            rows.push(PickRow::Header("Recent"));
            rows.extend(recent.into_iter().map(PickRow::Family));
            rows.push(PickRow::Header("All fonts"));
        }
    }
    if matches.is_empty() {
        widgets::label(ui, "No fonts match", t.text_tertiary);
        return None;
    }
    rows.extend(matches.into_iter().map(PickRow::Family));
    let previews = Previews::get(ui.ctx());
    let mut budget = 4;
    let mut picked = None;
    egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show_rows(ui, ROW, rows.len(), |ui, range| {
        for r in rows.get(range).unwrap_or(&[]) {
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW), if matches!(r, PickRow::Family(_)) { Sense::click() } else { Sense::hover() });
            match r {
                PickRow::Header(h) => {
                    ui.painter().text(pos2(rect.min.x + 8.0, rect.center().y), Align2::LEFT_CENTER, *h, semibold(10.5), t.text_secondary);
                }
                PickRow::Family(f) => {
                    let hovered = resp.hovered();
                    if hovered {
                        ui.painter().rect_filled(rect, CornerRadius::same(5), t.accent);
                    }
                    let col = if hovered { Color32::WHITE } else { t.text };
                    if current.is_some_and(|c| c.eq_ignore_ascii_case(f)) {
                        icons::paint(
                            ui.painter(),
                            Rect::from_center_size(pos2(rect.min.x + 10.0, rect.center().y), vec2(12.0, 12.0)),
                            Icon::Check,
                            col,
                        );
                    }
                    let text_min = pos2(rect.min.x + 22.0, rect.min.y);
                    match previews.texture(ui.ctx(), f, &mut budget) {
                        Some(tex) => {
                            let ppp = ui.ctx().pixels_per_point().max(0.5);
                            let size = tex.size_vec2() / ppp;
                            let avail = rect.max.x - text_min.x - 6.0;
                            let shown = vec2(size.x.min(avail), size.y.min(ROW));
                            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(shown.x / size.x.max(1.0), shown.y / size.y.max(1.0)));
                            let r = Rect::from_min_size(pos2(text_min.x, rect.center().y - shown.y / 2.0), shown);
                            ui.painter().image(tex.id(), r, uv, col);
                        }
                        None => {
                            ui.painter().text(pos2(text_min.x, rect.center().y), Align2::LEFT_CENTER, f, regular(12.0), col);
                        }
                    }
                    if resp.clicked() {
                        picked = Some(f.clone());
                    }
                }
            }
        }
    });
    if budget == 0 {
        ui.ctx().request_repaint();
    }
    picked
}

/// Family-name previews drawn in their own font by FigCraft's text engine (rendered lazily, a
/// few per frame, cached for the session). `None` = no usable preview: the name shows plain.
#[derive(Clone, Default)]
struct Previews(Arc<Mutex<HashMap<String, Option<egui::TextureHandle>>>>);

impl Previews {
    fn get(ctx: &egui::Context) -> Previews {
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Previews>(Id::new("font_previews")).clone())
    }

    fn texture(&self, ctx: &egui::Context, family: &str, budget: &mut usize) -> Option<egui::TextureHandle> {
        let mut map = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(t) = map.get(family) {
            return t.clone();
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let tex = render_preview(ctx, family);
        if map.len() > 2000 {
            map.clear();
        }
        map.insert(family.to_string(), tex.clone());
        tex
    }
}

fn render_preview(ctx: &egui::Context, family: &str) -> Option<egui::TextureHandle> {
    // Only when the family resolves to itself and can draw its own name (not symbol fonts).
    let styles = figcraft_text::family_styles(family);
    let style = styles.iter().find(|s| s.eq_ignore_ascii_case("Regular")).or(styles.first())?.clone();
    let face = figcraft_text::find_face(family, &style)?;
    if !face.family.eq_ignore_ascii_case(family) || !family.chars().all(|c| c == ' ' || face.has_char(c)) {
        return None;
    }
    let mut doc = Document::new("preview");
    let page = doc.first_page()?;
    let id = doc.alloc_id();
    let mut td = TextData::new(family);
    td.style.font_family = family.to_string();
    td.style.font_style = style;
    td.style.font_size = 13.0;
    let mut n = Node::new(id, NodeKind::Text(td), "preview", 1.0, 1.0);
    n.fills = vec![Paint::solid(Color::WHITE)];
    let (w, h) = figcraft_text::auto_size(&n)?;
    n.width = w.min(400.0);
    n.height = h.min(40.0);
    doc.insert(n, page, None).ok()?;
    let ppp = f64::from(ctx.pixels_per_point()).clamp(1.0, 3.0);
    let pix = figcraft_render::render_node(&doc, id, ppp)?;
    let (pw, ph) = (pix.width() as usize, pix.height() as usize);
    let rgba = figcraft_render::to_rgba8(&pix);
    if rgba.len() != pw * ph * 4 {
        return None;
    }
    let img = egui::ColorImage::from_rgba_unmultiplied([pw, ph], &rgba);
    Some(ctx.load_texture(format!("font-preview-{family}"), img, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_line_height_and_letter_spacing() {
        assert_eq!(parse_line_height("Auto", None), Some(json!("auto")));
        assert_eq!(parse_line_height("", Some(LineHeight::Pixels(20.0))), Some(json!("auto")));
        assert_eq!(parse_line_height("150%", Some(LineHeight::Auto)), Some(json!("150%")));
        assert_eq!(parse_line_height("24", Some(LineHeight::Auto)), Some(json!(24.0)));
        assert_eq!(parse_line_height("24px", Some(LineHeight::Percent(120.0))), Some(json!(24.0)));
        assert_eq!(parse_line_height("130", Some(LineHeight::Percent(120.0))), Some(json!("130%")));
        assert_eq!(parse_line_height("-5", Some(LineHeight::Pixels(10.0))), Some(json!(0.0)));
        assert_eq!(parse_line_height("abc", None), None);
        assert_eq!(parse_letter_spacing("2%", None), Some(json!("2%")));
        assert_eq!(parse_letter_spacing("1.5px", None), Some(json!(1.5)));
        assert_eq!(parse_letter_spacing("-1", None), Some(json!("-1%")));
        assert_eq!(parse_letter_spacing("3", Some(LetterSpacing::Pixels(1.0))), Some(json!(3.0)));
        assert_eq!(parse_letter_spacing("x", Some(LetterSpacing::Percent(0.0))), None);
    }

    #[test]
    fn axis_values_round_by_axis_width() {
        let axis = |min: f64, max: f64| figcraft_text::Axis { tag: "test".into(), name: "Test".into(), min, default: min, max, hidden: false };
        assert_eq!(round_axis(&axis(100.0, 900.0), 456.6), 457.0);
        assert_eq!(round_axis(&axis(-10.0, 0.0), -4.26), -4.3);
        assert_eq!(round_axis(&axis(0.0, 1.0), 0.04), 0.0);
    }

    #[test]
    fn uniform_reports_mixed() {
        let a = TextStyle::default();
        let b = TextStyle { font_size: 30.0, ..Default::default() };
        assert_eq!(uniform(&[a.clone(), b.clone()], |s| s.font_family.clone()), Some("Inter".to_string()));
        assert_eq!(uniform(&[a.clone(), b], |s| s.font_size), None);
        assert_eq!(uniform(&[], |s: &TextStyle| s.font_size), None);
        // Ranges pick only the spans they touch.
        let mut doc = Document::new("t");
        let page = doc.first_page().unwrap();
        let id = doc.alloc_id();
        let mut td = TextData::new("Hello world");
        td.restyle(6, 11, |s| s.font_size = 40.0);
        doc.insert(Node::new(id, NodeKind::Text(td), "t", 10.0, 10.0), page, None).unwrap();
        assert_eq!(target_styles(&doc, &[id], Some((id, 0, 5))).len(), 1);
        assert_eq!(target_styles(&doc, &[id], Some((id, 4, 8))).len(), 2);
        assert_eq!(target_styles(&doc, &[id], Some((id, 3, 3))).len(), 2);
        assert_eq!(target_styles(&doc, &[id], None).len(), 2);
    }
}
