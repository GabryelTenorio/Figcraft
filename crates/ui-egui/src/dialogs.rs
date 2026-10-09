//! Dialogs: About, Preferences, keyboard shortcuts, find & replace, variables, export.

use egui::{Align2, Color32, Context, CornerRadius, RichText, Sense, Stroke, vec2};
use figcraft_engine::catalog::MENU;
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{ACCENT, Tokens, regular, semibold};
use crate::toolbar::pretty_shortcut;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn window<'a>(title: &'a str, ctx: &Context) -> egui::Window<'a> {
    let t = Tokens::get(ctx);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.panel).inner_margin(egui::Margin::same(20)))
}

pub fn show(app: &mut FigApp, ctx: &Context) {
    let t = Tokens::get(ctx);
    if app.ui.about {
        let mut open = true;
        window("About FigCraft", ctx).open(&mut open).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(10), ACCENT);
                icons::paint(ui.painter(), r.shrink(10.0), Icon::Logo, Color32::WHITE);
                ui.vertical(|ui| {
                    ui.label(RichText::new("FigCraft").font(semibold(18.0)));
                    ui.label(RichText::new(format!("Version {VERSION}")).color(t.text_secondary));
                });
            });
            ui.add_space(12.0);
            ui.label("Interface design and prototyping. A free, open-source, clean-room design tool written from scratch in Rust, part of the ArtCraft Crafting Apps.");
            ui.add_space(8.0);
            ui.hyperlink_to("getartcraft.com/apps/figcraft", "https://getartcraft.com/apps/figcraft");
            ui.hyperlink_to("Join us on Discord", "https://discord.gg/artcraft");
            ui.add_space(8.0);
            ui.label(RichText::new("Dual-licensed MIT OR Apache-2.0. Made by the ArtCraft team and community.").color(t.text_secondary).font(regular(11.0)));
            let credits = crate::credits::CONTRIBUTORS;
            if !credits.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("Contributors").font(semibold(12.0)));
                ui.label(credits.iter().map(|c| format!("@{c}")).collect::<Vec<_>>().join(" · "));
            }
        });
        if !open {
            app.ui.about = false;
        }
    }
    if app.ui.preferences {
        let mut open = true;
        window("Preferences", ctx).open(&mut open).show(ctx, |ui| {
            ui.set_width(360.0);
            let prefs = app.session.prefs.clone();
            let mut toggles = vec![];
            for (label, val, cmd) in [
                ("Keep tool selected after use", prefs.keep_tool, "pref.keepTool"),
                ("Highlight layers on hover", prefs.highlight_on_hover, "pref.highlightHover"),
                ("Rename duplicated layers", prefs.rename_duplicates, "pref.renameDuplicates"),
                ("Show dimensions on objects", prefs.show_dimensions, "pref.showDimensions"),
                ("Use number keys for opacity", prefs.number_keys_opacity, "pref.numberKeysOpacity"),
            ] {
                if crate::widgets::checkbox(ui, val, label) {
                    toggles.push(cmd);
                }
            }
            for c in toggles {
                app.run(c, json!({}));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Nudge amount");
                let mut small = prefs.nudge_small;
                let mut big = prefs.nudge_big;
                let a = ui.add(egui::DragValue::new(&mut small).range(0.1..=100.0));
                let b = ui.add(egui::DragValue::new(&mut big).range(1.0..=1000.0));
                if a.changed() || b.changed() {
                    app.run("pref.nudge", json!({"small": small, "big": big}));
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Theme");
                for (l, c) in [("Light", "view.themeLight"), ("Dark", "view.themeDark"), ("System", "view.themeSystem")] {
                    if ui.selectable_label(app.session.prefs.theme.eq_ignore_ascii_case(l), l).clicked() {
                        app.run(c, json!({}));
                    }
                }
            });
        });
        if !open {
            app.ui.preferences = false;
        }
    }
    if app.ui.shortcuts_panel {
        let mut open = true;
        egui::Window::new("Keyboard shortcuts").open(&mut open).default_size(vec2(720.0, 420.0)).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.columns(3, |cols| {
                    let items: Vec<_> = MENU.iter().filter(|m| !m.shortcut.is_empty()).collect();
                    let per = items.len().div_ceil(3);
                    for (ci, col) in cols.iter_mut().enumerate() {
                        for m in items.iter().skip(ci * per).take(per) {
                            col.horizontal(|ui| {
                                ui.label(RichText::new(m.label).font(regular(11.0)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(pretty_shortcut(m.shortcut)).color(t.text_secondary).font(regular(11.0)));
                                });
                            });
                        }
                    }
                });
            });
        });
        if !open {
            app.ui.shortcuts_panel = false;
        }
    }
    if let Some((mut find, mut replace)) = app.ui.find.clone() {
        let mut open = true;
        let mut action = None;
        egui::Window::new("Find and replace").open(&mut open).anchor(Align2::LEFT_TOP, vec2(260.0, 120.0)).resizable(false).show(ctx, |ui| {
            ui.set_width(280.0);
            ui.add(egui::TextEdit::singleline(&mut find).hint_text("Find").desired_width(260.0));
            ui.add(egui::TextEdit::singleline(&mut replace).hint_text("Replace with").desired_width(260.0));
            ui.horizontal(|ui| {
                if ui.button("Find").clicked() {
                    action = Some(false);
                }
                if ui.button("Replace all").clicked() {
                    action = Some(true);
                }
            });
        });
        if let Some(rep) = action
            && !find.is_empty()
        {
            let r = if rep {
                app.run("edit.findReplace", json!({"find": find, "replace": replace, "layerNames": false}))
            } else {
                app.run("edit.findReplace", json!({"find": find, "layerNames": true}))
            };
            if let Some(v) = r {
                let n = v["matches"].as_array().map(|a| a.len()).unwrap_or(0);
                app.toast(format!("{n} match{}", if n == 1 { "" } else { "es" }));
            }
        }
        app.ui.find = if open { Some((find, replace)) } else { None };
    }
    crate::variables_panel::show(app, ctx);
    if app.ui.export_preview {
        export_dialog(app, ctx);
    }
}

