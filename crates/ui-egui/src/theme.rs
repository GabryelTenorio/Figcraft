//! Design tokens (UI3 light and dark) and fonts.

use std::sync::Arc;

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub canvas: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub icon: Color32,
    pub icon_secondary: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub field: Color32,
    pub field_hover_border: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_text: Color32,
    pub selected_row: Color32,
    pub selected_row_child: Color32,
    pub component: Color32,
    pub component_row: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub guide: Color32,
    pub tab_bar: Color32,
    pub tab_active: Color32,
    pub shadow: Color32,
    pub menu: Color32,
    pub menu_text: Color32,
    pub menu_hover: Color32,
    pub menu_dim: Color32,
}

pub const ACCENT: Color32 = Color32::from_rgb(0x0D, 0x99, 0xFF);
pub const COMPONENT: Color32 = Color32::from_rgb(0x97, 0x47, 0xFF);

impl Tokens {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            canvas: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            border: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            border_strong: Color32::from_rgb(0xD9, 0xD9, 0xD9),
            text: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            text_secondary: Color32::from_rgb(0x75, 0x75, 0x75),
            text_tertiary: Color32::from_rgb(0xB3, 0xB3, 0xB3),
            icon: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            icon_secondary: Color32::from_rgb(0x75, 0x75, 0x75),
            hover: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            pressed: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            field: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            field_hover_border: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            accent: ACCENT,
            accent_hover: Color32::from_rgb(0x00, 0x7B, 0xE5),
            accent_text: Color32::from_rgb(0x00, 0x7B, 0xE5),
            selected_row: Color32::from_rgb(0xE5, 0xF4, 0xFF),
            selected_row_child: Color32::from_rgb(0xF2, 0xF9, 0xFF),
            component: COMPONENT,
            component_row: Color32::from_rgb(0xF1, 0xE9, 0xFF),
            danger: Color32::from_rgb(0xF2, 0x48, 0x22),
            success: Color32::from_rgb(0x14, 0xAE, 0x5C),
            warning: Color32::from_rgb(0xFF, 0xCD, 0x29),
            guide: Color32::from_rgb(0xF2, 0x48, 0x22),
            tab_bar: Color32::from_rgb(0xE8, 0xE8, 0xE8),
            tab_active: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            shadow: Color32::from_black_alpha(40),
            menu: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            menu_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            menu_hover: ACCENT,
            menu_dim: Color32::from_rgb(0x9A, 0x9A, 0x9A),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(0x2C, 0x2C, 0x2C),
            panel: Color32::from_rgb(0x2C, 0x2C, 0x2C),
            canvas: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            border: Color32::from_rgb(0x44, 0x44, 0x44),
            border_strong: Color32::from_rgb(0x55, 0x55, 0x55),
            text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            text_secondary: Color32::from_rgb(0xB3, 0xB3, 0xB3),
            text_tertiary: Color32::from_rgb(0x75, 0x75, 0x75),
            icon: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            icon_secondary: Color32::from_rgb(0xB3, 0xB3, 0xB3),
            hover: Color32::from_rgb(0x38, 0x38, 0x38),
            pressed: Color32::from_rgb(0x44, 0x44, 0x44),
            field: Color32::from_rgb(0x38, 0x38, 0x38),
            field_hover_border: Color32::from_rgb(0x4D, 0x4D, 0x4D),
            accent: ACCENT,
            accent_hover: Color32::from_rgb(0x3D, 0xAD, 0xFF),
            accent_text: Color32::from_rgb(0x7C, 0xC4, 0xF8),
            selected_row: Color32::from_rgb(0x2A, 0x45, 0x5C),
            selected_row_child: Color32::from_rgb(0x2D, 0x37, 0x41),
            component: Color32::from_rgb(0xD2, 0x7C, 0xFF),
            component_row: Color32::from_rgb(0x3E, 0x30, 0x52),
            danger: Color32::from_rgb(0xF2, 0x48, 0x22),
            success: Color32::from_rgb(0x14, 0xAE, 0x5C),
            warning: Color32::from_rgb(0xFF, 0xCD, 0x29),
            guide: Color32::from_rgb(0xF2, 0x48, 0x22),
            tab_bar: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            tab_active: Color32::from_rgb(0x2C, 0x2C, 0x2C),
            shadow: Color32::from_black_alpha(90),
            menu: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            menu_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            menu_hover: ACCENT,
            menu_dim: Color32::from_rgb(0x9A, 0x9A, 0x9A),
        }
    }

    pub fn get(ctx: &Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::new("figcraft_tokens"))).unwrap_or_else(Tokens::light)
    }
}

