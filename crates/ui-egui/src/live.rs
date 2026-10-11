//! Live sessions in the window: other people's cursors (smoothly following their pointer, with a
//! name tag) and selections on the canvas (their edits arrive through the engine's sync), their avatars at the top of the right panel (click one
//! to follow their view), the Share popover (start a session, the invite link, stop; the server,
//! file and access of a session you joined; a published file's links), the Join dialog and the
//! Publish to server dialog. Everything acts through the `collab.*` commands; the state is the engine's
//! (`Session::live`). The network wakes the UI when something arrives, and cursors only ask for
//! frames while they glide, so an idle session draws nothing.

use std::collections::BTreeMap;

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use figcraft_engine::live::{Access, PEOPLE, Role, Status, initials, parse_color, presence_page};
use serde_json::json;

use crate::FigApp;
use crate::a11y::{self, Tip};
use crate::icons::{self, Icon};
use crate::right::{menu_item, menu_item_with, menu_popup};
use crate::theme::{ACCENT, Tokens, regular, semibold};
use crate::widgets;

/// In the browser: what joining needs (a page can't host, and https pages can't reach `ws://`).
const BROWSER_NOTE: &str = "In the browser you can join sessions the desktop app hosts and files on a FigCraft server. A page opened over https reaches other computers only through wss:// links (a server behind TLS).";

/// How undo works in a session (per person).
const UNDO_NOTE: &str = "Undo takes back only your own changes; what others changed since stays.";

/// What a view link means for you.
const VIEW_NOTE: &str = "View only: you see every change as it happens but can't edit. Leave to keep a copy you can edit.";

/// What a published file's links are.
const PUBLISHED_NOTE: &str = "Anyone with the edit link can change the file; the view link only lets people look. The links carry their secrets: share them like passwords. `figcraft-server links` makes more and revokes them.";

/// How fast a remote cursor closes in on where it was last reported (per second).
const GLIDE: f64 = 20.0;
/// Most of one person's selection outlined.
const MAX_OUTLINES: usize = 500;

/// UI-only live session state.
#[derive(Default)]
pub struct LiveUi {
    /// Where each remote cursor is drawn (page coordinates), gliding to where it was reported.
    shown: BTreeMap<u32, (f64, f64)>,
    /// The UI clock at the last cursor frame.
    last: f64,
    /// The pointer position last handed to the session (page coordinates).
    seen: Option<(f64, f64)>,
    /// The Join dialog's link field (open when `Some`).
    pub join: Option<String>,
    /// Put the keyboard in the Join dialog's field on its next frame.
    focus_join: bool,
    /// Start sessions on the local network (else this machine only).
    pub lan: bool,
    /// The Publish to server dialog's fields (open when `Some`).
    pub publish: Option<PublishForm>,
    /// Put the keyboard in the Publish dialog's first field on its next frame.
    focus_publish: bool,
    /// The last server address published to (kept for this run only: it carries the server key).
    last_server: String,
}

/// What the Publish to server dialog asks for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PublishForm {
    /// The server's address with its key (`ws://host:47900/?key=…`).
    pub server: String,
    /// The file's name on the server.
    pub name: String,
}

impl LiveUi {
    /// The remote cursors drawn last frame (`ui.inspect`).
    pub fn shown(&self) -> Vec<u32> {
        self.shown.keys().copied().collect()
    }
}

/// A person's colour (`#RRGGBB` from the session), or the accent for anything else.
pub fn color(s: &str) -> Color32 {
    parse_color(s).map_or(ACCENT, |(r, g, b)| Color32::from_rgb(r, g, b))
}

/// The palette colour `i` (comment authors use it too).
pub fn people(i: usize) -> Color32 {
    PEOPLE.get(i % PEOPLE.len()).map_or(ACCENT, |c| color(c))
}

/// Text that reads on `fill`.
pub fn text_on(fill: Color32) -> Color32 {
    let l = 0.299 * f32::from(fill.r()) + 0.587 * f32::from(fill.g()) + 0.114 * f32::from(fill.b());
    if l > 170.0 { Color32::from_rgb(0x1E, 0x1E, 0x1E) } else { Color32::WHITE }
}

/// An avatar: the initials in a circle of the person's colour.
pub fn avatar(painter: &egui::Painter, center: Pos2, r: f32, name: &str, fill: Color32) {
    painter.circle_filled(center, r, fill);
    painter.text(center, Align2::CENTER_CENTER, initials(name), semibold(r * 0.8), text_on(fill));
}

