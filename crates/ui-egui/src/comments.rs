//! Comments: pins on the canvas, the thread popover, the new-comment composer and the Comments
//! panel shown in place of the Design panel while the Comment tool is active.

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::Comment;
use figcraft_engine::ToolId;
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{ACCENT, Tokens, regular, semibold};

const PIN: f32 = 28.0;
const POPOVER_W: f32 = 300.0;

/// Comment UI state.
#[derive(Default)]
pub struct CommentUi {
    /// The open thread.
    pub open: Option<u64>,
    /// A new comment being written at a page point.
    pub draft: Option<(f64, f64, String)>,
    pub reply: String,
    pub show_resolved: bool,
    pub search: String,
}

/// Pins are visible with the Comment tool, or when View › Comments is on.
fn visible(app: &FigApp) -> bool {
    app.session.tool.tool == ToolId::Comment
        || app.session.ds().is_ok_and(|ds| ds.view.comments)
        || app.ui.comments.open.is_some()
        || app.ui.comments.draft.is_some()
}

/// Comments on the current page that should show a pin, with their screen anchor (bottom-left).
fn pins(app: &FigApp) -> Vec<(Comment, Pos2)> {
    let Ok(ds) = app.session.ds() else { return Vec::new() };
    ds.doc
        .comments
        .iter()
        .filter(|c| c.page == ds.page)
        .filter(|c| !c.resolved || app.ui.comments.show_resolved || app.ui.comments.open == Some(c.id))
        .map(|c| {
            let (x, y) = c.position(&ds.doc);
            (c.clone(), app.to_screen(x, y))
        })
        .collect()
}

fn pin_rect(anchor: Pos2) -> Rect {
    Rect::from_min_max(pos2(anchor.x, anchor.y - PIN), pos2(anchor.x + PIN, anchor.y))
}

/// A stable colour per author.
pub fn author_color(name: &str) -> Color32 {
    const PALETTE: [Color32; 8] = [
        Color32::from_rgb(0x0D, 0x99, 0xFF),
        Color32::from_rgb(0x97, 0x47, 0xFF),
        Color32::from_rgb(0x14, 0xAE, 0x5C),
        Color32::from_rgb(0xF2, 0x48, 0x22),
        Color32::from_rgb(0xFF, 0xA6, 0x29),
        Color32::from_rgb(0xE8, 0x3E, 0x8C),
        Color32::from_rgb(0x00, 0xA3, 0xA3),
        Color32::from_rgb(0x66, 0x5C, 0xE0),
    ];
    let h = name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    PALETTE.get(h as usize % PALETTE.len()).copied().unwrap_or(ACCENT)
}

fn initials(name: &str) -> String {
    let mut out: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
    if out.is_empty() {
        out.push('?');
    }
    out.to_uppercase()
}

fn avatar(painter: &egui::Painter, center: Pos2, r: f32, name: &str) {
    painter.circle_filled(center, r, author_color(name));
    painter.text(center, Align2::CENTER_CENTER, initials(name), semibold(r * 0.8), Color32::WHITE);
}

/// Seconds since `t` (Unix seconds); `None` for unknown or future times.
fn age(t: u64) -> Option<u64> {
    let now = figcraft_engine::cmd::comment::now_secs();
    (t != 0 && now >= t).then(|| now - t)
}

/// "now", "5m", "3h", "2d", or a week count.
pub fn ago(created: u64) -> String {
    let Some(d) = age(created) else { return String::new() };
    match d {
        0..60 => "now".into(),
        60..3600 => format!("{}m", d / 60),
        3600..86_400 => format!("{}h", d / 3600),
        86_400..604_800 => format!("{}d", d / 86_400),
        _ => format!("{}w", d / 604_800),
    }
}

/// "Just now", "5 minutes ago", "Yesterday", "3 weeks ago"…
pub fn ago_long(t: u64) -> String {
    let Some(d) = age(t) else { return String::new() };
    let units = |n: u64, unit: &str| if n == 1 { format!("1 {unit} ago") } else { format!("{n} {unit}s ago") };
    match d {
        0..60 => "Just now".into(),
        60..3600 => units(d / 60, "minute"),
        3600..86_400 => units(d / 3600, "hour"),
        86_400..172_800 => "Yesterday".into(),
        172_800..604_800 => units(d / 86_400, "day"),
        604_800..2_592_000 => units(d / 604_800, "week"),
        2_592_000..31_536_000 => units(d / 2_592_000, "month"),
        _ => units(d / 31_536_000, "year"),
    }
}