fn export_dialog(app: &mut FigApp, ctx: &Context) {
    let t = Tokens::get(ctx);
    let mut open = true;
    let sel = app.session.selection();
    let page_children: Vec<_> = app.session.ds().ok().and_then(|d| d.doc.node(d.page).map(|p| p.children.clone())).unwrap_or_default();
    let ids = if sel.is_empty() { page_children } else { sel };
    let mut do_export: Option<(&str, f64)> = None;
    window("Export", ctx).open(&mut open).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.label(RichText::new(format!("{} layer{}", ids.len(), if ids.len() == 1 { "" } else { "s" })).color(t.text_secondary));
        ui.add_space(8.0);
        for (label, fmt, scale) in [
            ("PNG @1x", "png", 1.0),
            ("PNG @2x", "png", 2.0),
            ("PNG @3x", "png", 3.0),
            ("JPG @2x", "jpg", 2.0),
            ("WebP @2x", "webp", 2.0),
            ("SVG", "svg", 1.0),
        ] {
            let (r, resp) = ui.allocate_exact_size(vec2(320.0, 30.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
            }
            ui.painter().text(r.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, regular(12.0), t.text);
            if resp.clicked() {
                do_export = Some((fmt, scale));
            }
        }
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(320.0, 1.0), Sense::hover());
        ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, t.border));
    });
    if let Some((fmt, scale)) = do_export {
        let name = ids.first().and_then(|i| app.session.doc().ok()?.node(*i).map(|n| n.name.clone())).unwrap_or_else(|| "Export".into());
        let sfx = if fmt != "svg" && scale != 1.0 { format!("@{scale}x") } else { String::new() };
        if let Some(p) = app.services.pick_save("Export", &format!("{name}{sfx}.{fmt}"), &[fmt]) {
            let ps = p.to_string_lossy().to_string();
            if app.run("file.export", json!({"ids": ids, "format": fmt, "scale": scale, "path": ps})).is_some() {
                app.toast("Exported");
            }
        }
        open = false;
    }
    if !open {
        app.ui.export_preview = false;
    }
}