/// FigCraft's remote pointer: a notched dart pointing up-left from `tip`, filled with the
/// person's colour and edged in white so it shows on any canvas.
pub fn pointer(painter: &egui::Painter, tip: Pos2, fill: Color32) {
    let at = |x: f32, y: f32| tip + vec2(x, y);
    let (left, notch, right) = (at(3.5, 15.5), at(7.5, 8.5), at(15.5, 4.0));
    painter.add(Shape::convex_polygon(vec![tip, left, notch], fill, Stroke::NONE));
    painter.add(Shape::convex_polygon(vec![tip, notch, right], fill, Stroke::NONE));
    painter.add(Shape::closed_line(vec![tip, left, notch, right], Stroke::new(1.5, Color32::WHITE)));
}

/// A name tag in a person's colour, placed by `align` at `anchor` (next to their pointer, above
/// their text caret).
pub(crate) fn name_tag(painter: &egui::Painter, anchor: Pos2, align: Align2, name: &str, fill: Color32) {
    let galley = painter.layout_no_wrap(name.to_string(), semibold(11.0), text_on(fill));
    let r = align.anchor_size(anchor, galley.size() + vec2(12.0, 6.0));
    painter.rect_filled(r, CornerRadius::same(4), fill);
    painter.galley(r.min + vec2(6.0, 3.0), galley, text_on(fill));
}

/// Hand the pointer's page position to the session when it moved (or the view under it did).
pub fn track_pointer(app: &mut FigApp, ctx: &egui::Context) {
    if app.session.live.status != Status::Live {
        app.ui.live.seen = None;
        return;
    }
    let at = ctx.input(|i| i.pointer.hover_pos()).filter(|p| app.canvas.rect.contains(*p)).map(|p| app.to_page(p)).map(|p| (p.x, p.y));
    if at != app.ui.live.seen {
        app.ui.live.seen = at;
        app.session.live.cursor = at;
    }
}

/// Others' selections and cursors on the canvas, and the frame around it while following.
pub fn draw(app: &mut FigApp, painter: &egui::Painter, ctx: &egui::Context) {
    let live = &app.session.live;
    if live.peers.is_empty() {
        app.ui.live.shown.clear();
        return;
    }
    let Ok(ds) = app.session.ds() else { return };
    let (doc, page, shared) = (ds.doc.clone(), ds.page, app.session.live_shared());
    let mut cursors = Vec::new();
    for (id, p) in &live.presence {
        let Some(peer) = live.peers.get(id) else { continue };
        let fill = color(&peer.color);
        // Selections only mean something on the same page of the same (synced) document.
        if shared
            && presence_page(p) == Some(page)
            && let Some(sel) = &p.selection
        {
            for n in sel.iter().filter(|n| doc.page_of(**n) == Some(page)).take(MAX_OUTLINES) {
                if let Some(q) = crate::canvas::node_quad(app, &doc, *n) {
                    painter.add(Shape::closed_line(q.to_vec(), Stroke::new(1.5, fill)));
                }
            }
        }
        // Cursors: on your page of the session's document; before its welcome arrives (or with
        // another document in front) they show in page coordinates on whatever page you look at.
        if let Some((pg, x, y)) = p.cursor
            && (pg == page || !shared)
        {
            cursors.push((*id, peer.name.clone(), fill, (x, y)));
        }
    }
    if let Some(peer) = live.follow.and_then(|f| live.peers.get(&f)) {
        follow_frame(painter, app.canvas.rect, &peer.name, color(&peer.color));
    }
    let now = ctx.input(|i| i.time);
    let dt = (now - app.ui.live.last).clamp(0.0, 0.1);
    app.ui.live.last = now;
    let k = if app.reduce_motion() { 1.0 } else { 1.0 - (-dt * GLIDE).exp() };
    let zoom = ds.view.zoom.max(1e-9);
    let mut gliding = false;
    let mut shown = BTreeMap::new();
    for (id, name, fill, (x, y)) in cursors {
        let (mut sx, mut sy) = app.ui.live.shown.get(&id).copied().unwrap_or((x, y));
        sx += (x - sx) * k;
        sy += (y - sy) * k;
        // Within half a pixel: there.
        if ((x - sx) * zoom).hypot((y - sy) * zoom) < 0.5 {
            (sx, sy) = (x, y);
        } else {
            gliding = true;
        }
        shown.insert(id, (sx, sy));
        let tip = app.to_screen(sx, sy);
        pointer(painter, tip, fill);
        name_tag(painter, tip + vec2(12.0, 16.0), Align2::LEFT_TOP, &name, fill);
    }
    app.ui.live.shown = shown;
    if gliding {
        ctx.request_repaint();
    }
}

