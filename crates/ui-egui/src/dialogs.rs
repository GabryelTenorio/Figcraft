//! Dialogs: About, Preferences, keyboard shortcuts, find & replace, variables, export.

use egui::{Align2, Color32, Context, CornerRadius, RichText, Sense, Stroke, vec2};
use figcraft_doc::{ExportFormat, ExportSetting, ExportSize};
use figcraft_engine::catalog::MENU;
use figcraft_engine::cmd::file::{export_jobs, exportable_layers};
use serde_json::json;

use crate::FigApp;
use crate::export_panel::{export_layers, save_one, suggested_name};
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

/// What the Export dialog was asked to do.
enum ExportChoice {
    /// Every layer with export settings, with those settings.
    Layers,
    /// The layers together as one file.
    Once(ExportSetting),
}

/// The Export dialog: the layers' own export settings when they have some, and one-off presets for
/// the selection (or the page) as one file.
fn export_dialog(app: &mut FigApp, ctx: &Context) {
    let t = Tokens::get(ctx);
    let mut open = true;
    let sel = app.session.selection();
    let Ok(ds) = app.session.ds() else {
        app.ui.export_preview = false;
        return;
    };
    let (doc, page) = (ds.doc.clone(), ds.page);
    let ids = if sel.is_empty() { doc.node(page).map(|p| p.children.clone()).unwrap_or_default() } else { sel.clone() };
    // With nothing selected, every layer on the page that has export settings.
    let with_settings = if sel.is_empty() { exportable_layers(&doc, page) } else { sel };
    let jobs = export_jobs(&doc, &with_settings);
    let preset = |size: f64, format| ExportSetting { size: ExportSize::Scale { value: size }, suffix: String::new(), format };
    let presets = [
        preset(1.0, ExportFormat::Png),
        preset(2.0, ExportFormat::Png),
        preset(3.0, ExportFormat::Png),
        preset(2.0, ExportFormat::Jpg),
        preset(2.0, ExportFormat::Webp),
        preset(1.0, ExportFormat::Svg),
        preset(1.0, ExportFormat::Pdf),
    ];
    let mut choice = None;
    window("Export", ctx).open(&mut open).show(ctx, |ui| {
        ui.set_width(320.0);
        let caption = |ui: &mut egui::Ui, s: &str| {
            ui.label(RichText::new(s).font(regular(11.0)).color(t.text_secondary));
        };
        if !jobs.is_empty() {
            caption(ui, "With the layers' export settings");
            const SHOWN: usize = 6;
            for j in jobs.iter().take(SHOWN) {
                ui.label(RichText::new(&j.file).font(regular(12.0)).color(t.text));
            }
            if jobs.len() > SHOWN {
                caption(ui, &format!("and {} more", jobs.len() - SHOWN));
            }
            ui.add_space(6.0);
            let layers = jobs.iter().map(|j| j.id).collect::<std::collections::BTreeSet<_>>().len();
            let label = format!("Export {layers} layer{}", if layers == 1 { "" } else { "s" });
            if crate::widgets::button(ui, &label, true, true).clicked() {
                choice = Some(ExportChoice::Layers);
            }
            ui.add_space(10.0);
            let (r, _) = ui.allocate_exact_size(vec2(320.0, 1.0), Sense::hover());
            ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, t.border));
            ui.add_space(10.0);
        }
        caption(ui, &format!("{} layer{} as one file", ids.len(), if ids.len() == 1 { "" } else { "s" }));
        ui.add_space(4.0);
        for p in &presets {
            let label = if p.format.is_raster() { format!("{} @{}", p.format.label(), p.size.label()) } else { p.format.label().to_string() };
            let (r, resp) = ui.allocate_exact_size(vec2(320.0, 30.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
            }
            ui.painter().text(r.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, regular(12.0), t.text);
            if resp.clicked() {
                choice = Some(ExportChoice::Once(p.clone()));
            }
        }
    });
    let close = choice.is_some() || !open;
    match choice {
        Some(ExportChoice::Layers) => export_layers(app, &with_settings),
        Some(ExportChoice::Once(s)) if !ids.is_empty() => save_one(app, &ids, &s, &suggested_name(&doc, &ids, &s, "Export")),
        Some(ExportChoice::Once(_)) => app.toast("Nothing to export"),
        None => {}
    }
    if close {
        app.ui.export_preview = false;
    }
}
