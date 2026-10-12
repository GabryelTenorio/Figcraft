//! The home screen: new file, open, join with a code, recent files, and your online files.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::FigApp;
use crate::a11y;
use crate::i18n::{tr, tr_noop};
use crate::icons::{self, Icon};
use crate::theme::{ACCENT, Tokens, regular, semibold};
use crate::widgets;

/// A row of the home screen's file lists.
pub(crate) struct FileRow<'a> {
    /// The mark before the name, and its colour.
    pub mark: Option<(Icon, Color32)>,
    pub name: &'a str,
    pub ink: Color32,
    /// What its right end says (a path, where the file is), with its size and colour.
    pub detail: &'a str,
    pub detail_size: f32,
    pub detail_ink: Color32,
}

/// Paint `row` in `r` (highlighted while `resp` is hovered): the name gives way to the detail.
pub(crate) fn file_row(ui: &Ui, t: &Tokens, r: Rect, resp: &Response, row: &FileRow) {
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
    }
    if let Some((icon, tint)) = row.mark {
        icons::paint(ui.painter(), Rect::from_min_size(pos2(r.min.x + 8.0, r.center().y - 8.0), vec2(16.0, 16.0)), icon, tint);
    }
    let detail = ui.painter().text(pos2(r.max.x - 8.0, r.center().y), Align2::RIGHT_CENTER, row.detail, regular(row.detail_size), row.detail_ink);
    let name = widgets::elide_font(ui, row.name, (detail.min.x - r.min.x - 44.0).max(40.0), regular(12.0));
    ui.painter().text(pos2(r.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, name, regular(12.0), row.ink);
}

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
        // Recents and Online are the sections there are; the others are placeholders.
        for (icon, label, section) in [
            (Icon::Home, tr_noop("Recents"), Some(false)),
            (Icon::Online, tr_noop("Online"), Some(true)),
            (Icon::Page, tr_noop("Drafts"), None),
            (Icon::Assets, tr_noop("Community"), None),
        ] {
            let r = Rect::from_min_size(pos2(left.min.x + 8.0, y), vec2(224.0, 32.0));
            let selected = section == Some(app.ui.online.open);
            if let Some(online) = section {
                let resp = ui.interact(r, ui.id().with(("home_section", label)), Sense::click());
                a11y::tab(&resp, tr(label), selected);
                if resp.clicked() {
                    app.ui.online.open = online;
                }
                if resp.hovered() && !selected {
                    ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
                }
            }
            if selected {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.selected_row);
            }
            icons::paint(ui.painter(), Rect::from_min_size(pos2(r.min.x + 10.0, r.center().y - 8.0), vec2(16.0, 16.0)), icon, t.icon);
            ui.painter().text(pos2(r.min.x + 36.0, r.center().y), Align2::LEFT_CENTER, tr(label), regular(12.0), t.text);
            y += 34.0;
        }
        let main = Rect::from_min_max(pos2(left.max.x + 40.0, rect.min.y + 24.0), rect.max - vec2(40.0, 24.0));
        if app.ui.online.open {
            crate::online::section(app, ui, main, &t);
            opened_join(app);
            return;
        }
        ui.painter().text(main.min, Align2::LEFT_TOP, tr("Recents"), semibold(18.0), t.text);
        // Action cards.
        let mut x = main.min.x;
        let cy = main.min.y + 48.0;
        let mut new_file = false;
        let mut open = false;
        for (icon, title, sub) in [
            (Icon::Plus, tr_noop("New design file"), tr_noop("Start from a blank canvas")),
            (Icon::Page, tr_noop("Open file"), tr_noop("Open .figcraft, .svg or images")),
        ] {
            let r = Rect::from_min_size(pos2(x, cy), vec2(260.0, 72.0));
            let mut resp = ui.interact(r, ui.id().with(("card", title)), Sense::click());
            // One line each inside the card; a long translation ends with an ellipsis, in full on hover.
            let text_w = r.width() - 72.0;
            let (title_shown, sub_shown) = (widgets::elide_font(ui, tr(title), text_w, semibold(12.0)), widgets::elide(ui, tr(sub), text_w));
            if title_shown != tr(title) || sub_shown != tr(sub) {
                resp = resp.on_hover_text(format!("{}\n{}", tr(title), tr(sub)));
            }
            ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { t.hover } else { t.panel });
            ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, t.border), StrokeKind::Inside);
            let ir = Rect::from_min_size(pos2(r.min.x + 16.0, r.center().y - 16.0), vec2(32.0, 32.0));
            ui.painter().rect_filled(ir, CornerRadius::same(6), ACCENT);
            icons::paint(ui.painter(), ir.shrink(8.0), icon, egui::Color32::WHITE);
            ui.painter().text(pos2(r.min.x + 60.0, r.center().y - 8.0), Align2::LEFT_CENTER, title_shown, semibold(12.0), t.text);
            ui.painter().text(pos2(r.min.x + 60.0, r.center().y + 9.0), Align2::LEFT_CENTER, sub_shown, regular(11.0), t.text_secondary);
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
        // Join a file someone shared: the join field, and what the join is doing.
        let join = Rect::from_min_size(pos2(main.min.x, cy + 96.0), vec2(main.width().min(540.0), 74.0));
        ui.painter().text(join.min, Align2::LEFT_TOP, tr("Join with a code"), semibold(12.0), t.text_secondary);
        let field = Rect::from_min_max(pos2(join.min.x, join.min.y + 24.0), join.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(field).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
            if let Some(session) = crate::live::join_row(app, ui, 260.0) {
                app.ui.live.home_join = Some(session);
            }
            if let Some(session) = app.ui.live.home_join {
                crate::live::join_status(app, ui, &t, session);
            }
        });
        opened_join(app);
        // Open documents and recent files.
        let mut y = join.max.y + 22.0;
        ui.painter().text(pos2(main.min.x, y), Align2::LEFT_TOP, tr("Open files"), semibold(12.0), t.text_secondary);
        y += 28.0;
        let mut switch = None;
        for (i, d) in app.session.docs.iter().enumerate() {
            let r = Rect::from_min_size(pos2(main.min.x, y), vec2(main.width().min(640.0), 36.0));
            let resp = ui.interact(r, ui.id().with(("open", d.uid)), Sense::click());
            let detail = d.path.as_deref().unwrap_or_default();
            let row =
                FileRow { mark: Some((Icon::Logo, ACCENT)), name: &d.doc.name, ink: t.text, detail, detail_size: 11.0, detail_ink: t.text_secondary };
            file_row(ui, &t, r, &resp, &row);
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
            ui.painter().text(pos2(main.min.x, y), Align2::LEFT_TOP, tr("Recent files"), semibold(12.0), t.text_secondary);
            y += 28.0;
            let mut open_path = None;
            for p in recents.iter().take(12) {
                let r = Rect::from_min_size(pos2(main.min.x, y), vec2(main.width().min(640.0), 32.0));
                let resp = ui.interact(r, ui.id().with(("recent", p.as_str())), Sense::click());
                let name = std::path::Path::new(p).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
                file_row(
                    ui,
                    &t,
                    r,
                    &resp,
                    &FileRow { mark: None, name: &name, ink: t.text, detail: p, detail_size: 10.0, detail_ink: t.text_tertiary },
                );
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

/// The file joined from the home screen (a code, or one of your online files) opened in a tab of
/// its own: show it. (A join that was left is forgotten.)
fn opened_join(app: &mut FigApp) {
    match app.ui.live.home_join.map(|id| app.session.live.get(id).map(figcraft_engine::live::Live::syncing)) {
        Some(Some(true)) => {
            app.ui.live.home_join = None;
            app.ui.home = false;
        }
        Some(None) => app.ui.live.home_join = None,
        _ => {}
    }
}