/// A frame around the canvas in the followed person's colour, with their name at the top.
fn follow_frame(painter: &egui::Painter, canvas: Rect, name: &str, fill: Color32) {
    painter.rect_stroke(canvas.shrink(1.0), CornerRadius::ZERO, Stroke::new(2.0, fill), egui::StrokeKind::Inside);
    let text = format!("Following {name} · {} to stop", crate::toolbar::pretty_shortcut("Escape"));
    let galley = painter.layout_no_wrap(text, semibold(11.0), text_on(fill));
    let r = Rect::from_center_size(pos2(canvas.center().x, canvas.min.y + 2.0 + (galley.size().y + 6.0) / 2.0), galley.size() + vec2(16.0, 6.0));
    painter.rect_filled(r, CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }, fill);
    painter.galley(r.min + vec2(8.0, 3.0), galley, text_on(fill));
}

/// You and the others at the top of the right panel, within `area`: click someone to follow
/// them (again to stop); the chevron lists everyone.
pub fn people_bar(app: &mut FigApp, ui: &mut Ui, area: Rect) {
    let t = Tokens::get(ui.ctx());
    const R: f32 = 12.0;
    const STEP: f32 = 20.0;
    let cy = area.center().y;
    let live = &app.session.live;
    let me_name = match live.info.as_ref() {
        Some(i) if !i.name.is_empty() => i.name.clone(),
        _ => app.session.prefs.user_name.clone(),
    };
    let me_fill = live.info.as_ref().map_or(t.success, |i| color(&i.color));
    let peers: Vec<(u32, String, Color32)> = live.peers.values().map(|p| (p.client_id, p.name.clone(), color(&p.color))).collect();
    let follow = live.follow;
    // Room for the avatars after yours and the chevron.
    let fit = (((area.width() - 2.0 * R - 18.0) / STEP).floor() as usize).max(1);
    let (show, more) = if peers.len() > fit { (fit - 1, peers.len() - (fit - 1)) } else { (peers.len(), 0) };
    let mut x = area.min.x + R + 2.0;
    let ring = |ui: &Ui, c: Pos2| ui.painter().circle_stroke(c, R + 1.0, Stroke::new(2.0, t.panel));
    avatar(ui.painter(), pos2(x, cy), R, &me_name, me_fill);
    let mut clicked = None;
    for (id, name, fill) in peers.iter().take(show) {
        x += STEP;
        let c = pos2(x, cy);
        let following = follow == Some(*id);
        let tip = if following { format!("Stop following {name}") } else { format!("Follow {name}") };
        let resp = ui.interact(Rect::from_center_size(c, vec2(2.0 * R, 2.0 * R)), ui.id().with(("live_avatar", id)), Sense::click()).tip(tip);
        ring(ui, c);
        avatar(ui.painter(), c, R, name, *fill);
        if following || resp.hovered() {
            ui.painter().circle_stroke(c, R + 2.5, Stroke::new(2.0, *fill));
        }
        if resp.clicked() {
            clicked = Some(*id);
        }
    }
    if more > 0 {
        x += STEP;
        let c = pos2(x, cy);
        ring(ui, c);
        ui.painter().circle_filled(c, R, t.pressed);
        ui.painter().text(c, Align2::CENTER_CENTER, format!("+{more}"), semibold(10.0), t.text);
    }
    let chev = Rect::from_center_size(pos2(x + R + 10.0, cy), vec2(16.0, 24.0));
    let cresp = ui.interact(chev, ui.id().with("live_people"), Sense::click());
    a11y::menu_button(&cresp, "People in this file", a11y::menu_open(&cresp));
    if cresp.hovered() {
        ui.painter().rect_filled(chev, CornerRadius::same(4), t.hover);
    }
    icons::paint(ui.painter(), Rect::from_center_size(chev.center(), vec2(10.0, 10.0)), Icon::Chevron, t.icon_secondary);
    menu_popup(&cresp).width(220.0).show(|ui| {
        let row = |ui: &mut Ui, label: &str, fill: Color32, check: bool| {
            menu_item_with(ui, label, if check { "Following" } else { "" }, 22.0, |p, r, _| {
                avatar(p, pos2(r.max.x - 15.0, r.center().y), 8.0, label, fill)
            })
        };
        row(ui, &format!("{me_name} (you)"), me_fill, false);
        for (id, name, fill) in &peers {
            if row(ui, name, *fill, follow == Some(*id)) {
                clicked = Some(*id);
                ui.close();
            }
        }
        if peers.is_empty() {
            widgets::label(ui, if app.session.live.status.active() { "Nobody else is here yet" } else { "Not in a live session" }, t.text_secondary);
        }
    });
    if let Some(id) = clicked {
        let params = if follow == Some(id) { json!({}) } else { json!({"client": id}) };
        app.run("collab.follow", params);
    }
}

