//! The home screen: new file, open, recent files, templates.

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{ACCENT, Tokens, regular, semibold};

pub fn show(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.panel)).show(ui, |ui| {
        let rect = ui.max_rect();
        let left = Rect::from_min_size(rect.min, vec2(240.0, rect.height()));
        ui.painter().line_segment([left.right_top(), left.right_bottom()], Stroke::new(1.0, t.border));
        // Sidebar.
        let mut y = left.min.y + 20.0;
        icons::paint(ui.painter(), Rect::from_min_size(pos2(left.min.x + 16.0, y), vec2(20.0, 20.0)), Icon::Logo, ACCENT);
        ui.painter().text(pos2(left.min.x + 44.0, y + 10.0), Align2::LEFT_CENTER, "FigCraft", semibold(14.0), t.text);
        y += 48.0;
        for (icon, label) in [(Icon::Home, "Recents"), (Icon::Page, "Drafts"), (Icon::Assets, "Community")] {
            let r = Rect::from_min_size(pos2(left.min.x + 8.0, y), vec2(224.0, 32.0));
            if label == "Recents" {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.selected_row);
            }
            icons::paint(ui.painter(), Rect::from_min_size(pos2(r.min.x + 10.0, r.center().y - 8.0), vec2(16.0, 16.0)), icon, t.icon);
            ui.painter().text(pos2(r.min.x + 36.0, r.center().y), Align2::LEFT_CENTER, label, regular(12.0), t.text);
            y += 34.0;
        }
        let main = Rect::from_min_max(pos2(left.max.x + 40.0, rect.min.y + 24.0), rect.max - vec2(40.0, 24.0));
        ui.painter().text(main.min, Align2::LEFT_TOP, "Recents", semibold(18.0), t.text);
        // Action cards.
        let mut x = main.min.x;
        let cy = main.min.y + 48.0;
        let mut new_file = false;
        let mut open = false;
        for (icon, title, sub) in
            [(Icon::Plus, "New design file", "Start from a blank canvas"), (Icon::Page, "Open file", "Open .figcraft, .svg or images")]
        {
            let r = Rect::from_min_size(pos2(x, cy), vec2(260.0, 72.0));
            let resp = ui.interact(r, ui.id().with(("card", title)), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { t.hover } else { t.panel });
            ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, t.border), StrokeKind::Inside);
            let ir = Rect::from_min_size(pos2(r.min.x + 16.0, r.center().y - 16.0), vec2(32.0, 32.0));
            ui.painter().rect_filled(ir, CornerRadius::same(6), ACCENT);
            icons::paint(ui.painter(), ir.shrink(8.0), icon, egui::Color32::WHITE);
            ui.painter().text(pos2(r.min.x + 60.0, r.center().y - 8.0), Align2::LEFT_CENTER, title, semibold(12.0), t.text);
            ui.painter().text(pos2(r.min.x + 60.0, r.center().y + 9.0), Align2::LEFT_CENTER, sub, regular(11.0), t.text_secondary);
            if resp.clicked() {
                if title.starts_with("New") {
                    new_file = true;
                } else {
                    open = true;
                }
            }
            x += 280.0;
        }
        if new_file {
            app.run("file.new", json!({}));
            app.ui.home = false;
        }
        if open {
            app.open_dialog();
        }
        // Open documents and recent files.
        let mut y = cy + 108.0;
        ui.painter().text(pos2(main.min.x, y), Align2::LEFT_TOP, "Open files", semibold(12.0), t.text_secondary);
        y += 28.0;
        let mut switch = None;
        for (i, d) in app.session.docs.iter().enumerate() {
            let r = Rect::from_min_size(pos2(main.min.x, y), vec2(main.width().min(640.0), 36.0));
            let resp = ui.interact(r, ui.id().with(("open", d.uid)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
            }
            icons::paint(ui.painter(), Rect::from_min_size(pos2(r.min.x + 8.0, r.center().y - 8.0), vec2(16.0, 16.0)), Icon::Logo, ACCENT);
            ui.painter().text(pos2(r.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, &d.doc.name, regular(12.0), t.text);
            if let Some(p) = &d.path {
                ui.painter().text(pos2(r.max.x - 8.0, r.center().y), Align2::RIGHT_CENTER, p, regular(11.0), t.text_secondary);
            }
            if resp.clicked() {
                switch = Some(i);
            }
            y += 38.0;
        }
        if let Some(i) = switch {
            app.run("doc.switch", json!({"index": i}));
            app.ui.home = false;
        }
        y += 16.0;
        let recents = app.session.prefs.recent_files.clone();
        if !recents.is_empty() {
            ui.painter().text(pos2(main.min.x, y), Align2::LEFT_TOP, "Recent files", semibold(12.0), t.text_secondary);
            y += 28.0;
            let mut open_path = None;
            for p in recents.iter().take(12) {
                let r = Rect::from_min_size(pos2(main.min.x, y), vec2(main.width().min(640.0), 32.0));
                let resp = ui.interact(r, ui.id().with(("recent", p.as_str())), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
                }
                let name = std::path::Path::new(p).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
                ui.painter().text(pos2(r.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, name, regular(12.0), t.text);
                ui.painter().text(pos2(r.max.x - 8.0, r.center().y), Align2::RIGHT_CENTER, p, regular(10.0), t.text_tertiary);
                if resp.clicked() {
                    open_path = Some(p.clone());
                }
                y += 34.0;
            }
            if let Some(p) = open_path
                && app.run("file.open", json!({"path": p})).is_some()
            {
                app.ui.home = false;
            }
        }
    });
}
