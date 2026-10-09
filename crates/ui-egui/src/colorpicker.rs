//! The colour picker popover: paint type, saturation/brightness square, hue and alpha sliders,
//! hex and RGB fields, gradient stops, image fills and document colours.

use egui::{Align2, Color32, CornerRadius, Id, Mesh, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_color::{Color, ColorStop, Gradient, GradientKind, Hsb, ImageScaleMode, Paint, PaintKind};
use figcraft_doc::{EffectKind, NodeId, NodeKind};
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, c32, regular, semibold};
use crate::widgets;

#[derive(Clone, Debug, PartialEq)]
pub enum PickerTarget {
    PageBackground,
    Paint { fills: bool, index: usize, ids: Vec<NodeId> },
    Effect { ids: Vec<NodeId>, index: usize },
}

fn current_paint(app: &FigApp, target: &PickerTarget) -> Option<Paint> {
    let doc = app.session.doc().ok()?;
    match target {
        PickerTarget::PageBackground => match &doc.node(app.session.page().ok()?)?.kind {
            NodeKind::Page { background, .. } => Some(Paint::solid(*background)),
            _ => None,
        },
        PickerTarget::Paint { fills, index, ids } => {
            let n = doc.node(*ids.first()?)?;
            let list = if *fills { &n.fills } else { &n.strokes };
            list.get(*index).cloned()
        }
        PickerTarget::Effect { ids, index } => match &doc.node(*ids.first()?)?.effects.get(*index)?.kind {
            EffectKind::DropShadow(s) | EffectKind::InnerShadow(s) => Some(Paint::solid(s.color)),
            _ => None,
        },
    }
}

/// Write `paint` to the target. `live` edits are part of an ongoing drag.
fn write(app: &mut FigApp, target: &PickerTarget, paint: &Paint, live: bool) {
    if live {
        let _ = app.session.begin_interaction("Color");
    }
    let r = match target {
        PickerTarget::PageBackground => {
            let c = paint.solid_color().unwrap_or(Color::DEFAULT_PAGE);
            app.session.execute("page.setBackground", &json!({"color": c.to_css_hex()}))
        }
        PickerTarget::Paint { fills, index, ids } => {
            app.session.execute(if *fills { "fill.set" } else { "stroke.set" }, &json!({"ids": ids, "index": index, "paint": paint}))
        }
        PickerTarget::Effect { ids, index } => {
            let Some(e) = ids.first().and_then(|i| app.session.doc().ok()?.node(*i)?.effects.get(*index).cloned()) else { return };
            let c = paint.solid_color().unwrap_or(Color::BLACK);
            let mut e2 = e;
            if let EffectKind::DropShadow(s) | EffectKind::InnerShadow(s) = &mut e2.kind {
                s.color = c;
            }
            app.session.execute("effect.set", &json!({"ids": ids, "index": index, "effect": e2}))
        }
    };
    if let Err(e) = r {
        app.toast(e.to_string());
    }
    if let Some(c) = paint.solid_color() {
        app.session.recent_colors.retain(|x| *x != c);
        app.session.recent_colors.insert(0, c);
        app.session.recent_colors.truncate(16);
    }
}

fn finish(app: &mut FigApp) {
    if app.session.ds().map(|d| d.interaction.is_some()).unwrap_or(false) {
        let _ = app.session.commit_interaction();
    }
}

/// The selected gradient stop in the picker (shared with the on-canvas stops).
pub fn stop_id() -> Id {
    Id::new("cp_stop")
}

/// Apply a swatch colour: a solid paint's colour, or the selected gradient stop's.
fn set_color(app: &mut FigApp, target: &PickerTarget, paint: &mut Paint, c: Color, stop: Id, ui: &Ui) {
    match &mut paint.kind {
        PaintKind::Solid { color } => {
            *color = c.with_alpha(1.0);
            paint.opacity = c.a;
        }
        PaintKind::Gradient(g) => {
            let sel: usize = ui.data(|d| d.get_temp(stop)).unwrap_or(0);
            if let Some(s) = g.stops.get_mut(sel) {
                s.color = c;
            }
        }
        PaintKind::Image(_) => return,
    }
    write(app, target, paint, false);
}