/// The Share button's popover: the live session (start, invite link, stop or leave) and export.
pub fn share_popover(app: &mut FigApp, anchor: &egui::Response) {
    menu_popup(anchor).width(300.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        let t = Tokens::get(ui.ctx());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.set_max_width(300.0);
        // Menus lay out justified and don't wrap: the session's notes wrap, left-aligned, within
        // the popover's width.
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Live session").font(semibold(12.0)));
            let status = app.session.live.status.clone();
            let role = app.session.live.role();
            let others = app.session.live.peers.len();
            let published = app.session.live.published.is_some();
            // Connecting with a tab already chosen and nobody's welcome yet: the upload.
            let publishing = status == Status::Connecting && role.is_none() && app.session.live.doc_uid.is_some();
            let (line, col) = match (&status, role) {
                (Status::Live, Some(Role::Host)) => (format!("Live · {}", people_count(others)), t.success),
                (Status::Live, _) if published => (format!("Published · {}", people_count(others)), t.success),
                (Status::Connecting, _) if publishing => ("Publishing…".to_string(), t.text_secondary),
                (Status::Live, _) => {
                    let file = app.session.live.info.as_ref().map(|i| i.file_name.clone()).unwrap_or_default();
                    (if file.is_empty() { "Joined".to_string() } else { format!("Joined {file}") }, t.success)
                }
                (Status::Failed { .. }, _) => (status.label(), t.danger),
                _ => (status.label(), t.text_secondary),
            };
            widgets::label(ui, &line, col);
            match (&status, role) {
                (Status::Off | Status::Failed { .. }, _) => start_ui(app, ui, &t),
                (Status::Live, Some(Role::Host)) => host_ui(app, ui),
                _ => guest_ui(app, ui, &t),
            }
        });
        ui.separator();
        if menu_item(ui, "Export…", "") {
            app.ui.export_preview = true;
            ui.close();
        }
    });
}

fn people_count(others: usize) -> String {
    match others {
        0 => "nobody else here yet".into(),
        1 => "1 person joined".into(),
        n => format!("{n} people joined"),
    }
}

/// Not in a session: your name, where to share, start; or join one.
fn start_ui(app: &mut FigApp, ui: &mut Ui, t: &Tokens) {
    ui.horizontal(|ui| {
        widgets::label(ui, "Your name", t.text);
        let name = app.session.prefs.user_name.clone();
        if let Some(n) = widgets::text_field(ui, egui::Id::new("live_name"), &name, 160.0, "Your name") {
            app.run("pref.userName", json!({"name": n}));
        }
    });
    if !app.session.live.can_host() {
        widgets::label(ui, "Hosting a session needs the desktop app; here you can join one.", t.text_secondary);
        if widgets::button(ui, "Join a session…", true, true).clicked() {
            open_join(app, "");
            ui.close();
        }
        return;
    }
    if widgets::checkbox(ui, app.ui.live.lan, "Share on the local network") {
        app.ui.live.lan = !app.ui.live.lan;
    }
    let reach = if app.ui.live.lan {
        "Anyone on your network who has the invite link can join."
    } else {
        "Only this computer can join (turn on network sharing to invite others)."
    };
    widgets::label(ui, reach, t.text_secondary);
    ui.horizontal(|ui| {
        if widgets::button(ui, "Start live session", true, true).clicked() {
            let bind = if app.ui.live.lan { "lan" } else { "local" };
            app.run("collab.host", json!({"bind": bind}));
        }
        if widgets::button(ui, "Join a session…", false, true).clicked() {
            open_join(app, "");
            ui.close();
        }
    });
    ui.separator();
    widgets::label(ui, "Or keep the file on a FigCraft server and share links to edit or to view it.", t.text_secondary);
    if widgets::button(ui, "Publish to server…", false, true).clicked() {
        open_publish(app);
        ui.close();
    }
}