/// Install tokens and egui visuals.
pub fn apply(ctx: &Context, t: Tokens) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("figcraft_tokens"), t));
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.hover;
    v.override_text_color = Some(t.text);
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_corner_radius = egui::CornerRadius::same(13);
    v.menu_corner_radius = egui::CornerRadius::same(13);
    v.selection.bg_fill = t.accent.gamma_multiply(0.3);
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.hyperlink_color = t.accent_text;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open, &mut v.widgets.noninteractive] {
        w.corner_radius = egui::CornerRadius::same(5);
        w.fg_stroke = Stroke::new(1.0, t.text);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.noninteractive.bg_fill = t.panel;
    v.widgets.inactive.bg_fill = t.field;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.field_hover_border);
    v.widgets.active.bg_fill = t.pressed;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.bg_stroke = Stroke::new(1.0, t.accent);
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: t.shadow };
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: t.shadow };
    v.text_cursor.stroke = Stroke::new(1.0, t.accent);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        s.spacing.menu_margin = egui::Margin::same(8);
        s.spacing.window_margin = egui::Margin::same(12);
        s.text_styles = [
            (TextStyle::Small, FontId::new(10.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(13.0, FontFamily::Name("semibold".into()))),
            (TextStyle::Monospace, FontId::new(11.0, FontFamily::Monospace)),
        ]
        .into();
        s.interaction.tooltip_delay = 0.4;
    });
}

/// UI fonts: Inter (from the craft-fonts build input) when present, else egui's defaults.
pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let find = |family: &str, style: &str| figcraft_text::CRAFT_FONTS.iter().find(|f| f.family == family && f.style == style).map(|f| f.bytes);
    let regular = find("Inter", "Regular");
    let semibold = find("Inter", "SemiBold").or_else(|| find("Inter", "Bold"));
    if let Some(r) = regular {
        fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(r)));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "inter".into());
        }
    }
    let sb_name = if let Some(sb) = semibold {
        fonts.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(sb)));
        "inter-semibold"
    } else {
        "inter"
    };
    let mut semi: Vec<String> = Vec::new();
    if fonts.font_data.contains_key(sb_name) {
        semi.push(sb_name.into());
    }
    semi.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("semibold".into()), semi);
    // CJK and other scripts from craft-fonts as fallbacks.
    for f in figcraft_text::CRAFT_FONTS.iter().filter(|f| f.family != "Inter" && f.style == "Regular") {
        if matches!(f.family, "BIZ UDPGothic" | "Noto Sans Arabic") {
            let key = format!("craft-{}", f.family);
            fonts.font_data.insert(key.clone(), Arc::new(FontData::from_static(f.bytes)));
            for fam in [FontFamily::Proportional, FontFamily::Name("semibold".into())] {
                if let Some(list) = fonts.families.get_mut(&fam) {
                    list.push(key.clone());
                }
            }
        }
    }
    ctx.set_fonts(fonts);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn c32(c: figcraft_color::Color) -> Color32 {
    let [r, g, b, a] = c.to_rgba8();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn screen_conversion_id() -> egui::Id {
    egui::Id::new("figcraft_screen_conversion")
}

/// Remember the colour profile of the document on screen (once per frame): document colours drawn
/// by the UI ([`doc_c32`]) go from it to the screen's sRGB.
pub fn set_doc_profile(ctx: &Context, profile: figcraft_color::ColorProfile) {
    let conv = profile.conversion_to(figcraft_color::ColorProfile::Srgb);
    ctx.data_mut(|d| d.insert_temp(screen_conversion_id(), conv));
}

/// A document colour (a paint, swatch, background) for the screen: converted from the document's
/// colour profile to sRGB. UI chrome colours use [`c32`].
pub fn doc_c32(ctx: &Context, c: figcraft_color::Color) -> Color32 {
    let conv = ctx.data(|d| d.get_temp::<figcraft_color::ProfileConversion>(screen_conversion_id()));
    c32(conv.map_or(c, |k| k.apply(c)))
}