/// The page's most used solid colours, recomputed only when the document snapshot changes.
fn page_colors(app: &FigApp, ui: &Ui) -> Vec<Color> {
    let Ok(ds) = app.session.ds() else { return vec![] };
    let key = (std::sync::Arc::as_ptr(&ds.doc) as usize, ds.page);
    let cache_id = Id::new("cp_doc_colors");
    if let Some((k, colors)) = ui.data(|d| d.get_temp::<((usize, figcraft_doc::NodeId), Vec<Color>)>(cache_id))
        && k == key
    {
        return colors;
    }
    let colors: Vec<Color> = figcraft_engine::cmd::styles::document_colors(&ds.doc, ds.page).into_iter().take(21).map(|(c, _)| c).collect();
    ui.data_mut(|d| d.insert_temp(cache_id, (key, colors.clone())));
    colors
}

/// Colour models the value row can show.
const MODELS: [&str; 5] = ["Hex", "RGB", "HSL", "HSB", "CSS"];

/// The model dropdown with its fields and the alpha field. Returns a committed colour.
fn model_fields(ui: &mut Ui, c: Color) -> Option<Color> {
    let model_id = Id::new("cp_model");
    let model: usize = ui.data(|d| d.get_temp(model_id)).unwrap_or(0).min(MODELS.len() - 1);
    let mut out: Option<Color> = None;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        let resp = widgets::dropdown(ui, MODELS.get(model).copied().unwrap_or("Hex"), 52.0, None);
        egui::Popup::menu(&resp).show(|ui| {
            for (i, m) in MODELS.iter().enumerate() {
                if crate::right::menu_item(ui, m, "") {
                    ui.data_mut(|d| d.insert_temp(model_id, i));
                    ui.close();
                }
            }
        });
        let num = |ui: &mut Ui, id: &str, v: f64, w: f32| match widgets::number_field(
            ui,
            Id::new(("cp_f", id)),
            widgets::Lead::None,
            Some(v.round()),
            "",
            w,
            1.0,
        ) {
            Some(widgets::FieldEdit::Set(x)) | Some(widgets::FieldEdit::Scrub(x, _)) => Some(x),
            None => None,
        };
        match MODELS.get(model).copied().unwrap_or("Hex") {
            "RGB" => {
                let [r, g, b, _] = c.to_rgba8();
                let vals = [f64::from(r), f64::from(g), f64::from(b)];
                for (k, v) in vals.iter().enumerate() {
                    if let Some(x) = num(ui, ["r", "g", "b"][k.min(2)], *v, 31.0) {
                        let mut rgb = vals;
                        if let Some(slot) = rgb.get_mut(k) {
                            *slot = x.clamp(0.0, 255.0);
                        }
                        out = Some(Color::new(rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, c.a));
                    }
                }
            }
            "HSL" | "HSB" => {
                let hsl = model == 2;
                let (h, s2, l2) = if hsl {
                    let x = figcraft_color::Hsl::from_color(c);
                    (x.h, x.s, x.l)
                } else {
                    let x = Hsb::from_color(c);
                    (x.h, x.s, x.b)
                };
                let vals = [f64::from(h), f64::from(s2) * 100.0, f64::from(l2) * 100.0];
                for (k, v) in vals.iter().enumerate() {
                    if let Some(x) = num(ui, ["h", "s", "l"][k.min(2)], *v, 31.0) {
                        let mut v3 = vals;
                        if let Some(slot) = v3.get_mut(k) {
                            *slot = if k == 0 { x.rem_euclid(360.0) } else { x.clamp(0.0, 100.0) };
                        }
                        let (h, a, b) = (v3[0] as f32, v3[1] as f32 / 100.0, v3[2] as f32 / 100.0);
                        let col = if hsl { figcraft_color::Hsl { h, s: a, l: b }.to_color() } else { Hsb { h, s: a, b }.to_color() };
                        out = Some(col.with_alpha(c.a));
                    }
                }
            }
            "CSS" => {
                if let Some(text) = widgets::text_field(ui, Id::new("cp_css"), &c.to_css(), 152.0, "") {
                    out = Color::parse_css(&text);
                }
            }
            _ => {
                if let Some(h) = widgets::text_field(ui, Id::new("cp_hex"), &c.to_hex(), 101.0, "") {
                    out = Color::from_hex(&h).map(|h| h.with_alpha(if h.a < 1.0 { h.a } else { c.a }));
                }
            }
        }
        if MODELS.get(model) != Some(&"CSS")
            && let Some(widgets::FieldEdit::Set(v)) =
                widgets::number_field(ui, Id::new("cp_a"), widgets::Lead::None, Some((f64::from(out.unwrap_or(c).a) * 100.0).round()), "%", 44.0, 1.0)
        {
            out = Some(out.unwrap_or(c).with_alpha((v / 100.0).clamp(0.0, 1.0) as f32));
        }
    });
    out
}