/// Hosting: the invite link and URL to copy, and stop.
fn host_ui(app: &mut FigApp, ui: &mut Ui) {
    let Some(info) = app.session.live.info.clone() else { return };
    link_rows(app, ui, &[("Invite link", &info.link, "Invite link copied"), ("WebSocket URL", &info.url, "WebSocket URL copied")]);
    let t = Tokens::get(ui.ctx());
    widgets::label(ui, "The link carries the session's secret: share it only with people you invite.", t.text_secondary);
    widgets::label(ui, UNDO_NOTE, t.text_secondary);
    if widgets::button(ui, "Stop session", false, true).clicked() {
        app.run("collab.stop", json!({}));
    }
}

/// Joined, or publishing: where the file is and what you may do there, a published file's links,
/// and leave (the file stays open as your copy).
fn guest_ui(app: &mut FigApp, ui: &mut Ui, t: &Tokens) {
    let live = &app.session.live;
    let view = live.info.as_ref().is_some_and(|i| i.access == Access::View);
    if let Some(i) = live.info.as_ref() {
        widgets::label(ui, &format!("{} · {}", i.address, if view { "View only" } else { "Can edit" }), t.text_secondary);
        if !i.pages.is_empty() {
            let pages: Vec<&str> = i.pages.iter().map(|p| p.name.as_str()).take(8).collect();
            widgets::label(ui, &format!("Pages: {}", pages.join(", ")), t.text_secondary);
        }
    }
    if let Some(p) = live.published.clone() {
        link_rows(app, ui, &[("Edit link", &p.edit, "Edit link copied"), ("View link", &p.view, "View link copied")]);
        widgets::label(ui, PUBLISHED_NOTE, t.text_secondary);
    }
    widgets::label(ui, if view { VIEW_NOTE } else { UNDO_NOTE }, t.text_secondary);
    if widgets::button(ui, "Leave session", false, true).on_hover_text("Your copy of the file stays open").clicked() {
        app.run("collab.leave", json!({}));
    }
}

/// Links to copy: a label, the link (elided, monospace) and a Copy button each.
fn link_rows(app: &mut FigApp, ui: &mut Ui, rows: &[(&str, &String, &str)]) {
    for (label, value, what) in rows {
        ui.horizontal(|ui| {
            let t = Tokens::get(ui.ctx());
            let w = ui.available_width() - 64.0;
            ui.vertical(|ui| {
                ui.set_width(w);
                widgets::label(ui, label, t.text_secondary);
                ui.label(egui::RichText::new(widgets::elide(ui, value, w)).font(regular(10.5)).monospace());
            });
            if widgets::button(ui, "Copy", false, true).on_hover_text(format!("Copy the {}", label.to_lowercase())).clicked() {
                app.services.clipboard_set_text(value);
                app.toast(*what);
            }
        });
    }
}

/// Open the Join dialog, with `link` in its field (focused).
pub fn open_join(app: &mut FigApp, link: &str) {
    app.ui.live.join = Some(link.to_string());
    app.ui.live.focus_join = true;
}

/// An invite link the app was opened with (`figcraft figcraft://join?…`): join at once; a link
/// that can't be used shows in the Join dialog with the reason.
pub fn open_invite(app: &mut FigApp, link: &str) {
    if app.run("collab.join", json!({"link": link.trim()})).is_none() {
        open_join(app, link.trim());
    }
}

/// Escape closes the Join or Publish dialog (before the canvas's shortcuts see it: it doesn't
/// deselect).
pub fn dialog_keys(app: &mut FigApp, ctx: &egui::Context) {
    let live = &mut app.ui.live;
    if (live.join.is_some() || live.publish.is_some()) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        live.join = None;
        live.publish = None;
    }
}