/// Draw the pins (and the draft pin).
pub fn draw(app: &FigApp, painter: &egui::Painter) {
    if !visible(app) {
        return;
    }
    let hover = painter.ctx().input(|i| i.pointer.hover_pos());
    for (c, a) in pins(app) {
        let r = pin_rect(a);
        let open = app.ui.comments.open == Some(c.id);
        let hot = open || hover.is_some_and(|h| r.contains(h));
        // A speech bubble: round on three corners, pointed at the anchor.
        let cr = CornerRadius { nw: 14, ne: 14, se: 14, sw: 0 };
        painter.rect_filled(r.translate(vec2(0.0, 1.0)), cr, Color32::from_black_alpha(40));
        painter.rect_filled(r, cr, if hot { ACCENT } else { Color32::WHITE });
        painter.rect_stroke(r, cr, Stroke::new(1.0, Color32::from_black_alpha(30)), StrokeKind::Inside);
        let fill = if c.resolved { Color32::from_gray(150) } else { author_color(&c.author) };
        painter.circle_filled(r.center(), 11.0, fill);
        painter.text(r.center(), Align2::CENTER_CENTER, initials(&c.author), semibold(9.0), Color32::WHITE);
        if c.resolved {
            icons::paint(painter, Rect::from_center_size(r.right_top() + vec2(-3.0, 3.0), vec2(10.0, 10.0)), Icon::Check, ACCENT);
        }
    }
    if let Some((x, y, _)) = &app.ui.comments.draft {
        let r = pin_rect(app.to_screen(*x, *y));
        let cr = CornerRadius { nw: 14, ne: 14, se: 14, sw: 0 };
        painter.rect_filled(r, cr, ACCENT);
        painter.circle_filled(r.center(), 3.0, Color32::WHITE);
    }
}

/// Pointer handling: clicking a pin opens its thread. Returns true when a pin owns the pointer.
pub fn input(app: &mut FigApp, ctx: &egui::Context, pos: Pos2) -> bool {
    if !visible(app) {
        return false;
    }
    let pressed = ctx.input(|i| i.pointer.primary_clicked());
    for (c, a) in pins(app).iter().rev() {
        if pin_rect(*a).contains(pos) {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            if pressed {
                app.ui.comments.open = if app.ui.comments.open == Some(c.id) { None } else { Some(c.id) };
                app.ui.comments.draft = None;
                app.ui.comments.reply.clear();
            }
            return true;
        }
    }
    // A click elsewhere on the canvas closes an open thread (the Comment tool then starts a draft).
    if pressed && app.ui.comments.open.is_some() {
        app.ui.comments.open = None;
        return app.session.tool.tool != ToolId::Comment;
    }
    false
}

/// Start a new comment at a page point (the Comment tool's click).
pub fn start_draft(app: &mut FigApp, x: f64, y: f64) {
    app.ui.comments.open = None;
    app.ui.comments.draft = Some((x, y, String::new()));
}

fn composer(ui: &mut Ui, id: Id, text: &mut String, hint: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut submit = false;
    let frame = egui::Frame::NONE.fill(t.field).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 6));
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let te = egui::TextEdit::multiline(text)
                .id(id)
                .hint_text(hint)
                .font(regular(12.0))
                .frame(egui::Frame::NONE)
                .desired_rows(1)
                .desired_width(ui.available_width() - 30.0);
            let resp = ui.add(te);
            if !resp.has_focus() && ui.memory(|m| m.focused().is_none()) {
                resp.request_focus();
            }
            let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
            if enter {
                // Drop the newline the TextEdit just inserted.
                while text.ends_with('\n') {
                    text.pop();
                }
                submit = true;
            }
            let (br, bresp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            let ready = !text.trim().is_empty();
            ui.painter().circle_filled(br.center(), 12.0, if ready { ACCENT } else { t.hover });
            let c = br.center();
            let col = if ready { Color32::WHITE } else { t.icon_secondary };
            ui.painter().line_segment([c + vec2(0.0, 5.0), c + vec2(0.0, -5.0)], Stroke::new(1.6, col));
            ui.painter().line_segment([c + vec2(-4.0, -1.0), c + vec2(0.0, -5.0)], Stroke::new(1.6, col));
            ui.painter().line_segment([c + vec2(4.0, -1.0), c + vec2(0.0, -5.0)], Stroke::new(1.6, col));
            if bresp.on_hover_text("Post").clicked() {
                submit = true;
            }
        });
    });
    submit && !text.trim().is_empty()
}