fn hue_color(h: f32) -> Color32 {
    c32(Hsb { h, s: 1.0, b: 1.0 }.to_color())
}

fn sv_square(ui: &mut Ui, rect: Rect, hsb: &mut Hsb) -> (bool, bool) {
    // White → hue horizontally, then transparent → black vertically.
    let mut m = Mesh::default();
    let hc = hue_color(hsb.h);
    let corners = [(rect.left_top(), Color32::WHITE), (rect.right_top(), hc), (rect.right_bottom(), hc), (rect.left_bottom(), Color32::WHITE)];
    for (p, c) in corners {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
    let mut m2 = Mesh::default();
    for (p, c) in [
        (rect.left_top(), Color32::TRANSPARENT),
        (rect.right_top(), Color32::TRANSPARENT),
        (rect.right_bottom(), Color32::BLACK),
        (rect.left_bottom(), Color32::BLACK),
    ] {
        m2.colored_vertex(p, c);
    }
    m2.add_triangle(0, 1, 2);
    m2.add_triangle(0, 2, 3);
    ui.painter().add(m2);
    let resp = ui.interact(rect, Id::new("cp_sv"), Sense::click_and_drag());
    let mut changed = false;
    if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(p) = resp.interact_pointer_pos()
    {
        hsb.s = ((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
        hsb.b = 1.0 - ((p.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
        changed = true;
    }
    let knob = pos2(rect.min.x + hsb.s * rect.width(), rect.min.y + (1.0 - hsb.b) * rect.height());
    ui.painter().circle_stroke(knob, 6.0, Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(knob, 7.0, Stroke::new(1.0, Color32::from_black_alpha(80)));
    (changed, resp.drag_stopped() || resp.clicked())
}

fn hue_slider(ui: &mut Ui, rect: Rect, hsb: &mut Hsb) -> (bool, bool) {
    let n = 12;
    let mut m = Mesh::default();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = rect.min.x + t * rect.width();
        let c = hue_color(t * 360.0);
        m.colored_vertex(pos2(x, rect.min.y), c);
        m.colored_vertex(pos2(x, rect.max.y), c);
        if i > 0 {
            let k = (i * 2) as u32;
            m.add_triangle(k - 2, k - 1, k);
            m.add_triangle(k - 1, k, k + 1);
        }
    }
    ui.painter().add(m);
    slider_knob(ui, rect, hsb.h / 360.0, Id::new("cp_hue"))
        .map(|(t, done)| {
            hsb.h = (t * 360.0).clamp(0.0, 359.9);
            (true, done)
        })
        .unwrap_or((false, false))
}

fn slider_knob(ui: &mut Ui, rect: Rect, t: f32, id: Id) -> Option<(f32, bool)> {
    let resp = ui.interact(rect.expand(4.0), id, Sense::click_and_drag());
    let x = rect.min.x + t.clamp(0.0, 1.0) * rect.width();
    ui.painter().circle_stroke(pos2(x, rect.center().y), rect.height() / 2.0 + 1.0, Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(pos2(x, rect.center().y), rect.height() / 2.0 + 2.0, Stroke::new(1.0, Color32::from_black_alpha(80)));
    if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(p) = resp.interact_pointer_pos()
    {
        return Some((((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0), resp.drag_stopped() || resp.clicked()));
    }
    if resp.drag_stopped() {
        return Some((t, true));
    }
    None
}

fn alpha_slider(ui: &mut Ui, rect: Rect, color: Color) -> Option<(f32, bool)> {
    widgets::checker(&ui.painter().with_clip_rect(rect), rect, rect.height() / 2.0);
    let mut m = Mesh::default();
    let opaque = c32(color.with_alpha(1.0));
    for (p, c) in [
        (rect.left_top(), Color32::TRANSPARENT),
        (rect.right_top(), opaque),
        (rect.right_bottom(), opaque),
        (rect.left_bottom(), Color32::TRANSPARENT),
    ] {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
    slider_knob(ui, rect, color.a, Id::new("cp_alpha"))
}

pub fn show(app: &mut FigApp, ctx: &egui::Context) {
    let Some(target) = app.ui.picker.clone() else { return };
    let Some(mut paint) = current_paint(app, &target) else {
        app.ui.picker = None;
        return;
    };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let pos = pos2(screen.max.x - app.ui.right_width - 260.0, 140.0);
    let mut open = true;
    let mut close_now = false;
    egui::Window::new("color_picker")
        .title_bar(false)
        .fixed_pos(pos)
        .fixed_size(vec2(240.0, 0.0))
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.panel).inner_margin(egui::Margin::same(0)))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            // Header.
            let (hr, _) = ui.allocate_exact_size(vec2(240.0, 40.0), Sense::hover());
            ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Custom", semibold(11.0), t.text);
            let cr = Rect::from_center_size(pos2(hr.max.x - 20.0, hr.center().y), vec2(24.0, 24.0));
            let cresp = ui.interact(cr, Id::new("cp_close"), Sense::click());
            icons::paint(ui.painter(), cr.shrink(6.0), Icon::Close, t.icon);
            if cresp.clicked() {
                close_now = true;
            }
            // Paint type.
            if !matches!(target, PickerTarget::Effect { .. } | PickerTarget::PageBackground) {
                let kinds = [
                    (Icon::Fill, "Solid"),
                    (Icon::LayoutH, "Linear"),
                    (Icon::Ellipse, "Radial"),
                    (Icon::Rotate, "Angular"),
                    (Icon::Instance, "Diamond"),
                    (Icon::Image, "Image"),
                ];
                let active = match &paint.kind {
                    PaintKind::Solid { .. } => 0,
                    PaintKind::Gradient(g) => match g.kind {
                        GradientKind::Linear => 1,
                        GradientKind::Radial => 2,
                        GradientKind::Angular => 3,
                        GradientKind::Diamond => 4,
                    },
                    PaintKind::Image(_) => 5,
                };
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    if let Some(i) = widgets::segmented(ui, &kinds, Some(active), 34.0) {
                        let base = paint.preview_color();
                        paint.kind = match i {
                            0 => PaintKind::Solid { color: base.with_alpha(1.0) },
                            5 => {
                                // Pick an image file.
                                match app.services.pick_open("Choose image", &["png", "jpg", "jpeg", "webp", "gif"]) {
                                    Some(p) => match std::fs::read(&p) {
                                        Ok(bytes) => {
                                            let hash = app.session.doc_mut().ok().and_then(|d| figcraft_format_add(d, bytes));
                                            match hash {
                                                Some(h) => Paint::image(h, ImageScaleMode::Fill).kind,
                                                None => paint.kind.clone(),
                                            }
                                        }
                                        Err(_) => paint.kind.clone(),
                                    },
                                    None => paint.kind.clone(),
                                }
                            }
                            k => {
                                let kind = [GradientKind::Linear, GradientKind::Radial, GradientKind::Angular, GradientKind::Diamond][k - 1];
                                match &paint.kind {
                                    PaintKind::Gradient(g) => {
                                        PaintKind::Gradient(Gradient { kind, ..Gradient::default_for(kind, base).with_stops(g.stops.clone()) })
                                    }
                                    _ => PaintKind::Gradient(Gradient::default_for(kind, base)),
                                }
                            }
                        };
                        write(app, &target, &paint, false);
                    }
                });
            }
            // Gradient stops.
            let mut edit_color: Option<Color> = match &paint.kind {
                PaintKind::Solid { color } => Some(color.with_alpha(color.a * paint.opacity)),
                _ => None,
            };
            let sel_stop_id = stop_id();
            if let PaintKind::Gradient(g) = &paint.kind {
                let mut g = g.clone();
                let mut sel: usize = ui.data(|d| d.get_temp(sel_stop_id)).unwrap_or(0).min(g.stops.len().saturating_sub(1));
                let (bar, _) = ui.allocate_exact_size(vec2(240.0, 28.0), Sense::hover());
                let bar = bar.shrink2(vec2(16.0, 6.0));
                widgets::checker(&ui.painter().with_clip_rect(bar), bar, 4.0);
                let n = 24;
                for i in 0..n {
                    let a = i as f32 / n as f32;
                    let b = (i + 1) as f32 / n as f32;
                    let r = Rect::from_min_max(pos2(bar.min.x + a * bar.width(), bar.min.y), pos2(bar.min.x + b * bar.width(), bar.max.y));
                    ui.painter().rect_filled(r, CornerRadius::ZERO, c32(g.sample((a + b) / 2.0)));
                }
                let mut changed = false;
                let mut done = false;
                for (i, s) in g.stops.clone().iter().enumerate() {
                    let x = bar.min.x + s.position * bar.width();
                    let r = Rect::from_center_size(pos2(x, bar.center().y), vec2(12.0, 18.0));
                    let resp = ui.interact(r, Id::new(("cp_stopk", i)), Sense::click_and_drag());
                    ui.painter().rect_filled(r, CornerRadius::same(3), c32(s.color.with_alpha(1.0)));
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::same(3),
                        Stroke::new(if i == sel { 2.0 } else { 1.0 }, if i == sel { crate::theme::ACCENT } else { Color32::WHITE }),
                        StrokeKind::Outside,
                    );
                    if resp.clicked() || resp.drag_started() {
                        sel = i;
                    }
                    if resp.dragged()
                        && let Some(p) = resp.interact_pointer_pos()
                        && let Some(st) = g.stops.get_mut(i)
                    {
                        st.position = ((p.x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
                        changed = true;
                    }
                    if resp.drag_stopped() {
                        done = true;
                    }
                }
                // Click on the bar adds a stop.
                let bresp = ui.interact(bar, Id::new("cp_bar"), Sense::click());
                if bresp.clicked()
                    && let Some(p) = bresp.interact_pointer_pos()
                {
                    let pos = ((p.x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
                    g.stops.push(ColorStop { position: pos, color: g.sample(pos) });
                    sel = g.stops.len() - 1;
                    changed = true;
                    done = true;
                }
                ui.data_mut(|d| d.insert_temp(sel_stop_id, sel));
                if changed {
                    paint.kind = PaintKind::Gradient(g.clone());
                    write(app, &target, &paint, !done);
                }
                if done {
                    finish(app);
                }
                edit_color = g.stops.get(sel).map(|s| s.color);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    if ui.small_button("Remove stop").clicked() && g.stops.len() > 2 {
                        g.stops.remove(sel);
                        ui.data_mut(|d| d.insert_temp(sel_stop_id, 0usize));
                        paint.kind = PaintKind::Gradient(g.clone());
                        write(app, &target, &paint, false);
                    }
                    if ui.small_button("Flip").clicked() {
                        for s in &mut g.stops {
                            s.position = 1.0 - s.position;
                        }
                        paint.kind = PaintKind::Gradient(g.clone());
                        write(app, &target, &paint, false);
                    }
                    if ui.small_button("Rotate 90°").clicked() {
                        let (fx, fy) = g.from;
                        let rot = |(x, y): (f64, f64)| (fx - (y - fy), fy + (x - fx));
                        g.to = rot(g.to);
                        g.width = rot(g.width);
                        paint.kind = PaintKind::Gradient(g.clone());
                        write(app, &target, &paint, false);
                    }
                });
            }
            if let PaintKind::Image(img) = &paint.kind {
                let mut img = img.clone();
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    let resp = widgets::dropdown(ui, &format!("{:?}", img.scale_mode), 100.0, None);
                    egui::Popup::menu(&resp).show(|ui| {
                        for (l, m) in [
                            ("Fill", ImageScaleMode::Fill),
                            ("Fit", ImageScaleMode::Fit),
                            ("Crop", ImageScaleMode::Crop),
                            ("Tile", ImageScaleMode::Tile),
                        ] {
                            if crate::right::menu_item(ui, l, "") {
                                img.scale_mode = m;
                                paint.kind = PaintKind::Image(img.clone());
                                write(app, &target, &paint, false);
                                ui.close();
                            }
                        }
                    });
                    if icons::button(ui, Icon::Rotate, 24.0, false, "Rotate 90°").clicked() {
                        img.rotation = (img.rotation + 1) % 4;
                        paint.kind = PaintKind::Image(img.clone());
                        write(app, &target, &paint, false);
                    }
                });
                for (label, val) in [
                    ("Exposure", img.filters.exposure),
                    ("Contrast", img.filters.contrast),
                    ("Saturation", img.filters.saturation),
                    ("Temperature", img.filters.temperature),
                    ("Tint", img.filters.tint),
                    ("Highlights", img.filters.highlights),
                    ("Shadows", img.filters.shadows),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(egui::RichText::new(label).font(regular(11.0)));
                        let mut v = val;
                        let r = ui.add(egui::Slider::new(&mut v, -1.0..=1.0).show_value(false));
                        if r.changed() {
                            let f = &mut img.filters;
                            match label {
                                "Exposure" => f.exposure = v,
                                "Contrast" => f.contrast = v,
                                "Saturation" => f.saturation = v,
                                "Temperature" => f.temperature = v,
                                "Tint" => f.tint = v,
                                "Highlights" => f.highlights = v,
                                _ => f.shadows = v,
                            }
                            paint.kind = PaintKind::Image(img.clone());
                            write(app, &target, &paint, !r.drag_stopped());
                        }
                        if r.drag_stopped() {
                            finish(app);
                        }
                    });
                }
            }
            if let Some(color) = edit_color {
                let mut hsb = ui
                    .data(|d| d.get_temp::<(f32, f32, f32, [u8; 4])>(Id::new("cp_hsb")))
                    .filter(|(_, _, _, rgba)| *rgba == color.to_rgba8())
                    .map(|(h, s, b, _)| Hsb { h, s, b })
                    .unwrap_or_else(|| Hsb::from_color(color));
                let (sq, _) = ui.allocate_exact_size(vec2(240.0, 240.0), Sense::hover());
                let (c1, d1) = sv_square(ui, sq, &mut hsb);
                let (hr, _) = ui.allocate_exact_size(vec2(240.0, 28.0), Sense::hover());
                let hr = Rect::from_min_max(pos2(hr.min.x + 48.0, hr.min.y + 8.0), pos2(hr.max.x - 16.0, hr.max.y - 8.0));
                if icons::button(
                    &mut ui.new_child(
                        egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(hr.min.x - 36.0, hr.center().y - 12.0), vec2(24.0, 24.0))),
                    ),
                    Icon::Eyedropper,
                    24.0,
                    false,
                    "Pick color (I)",
                )
                .clicked()
                {
                    app.ui.picker = None;
                    app.run("edit.pickColor", json!({}));
                    return;
                }
                let (c2, d2) = hue_slider(ui, hr, &mut hsb);
                let (ar, _) = ui.allocate_exact_size(vec2(240.0, 20.0), Sense::hover());
                let ar = Rect::from_min_max(pos2(ar.min.x + 48.0, ar.min.y + 2.0), pos2(ar.max.x - 16.0, ar.max.y - 6.0));
                let mut new_alpha = None;
                if let Some((a, d)) = alpha_slider(ui, ar, color) {
                    new_alpha = Some((a, d));
                }
                let mut nc = hsb.to_color().with_alpha(color.a);
                let mut done = d1 || d2;
                let mut changed = c1 || c2;
                if let Some((a, d)) = new_alpha {
                    nc = nc.with_alpha(a);
                    changed = true;
                    done |= d;
                }
                // Colour model fields (Hex, RGB, HSL, HSB or CSS) + alpha.
                if let Some(c) = model_fields(ui, nc) {
                    nc = c;
                    hsb = Hsb::from_color(nc);
                    changed = true;
                    done = true;
                }
                ui.data_mut(|d| d.insert_temp(Id::new("cp_hsb"), (hsb.h, hsb.s, hsb.b, nc.to_rgba8())));
                if changed {
                    match &mut paint.kind {
                        PaintKind::Solid { color } => {
                            *color = nc.with_alpha(1.0);
                            paint.opacity = nc.a;
                        }
                        PaintKind::Gradient(g) => {
                            let sel: usize = ui.data(|d| d.get_temp(sel_stop_id)).unwrap_or(0);
                            if let Some(s) = g.stops.get_mut(sel) {
                                s.color = nc;
                            }
                        }
                        PaintKind::Image(_) => {}
                    }
                    write(app, &target, &paint, !done);
                }
                if done {
                    finish(app);
                }
            }
            // Colours used on this page.
            let doc_colors = page_colors(app, ui);
            if !doc_colors.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    widgets::label(ui, "On this page", t.text_secondary);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(16.0);
                    for c in &doc_colors {
                        if widgets::swatch(ui, &Paint::solid(*c), 14.0).on_hover_text(c.to_hex()).clicked() {
                            set_color(app, &target, &mut paint, *c, sel_stop_id, ui);
                        }
                    }
                });
            }
            // Recent colours.
            let recent = app.session.recent_colors.clone();
            if !recent.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    widgets::label(ui, "Recent", t.text_secondary);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(16.0);
                    for c in recent.iter().take(14) {
                        if widgets::swatch(ui, &Paint::solid(*c), 14.0).clicked() {
                            set_color(app, &target, &mut paint, *c, sel_stop_id, ui);
                        }
                    }
                });
            }
            ui.add_space(12.0);
        });
    if !open || close_now || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        finish(app);
        app.ui.picker = None;
    }
    let _ = Pos2::ZERO;
}

fn figcraft_format_add(doc: &mut figcraft_doc::Document, bytes: Vec<u8>) -> Option<String> {
    // Images are keyed by content hash; the engine's image.place uses the same scheme.
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().ok()?.into_dimensions().ok()?;
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut s);
        format!("img{:016x}{:08x}", s.finish(), bytes.len())
    };
    doc.images.insert(hash.clone(), figcraft_doc::ImageData { bytes: std::sync::Arc::new(bytes), width: w, height: h });
    Some(hash)
}

trait WithStops {
    fn with_stops(self, stops: Vec<ColorStop>) -> Self;
}

impl WithStops for Gradient {
    fn with_stops(mut self, stops: Vec<ColorStop>) -> Self {
        if stops.len() >= 2 {
            self.stops = stops;
        }
        self
    }
}