/// File ▸ Join live session: paste an invite link.
pub fn join_dialog(app: &mut FigApp, ctx: &egui::Context) {
    // The editor takes Escape before its shortcuts; the home screen has none.
    dialog_keys(app, ctx);
    let Some(mut link) = app.ui.live.join.take() else { return };
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut cancel = false;
    let mut go = false;
    let focus = std::mem::take(&mut app.ui.live.focus_join);
    crate::dialogs::window("Join live session", ctx).open(&mut open).show(ctx, |ui| {
        ui.set_width(380.0);
        widgets::label(ui, "Paste the invite link the host shared with you.", t.text);
        let resp = ui
            .add(egui::TextEdit::singleline(&mut link).hint_text("figcraft://join?host=…").desired_width(380.0).id(egui::Id::new("live_join_link")));
        a11y::text(&resp, "Invite link", "");
        if focus {
            resp.request_focus();
        }
        go = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        widgets::label(ui, "You'll get the host's file as a new tab and edit it together.", t.text_secondary);
        if !app.session.live.can_host() {
            widgets::label(ui, BROWSER_NOTE, t.text_secondary);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            go |= widgets::button(ui, "Join", true, !link.trim().is_empty()).clicked();
            cancel = widgets::button(ui, "Cancel", false, true).clicked();
        });
    });
    if go && !link.trim().is_empty() && app.run("collab.join", json!({"link": link.trim()})).is_some() {
        return;
    }
    if open && !cancel {
        app.ui.live.join = Some(link);
    }
}

/// Open the Publish to server dialog for the open file (the last address used this run in it).
pub fn open_publish(app: &mut FigApp) {
    if !app.session.live.can_host() {
        app.toast("Publishing to a server needs the desktop app; in the browser you can join a server's files");
        return;
    }
    let name = app.session.doc().map(|d| d.name.clone()).unwrap_or_default();
    app.ui.live.publish = Some(PublishForm { server: app.ui.live.last_server.clone(), name });
    app.ui.live.focus_publish = true;
}

/// Share ▸ Publish to server…: the server's address (with its key) and the file's name.
pub fn publish_dialog(app: &mut FigApp, ctx: &egui::Context) {
    let Some(mut form) = app.ui.live.publish.take() else { return };
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut cancel = false;
    let mut go = false;
    let focus = std::mem::take(&mut app.ui.live.focus_publish);
    crate::dialogs::window("Publish to server", ctx).open(&mut open).show(ctx, |ui| {
        ui.set_width(380.0);
        widgets::label(
            ui,
            "Uploads this file to a FigCraft server and keeps this tab in sync with it. You get a link to edit and a link to view.",
            t.text,
        );
        widgets::label(ui, "Server address", t.text_secondary);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut form.server)
                .hint_text("ws://server:47900/?key=…")
                .desired_width(380.0)
                .id(egui::Id::new("live_publish_server")),
        );
        // The address carries the server key: it isn't read out.
        a11y::text(&resp, "Server address", "");
        if focus {
            resp.request_focus();
        }
        let enter = |ui: &Ui| ui.input(|i| i.key_pressed(egui::Key::Enter));
        go = resp.lost_focus() && enter(ui);
        widgets::label(ui, "On the server's machine, `figcraft-server key` prints it.", t.text_secondary);
        widgets::label(ui, "File name", t.text_secondary);
        let nresp = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(380.0).id(egui::Id::new("live_publish_name")));
        a11y::text(&nresp, "File name", &form.name);
        go |= nresp.lost_focus() && enter(ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            go |= widgets::button(ui, "Publish", true, !form.server.trim().is_empty()).clicked();
            cancel = widgets::button(ui, "Cancel", false, true).clicked();
        });
    });
    if go && !form.server.trim().is_empty() && app.run("collab.publish", json!({"server": form.server.trim(), "name": form.name.trim()})).is_some() {
        app.ui.live.last_server = form.server.trim().to_string();
        return;
    }
    if open && !cancel {
        app.ui.live.publish = Some(form);
    }
}

/// Live session state for `ui.inspect`.
pub fn inspect(app: &FigApp) -> serde_json::Value {
    let l = &app.session.live;
    json!({
        "status": l.status.id(),
        "role": l.role(),
        "peers": l.peers.len(),
        "follow": l.follow,
        "cursorsShown": app.ui.live.shown(),
        "joinDialog": app.ui.live.join.is_some(),
        "publishDialog": app.ui.live.publish.is_some(),
        "readOnly": app.session.active_read_only(),
    })
}

/// Peer ids drawn on the canvas, by what they point at (tests).
#[cfg(test)]
pub(crate) fn shown_at(app: &FigApp) -> Vec<(u32, (f64, f64))> {
    app.ui.live.shown.iter().map(|(k, v)| (*k, *v)).collect()
}