fn message(ui: &mut Ui, author: &str, text: &str, created: u64) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
        avatar(ui.painter(), r.center(), 12.0, author);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(author).font(semibold(12.0)).color(t.text));
        ui.label(egui::RichText::new(ago(created)).font(regular(11.0)).color(t.text_tertiary));
    });
    ui.horizontal(|ui| {
        ui.add_space(36.0);
        ui.add(egui::Label::new(egui::RichText::new(text).font(regular(12.0)).color(t.text)).wrap());
    });
    ui.add_space(8.0);
}

/// The thread popover and the draft composer, floating next to their pins.
pub fn popovers(app: &mut FigApp, ctx: &egui::Context) {
    let t = Tokens::get(ctx);
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && (app.ui.comments.open.is_some() || app.ui.comments.draft.is_some()) {
        app.ui.comments.open = None;
        app.ui.comments.draft = None;
        return;
    }
    let canvas = app.canvas.rect;
    let place = |anchor: Pos2| {
        let mut p = anchor + vec2(PIN + 8.0, -PIN);
        if p.x + POPOVER_W > canvas.max.x - 8.0 {
            p.x = anchor.x - POPOVER_W - 8.0;
        }
        p.y = p.y.clamp(canvas.min.y + 8.0, (canvas.max.y - 200.0).max(canvas.min.y + 8.0));
        p
    };
    let frame = egui::Frame::NONE
        .fill(t.panel)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(12))
        .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) })
        .inner_margin(egui::Margin::same(12));
    if let Some((x, y, mut text)) = app.ui.comments.draft.clone() {
        let pos = place(app.to_screen(x, y));
        let mut post = false;
        egui::Area::new(Id::new("comment_draft")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(POPOVER_W - 24.0);
                post = composer(ui, Id::new("comment_draft_text"), &mut text, "Add a comment");
            });
        });
        if post {
            let r = app.run("comment.add", json!({"x": x, "y": y, "text": text.trim(), "anchor": true}));
            app.ui.comments.draft = None;
            app.ui.comments.open = r.and_then(|v| v.get("id").and_then(|i| i.as_u64()));
        } else if let Some(d) = &mut app.ui.comments.draft {
            d.2 = text;
        }
        return;
    }
    let Some(open) = app.ui.comments.open else { return };
    let Some((c, anchor)) = pins(app).into_iter().find(|(c, _)| c.id == open) else {
        app.ui.comments.open = None;
        return;
    };
    let pos = place(anchor);
    let mut reply = std::mem::take(&mut app.ui.comments.reply);
    let mut action: Option<(&str, serde_json::Value)> = None;
    egui::Area::new(Id::new("comment_thread")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_width(POPOVER_W - 24.0);
            // Header: resolve, more, close.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Comment").font(semibold(12.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Close, 24.0, false, "Close").clicked() {
                        action = Some(("close", json!({})));
                    }
                    let more = icons::button(ui, Icon::Dots, 24.0, false, "More");
                    egui::Popup::menu(&more).show(|ui| {
                        if crate::right::menu_item(ui, "Delete thread", "") {
                            action = Some(("comment.delete", json!({"id": c.id})));
                            ui.close();
                        }
                        if crate::right::menu_item(ui, if c.resolved { "Mark as unresolved" } else { "Mark as resolved" }, "") {
                            action = Some(("comment.resolve", json!({"id": c.id, "resolved": !c.resolved})));
                            ui.close();
                        }
                    });
                    let tip = if c.resolved { "Unresolve" } else { "Resolve" };
                    if icons::button(ui, Icon::Check, 24.0, c.resolved, tip).clicked() {
                        action = Some(("comment.resolve", json!({"id": c.id, "resolved": !c.resolved})));
                    }
                });
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                message(ui, &c.author, &c.text, c.created);
                for r in &c.replies {
                    message(ui, &r.author, &r.text, r.created);
                }
            });
            if composer(ui, Id::new(("comment_reply", c.id)), &mut reply, "Reply") {
                action = Some(("comment.reply", json!({"id": c.id, "text": reply.trim()})));
            }
        });
    });
    app.ui.comments.reply = reply;
    match action {
        Some(("close", _)) => app.ui.comments.open = None,
        Some((cmd, p)) => {
            if app.run(cmd, p).is_some() {
                if cmd == "comment.reply" {
                    app.ui.comments.reply.clear();
                }
                if cmd == "comment.delete" {
                    app.ui.comments.open = None;
                }
            }
        }
        None => {}
    }
}

/// The Comments panel (right side, Comment tool).
pub fn panel(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Ok(ds) = app.session.ds() else { return };
    let page = ds.page;
    let doc = ds.doc.clone();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("Comments").font(semibold(12.0)).color(t.text));
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let w = ui.available_width() - 16.0;
        if let Some(s) = crate::widgets::text_field(ui, Id::new("comment_search"), &app.ui.comments.search, w, "Search comments") {
            app.ui.comments.search = s;
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        if crate::widgets::checkbox(ui, app.ui.comments.show_resolved, "Show resolved comments") {
            app.ui.comments.show_resolved = !app.ui.comments.show_resolved;
        }
    });
    ui.add_space(8.0);
    let q = app.ui.comments.search.to_lowercase();
    let mut list: Vec<&Comment> = doc
        .comments
        .iter()
        .filter(|c| doc.pages.contains(&c.page))
        .filter(|c| !c.resolved || app.ui.comments.show_resolved)
        .filter(|c| {
            q.is_empty()
                || c.text.to_lowercase().contains(&q)
                || c.author.to_lowercase().contains(&q)
                || c.replies.iter().any(|r| r.text.to_lowercase().contains(&q))
        })
        .collect();
    // Current page first, newest first.
    list.sort_by_key(|c| (c.page != page, std::cmp::Reverse(c.created), std::cmp::Reverse(c.id)));
    if list.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No comments yet").font(semibold(12.0)).color(t.text_secondary));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Click anywhere on the canvas to leave one.").font(regular(11.0)).color(t.text_tertiary));
        });
        return;
    }
    let mut go: Option<(u64, figcraft_doc::NodeId)> = None;
    for c in list {
        let w = ui.available_width();
        let lines = c.text.lines().count().clamp(1, 3) as f32;
        let h = 52.0 + 16.0 * lines;
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
        let open = app.ui.comments.open == Some(c.id);
        if open {
            ui.painter().rect_filled(r, CornerRadius::ZERO, t.selected_row);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::ZERO, t.hover);
        }
        avatar(ui.painter(), pos2(r.min.x + 28.0, r.min.y + 22.0), 12.0, &c.author);
        ui.painter().text(pos2(r.min.x + 48.0, r.min.y + 22.0), Align2::LEFT_CENTER, &c.author, semibold(12.0), t.text);
        ui.painter().text(pos2(r.max.x - 16.0, r.min.y + 22.0), Align2::RIGHT_CENTER, ago(c.created), regular(11.0), t.text_tertiary);
        let body = ui.painter().layout(c.text.clone(), regular(12.0), t.text, w - 64.0);
        let clip = Rect::from_min_size(pos2(r.min.x + 48.0, r.min.y + 38.0), vec2(w - 64.0, 16.0 * lines));
        ui.painter().with_clip_rect(clip).galley(clip.min, body, t.text);
        let mut meta = Vec::new();
        if !c.replies.is_empty() {
            meta.push(format!("{} {}", c.replies.len(), if c.replies.len() == 1 { "reply" } else { "replies" }));
        }
        if c.page != page {
            meta.push(doc.node(c.page).map(|n| n.name.clone()).unwrap_or_default());
        }
        if c.resolved {
            meta.push("Resolved".into());
        }
        if !meta.is_empty() {
            ui.painter().text(pos2(r.min.x + 48.0, r.max.y - 12.0), Align2::LEFT_CENTER, meta.join(" · "), regular(11.0), t.text_secondary);
        }
        ui.painter().line_segment([pos2(r.min.x + 16.0, r.max.y), pos2(r.max.x, r.max.y)], Stroke::new(1.0, t.border));
        if resp.clicked() {
            go = Some((c.id, c.page));
        }
    }
    if let Some((id, pg)) = go {
        if pg != page {
            app.run("page.select", json!({"id": pg}));
        }
        // Centre the pin in view.
        if let Ok(ds) = app.session.ds_mut()
            && let Some(c) = ds.doc.comment(id).cloned()
        {
            let (x, y) = c.position(&ds.doc);
            ds.view.cx = x;
            ds.view.cy = y;
            ds.revision += 1;
        }
        app.ui.comments.open = Some(id);
        app.ui.comments.draft = None;
    }
}
