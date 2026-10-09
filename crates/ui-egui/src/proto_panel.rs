//! The Prototype tab: interactions (with an editor popover), flow starting points, scroll
//! behaviour, overlay settings and prototype settings. Every edit runs a `proto.*` command or
//! `node.set`.

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_color::Paint;
use figcraft_doc::proto::{Action, Direction, Easing, Interaction, Overflow, OverlayPosition, ScrollPosition, Transition, Trigger};
use figcraft_doc::*;
use serde_json::{Value, json};

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::right::{menu_item, row};
use crate::theme::{ACCENT, Tokens, regular, semibold};
use crate::widgets::{self, FIELD_H, FieldEdit, Lead, PAD};

/// The interaction whose editor popover is open: (layer, index).
fn editor_id() -> Id {
    Id::new("proto_editor")
}

pub fn open_editor(ctx: &egui::Context, target: Option<(NodeId, usize)>) {
    ctx.data_mut(|d| match target {
        Some(t) => {
            d.insert_temp(editor_id(), t);
        }
        None => {
            d.remove::<(NodeId, usize)>(editor_id());
        }
    });
}

pub fn editor(ctx: &egui::Context) -> Option<(NodeId, usize)> {
    ctx.data(|d| d.get_temp(editor_id()))
}

const TRIGGERS: [&str; 10] =
    ["On click", "On drag", "While hovering", "While pressing", "Key/Gamepad", "Mouse enter", "Mouse leave", "Mouse down", "Mouse up", "After delay"];

fn trigger_kind(t: &Trigger) -> &'static str {
    match t {
        Trigger::OnClick => "On click",
        Trigger::OnDrag => "On drag",
        Trigger::OnHover => "While hovering",
        Trigger::OnPress => "While pressing",
        Trigger::KeyPress { .. } => "Key/Gamepad",
        Trigger::MouseEnter => "Mouse enter",
        Trigger::MouseLeave => "Mouse leave",
        Trigger::MouseDown => "Mouse down",
        Trigger::MouseUp => "Mouse up",
        Trigger::AfterDelay { .. } => "After delay",
    }
}

fn trigger_of(kind: &str, cur: &Trigger) -> Trigger {
    match kind {
        "On drag" => Trigger::OnDrag,
        "While hovering" => Trigger::OnHover,
        "While pressing" => Trigger::OnPress,
        "Key/Gamepad" => match cur {
            Trigger::KeyPress { key } => Trigger::KeyPress { key: key.clone() },
            _ => Trigger::KeyPress { key: String::new() },
        },
        "Mouse enter" => Trigger::MouseEnter,
        "Mouse leave" => Trigger::MouseLeave,
        "Mouse down" => Trigger::MouseDown,
        "Mouse up" => Trigger::MouseUp,
        "After delay" => match cur {
            Trigger::AfterDelay { ms } => Trigger::AfterDelay { ms: *ms },
            _ => Trigger::AfterDelay { ms: 800 },
        },
        _ => Trigger::OnClick,
    }
}

/// The trigger as the list shows it ("Key/Gamepad Space", "After delay 800ms").
fn trigger_summary(t: &Trigger) -> String {
    match t {
        Trigger::KeyPress { key } if key.is_empty() => "Key/Gamepad".into(),
        other => other.label(),
    }
}

const ACTIONS: [&str; 9] =
    ["Navigate to", "Change to", "Back", "Scroll to", "Open link", "Open overlay", "Swap overlay", "Close overlay", "Set variable"];

const TRANSITIONS: [&str; 8] = ["Instant", "Dissolve", "Smart animate", "Move in", "Move out", "Push", "Slide in", "Slide out"];

fn transition_of(label: &str, cur: Transition) -> Transition {
    let d = cur.direction().unwrap_or_default();
    match label {
        "Dissolve" => Transition::Dissolve,
        "Smart animate" => Transition::SmartAnimate,
        "Move in" => Transition::MoveIn { direction: d },
        "Move out" => Transition::MoveOut { direction: d },
        "Push" => Transition::Push { direction: d },
        "Slide in" => Transition::SlideIn { direction: d },
        "Slide out" => Transition::SlideOut { direction: d },
        _ => Transition::Instant,
    }
}

/// Destinations a "Change to" can pick: the variants next to the layer's component.
fn variants_for(doc: &Document, id: NodeId) -> Vec<NodeId> {
    let comp = figcraft_engine::components::owning_component(doc, id).or_else(|| match doc.node(id).map(|n| &n.kind) {
        Some(NodeKind::Instance { component, .. }) => Some(*component),
        _ => None,
    });
    let set = comp
        .and_then(|c| doc.node(c))
        .and_then(|c| c.parent)
        .filter(|p| doc.node(*p).is_some_and(|n| matches!(n.kind, NodeKind::ComponentSet { .. })));
    match set.and_then(|s| doc.node(s)) {
        Some(s) => s.children.clone(),
        None => comp.into_iter().collect(),
    }
}

/// Destinations for navigate/overlay actions: the page's top-level frames.
fn frames_for(doc: &Document, page: NodeId) -> Vec<NodeId> {
    doc.top_frames(page)
}

fn name_of(doc: &Document, id: NodeId) -> String {
    doc.node(id).map(|n| n.name.clone()).unwrap_or_else(|| "Missing layer".into())
}

/// One-line action summary ("Navigate to Frame 2").
fn action_summary(doc: &Document, a: &Action) -> String {
    match a {
        Action::OpenUrl { url } if !url.is_empty() => format!("Open link {url}"),
        Action::SetVariable { variable, .. } => {
            let name = doc.variables.get(variable).map(|v| v.name.clone()).unwrap_or_else(|| variable.clone());
            format!("Set variable {name}")
        }
        other => match other.dest() {
            Some(d) => format!("{} {}", other.label(), name_of(doc, d)),
            None => other.label().to_string(),
        },
    }
}

/// A default action of the kind labelled `label` (None when it needs a destination and there is
/// none).
fn action_of(label: &str, doc: &Document, page: NodeId, node: NodeId, cur: Option<&Action>) -> Option<Action> {
    let own = doc.top_level_frame(node);
    let frames = frames_for(doc, page);
    let cur_dest = cur.and_then(|a| a.dest()).filter(|d| frames.contains(d));
    let frame_dest = cur_dest.or_else(|| frames.iter().copied().find(|f| Some(*f) != own)).or(frames.first().copied());
    Some(match label {
        "Navigate to" => Action::Navigate { dest: frame_dest? },
        "Change to" => {
            let vs = variants_for(doc, node);
            let mine = figcraft_engine::components::owning_component(doc, node);
            Action::ChangeTo { dest: vs.iter().copied().find(|v| Some(*v) != mine).or(vs.first().copied())? }
        }
        "Back" => Action::Back,
        "Scroll to" => {
            let root = own?;
            let dest = doc.subtree(root).into_iter().skip(1).find(|i| *i != node)?;
            Action::ScrollTo { dest }
        }
        "Open link" => Action::OpenUrl { url: String::new() },
        "Open overlay" => Action::OpenOverlay { dest: frame_dest?, position: OverlayPosition::Center, close_on_outside: true, background: None },
        "Swap overlay" => Action::SwapOverlay { dest: frame_dest? },
        "Close overlay" => Action::CloseOverlay,
        "Set variable" => {
            let v = doc.variables.variables.first()?;
            let value = match v.values.values().next() {
                Some(VariableValue::Color(c)) => json!(c.to_css_hex()),
                Some(VariableValue::Number(n)) => json!(n),
                Some(VariableValue::Boolean(b)) => json!(b),
                Some(VariableValue::String(s)) => json!(s),
                _ => Value::Null,
            };
            Action::SetVariable { variable: v.id.clone(), value }
        }
        _ => return None,
    })
}

fn patch(app: &mut FigApp, node: NodeId, index: usize, fields: Value) {
    let mut p = json!({"ids": [node], "index": index});
    if let (Some(o), Value::Object(f)) = (p.as_object_mut(), fields) {
        o.extend(f);
    }
    app.run("proto.interaction.set", p);
}

pub fn prototype(app: &mut FigApp, ui: &mut Ui) {
    let Ok(ds) = app.session.ds() else { return };
    let doc = ds.doc.clone();
    let sel = ds.selection.clone();
    let page = ds.page;
    if sel.is_empty() {
        open_editor(ui.ctx(), None);
        flows_overview(app, ui, &doc, page);
        widgets::separator(ui);
        settings(app, ui, &doc, page);
        return;
    }
    let first = sel.first().copied().unwrap_or(NodeId(0));
    let Some(node) = doc.node(first).cloned() else { return };
    if let Some((n, _)) = editor(ui.ctx())
        && n != first
    {
        open_editor(ui.ctx(), None);
    }
    // Flow starting point (top-level frames).
    if doc.top_level_frame(first) == Some(first) {
        flow_section(app, ui, &doc, page, first);
        widgets::separator(ui);
    }
    interactions(app, ui, &doc, page, &node);
    widgets::separator(ui);
    scroll_section(app, ui, &doc, &sel, &node);
}

fn flows_of(doc: &Document, page: NodeId) -> Vec<proto::Flow> {
    match doc.node(page).map(|n| &n.kind) {
        Some(NodeKind::Page { flows, .. }) => flows.clone(),
        _ => vec![],
    }
}

/// One flow row: play, editable name, remove.
fn flow_row(app: &mut FigApp, ui: &mut Ui, f: &proto::Flow) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    let play = Rect::from_center_size(pos2(r.min.x + PAD + 8.0, r.center().y), vec2(24.0, 24.0));
    let presp = ui.interact(play, ui.id().with(("flow_play", f.start)), Sense::click()).on_hover_text("Present this flow");
    if presp.hovered() {
        ui.painter().rect_filled(play, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), play.shrink(5.0), Icon::Play, t.icon);
    if presp.clicked() {
        app.run("proto.present", json!({"start": f.start}));
    }
    let name_rect = Rect::from_min_max(pos2(play.max.x + 6.0, r.center().y - 9.0), pos2(r.max.x - 40.0, r.center().y + 9.0));
    if let Some(n) = widgets::text_field_inline(ui, Id::new(("flow_name", f.start)), &f.name, name_rect, regular(11.0)) {
        app.run("proto.flow.rename", json!({"start": f.start, "name": n}));
    }
    let rm = Rect::from_center_size(pos2(r.max.x - 20.0, r.center().y), vec2(24.0, 24.0));
    let rresp = ui.interact(rm, ui.id().with(("flow_rm", f.start)), Sense::click()).on_hover_text("Remove starting point");
    if rresp.hovered() {
        ui.painter().rect_filled(rm, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), rm.shrink(5.0), Icon::Minus, t.icon);
    if rresp.clicked() {
        app.run("proto.flow.remove", json!({"start": f.start}));
    }
}

fn flows_overview(app: &mut FigApp, ui: &mut Ui, doc: &Document, page: NodeId) {
    let t = Tokens::get(ui.ctx());
    let flows = flows_of(doc, page);
    if widgets::section_header(ui, "Flow starting points", &[(Icon::Plus, "Add starting point")], flows.is_empty()).is_some() {
        let free = doc.top_frames(page).into_iter().find(|f| !flows.iter().any(|x| x.start == *f));
        match free {
            Some(f) => {
                app.run("proto.flow.add", json!({"start": f}));
            }
            None => app.toast("Every top-level frame already starts a flow"),
        }
    }
    if flows.is_empty() {
        row(ui, 36.0, |ui| widgets::label(ui, "Select a frame and add a starting point to present it as a flow.", t.text_secondary));
    }
    for f in &flows {
        flow_row(app, ui, f);
    }
}

fn settings(app: &mut FigApp, ui: &mut Ui, doc: &Document, page: NodeId) {
    widgets::section_header(ui, "Prototype settings", &[], false);
    let (device, bg) = match doc.node(page).map(|n| &n.kind) {
        Some(NodeKind::Page { prototype, .. }) => (prototype.device.clone(), prototype.background),
        _ => ("None".into(), figcraft_color::Color::BLACK),
    };
    widgets::sublabels(ui, "Device", None);
    row(ui, 32.0, |ui| {
        let resp = widgets::dropdown(ui, &device, ui.available_width(), None);
        egui::Popup::menu(&resp).show(|ui| {
            for d in ["None", "iPhone 16", "iPhone 16 Pro Max", "Android Compact", "iPad mini", "MacBook Air", "Desktop"] {
                if menu_item(ui, d, "") {
                    app.run("proto.settings", json!({"device": d}));
                    ui.close();
                }
            }
        });
    });
    widgets::sublabels(ui, "Background", None);
    row(ui, 32.0, |ui| {
        widgets::swatch(ui, &Paint::solid(bg), 16.0);
        if let Some(h) = widgets::text_field(ui, Id::new("proto_bg"), &bg.to_hex(), 96.0, "")
            && let Some(c) = figcraft_color::Color::from_hex(&h)
        {
            app.run("proto.settings", json!({"background": c.to_css_hex()}));
        }
    });
}

fn flow_section(app: &mut FigApp, ui: &mut Ui, doc: &Document, page: NodeId, frame: NodeId) {
    let flows = flows_of(doc, page);
    let mine = flows.iter().find(|f| f.start == frame).cloned();
    let buttons: &[(Icon, &str)] = if mine.is_none() { &[(Icon::Plus, "Add starting point")] } else { &[] };
    if widgets::section_header(ui, "Flow starting point", buttons, mine.is_none()).is_some() {
        app.run("proto.flow.add", json!({"start": frame}));
    }
    if let Some(f) = mine {
        flow_row(app, ui, &f);
        row(ui, 32.0, |ui| {
            if let Some(d) = widgets::text_field(ui, Id::new(("flow_desc", frame)), &f.description, ui.available_width(), "Add a description")
                && d != f.description
            {
                app.run("proto.flow.rename", json!({"start": frame, "description": d}));
            }
        });
    }
}

fn interactions(app: &mut FigApp, ui: &mut Ui, doc: &Document, page: NodeId, node: &Node) {
    let t = Tokens::get(ui.ctx());
    let id = node.id;
    let list = node.interactions.clone();
    if widgets::section_header(ui, "Interactions", &[(Icon::Plus, "Add interaction")], list.is_empty()).is_some() {
        // Click → navigate to the next frame (or Back when there's nowhere to go).
        let action = action_of("Navigate to", doc, page, id, None).unwrap_or(Action::Back);
        if let Some(r) = app.run("proto.interaction.add", json!({"ids": [id], "trigger": {"type": "onClick"}, "action": action})) {
            let k = r.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            open_editor(ui.ctx(), Some((id, k)));
        }
    }
    let open = editor(ui.ctx());
    let mut anchor: Option<Rect> = None;
    for (i, it) in list.iter().enumerate() {
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        let body = Rect::from_min_max(pos2(r.min.x + PAD - 6.0, r.min.y + 2.0), pos2(r.max.x - 40.0, r.max.y - 2.0));
        let resp = ui.interact(body, ui.id().with(("proto_it", id, i)), Sense::click());
        let active = open == Some((id, i));
        if active {
            ui.painter().rect_filled(body, CornerRadius::same(5), t.selected_row);
            anchor = Some(r);
        } else if resp.hovered() {
            ui.painter().rect_filled(body, CornerRadius::same(5), t.hover);
        }
        // A small connection glyph.
        let g = pos2(body.min.x + 14.0, body.center().y);
        ui.painter().circle_stroke(g, 5.0, Stroke::new(1.2, ACCENT));
        ui.painter().circle_filled(g, 2.0, ACCENT);
        let tx = body.min.x + 28.0;
        let clip = ui.painter().with_clip_rect(body);
        clip.text(pos2(tx, body.center().y - 7.0), Align2::LEFT_CENTER, trigger_summary(&it.trigger), semibold(11.0), t.text);
        let summary = it.actions.iter().map(|a| action_summary(doc, a)).collect::<Vec<_>>().join(", ");
        clip.text(pos2(tx, body.center().y + 8.0), Align2::LEFT_CENTER, summary, regular(11.0), t.text_secondary);
        if resp.clicked() {
            open_editor(ui.ctx(), if active { None } else { Some((id, i)) });
        }
        let rm = Rect::from_center_size(pos2(r.max.x - 20.0, r.center().y), vec2(24.0, 24.0));
        let rresp = ui.interact(rm, ui.id().with(("proto_rm", id, i)), Sense::click()).on_hover_text("Remove interaction");
        if rresp.hovered() {
            ui.painter().rect_filled(rm, CornerRadius::same(5), t.hover);
        }
        icons::paint(ui.painter(), rm.shrink(5.0), Icon::Minus, t.icon);
        if rresp.clicked() {
            open_editor(ui.ctx(), None);
            app.run("proto.interaction.remove", json!({"ids": [id], "index": i}));
            return;
        }
    }
    if let Some((n, k)) = open
        && n == id
        && let Some(it) = list.get(k)
    {
        let panel_left = ui.clip_rect().min.x;
        let y = anchor.map(|a| a.min.y).unwrap_or(ui.clip_rect().min.y + 80.0);
        editor_popover(app, ui.ctx(), doc, page, id, k, it, pos2(panel_left - 8.0, y));
    } else if open.is_some_and(|(n, _)| n == id) {
        open_editor(ui.ctx(), None);
    }
}

/// The interaction editor, floating left of the panel (`at` is its top-right corner).
#[allow(clippy::too_many_arguments)]
fn editor_popover(app: &mut FigApp, ctx: &egui::Context, doc: &Document, page: NodeId, node: NodeId, k: usize, it: &Interaction, at: Pos2) {
    let t = Tokens::get(ctx);
    let width = 264.0;
    let screen = ctx.content_rect();
    let pos = pos2(at.x - width, at.y.min(screen.max.y - 420.0).max(screen.min.y + 48.0));
    let mut close = false;
    egui::Area::new(Id::new("proto_editor_area")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::new()
            .fill(t.panel)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(CornerRadius::same(8))
            .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: t.shadow })
            .show(ui, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                // Header.
                let (hr, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
                ui.painter().text(pos2(hr.min.x + PAD, hr.center().y), Align2::LEFT_CENTER, "Interaction", semibold(11.0), t.text);
                let x = Rect::from_center_size(pos2(hr.max.x - 20.0, hr.center().y), vec2(24.0, 24.0));
                let xr = ui.interact(x, ui.id().with("proto_editor_close"), Sense::click()).on_hover_text("Close");
                if xr.hovered() {
                    ui.painter().rect_filled(x, CornerRadius::same(5), t.hover);
                }
                icons::paint(ui.painter(), x.shrink(6.0), Icon::Close, t.icon);
                if xr.clicked() {
                    close = true;
                }
                widgets::separator(ui);
                trigger_rows(app, ui, node, k, it);
                widgets::separator(ui);
                action_rows(app, ui, doc, page, node, k, it);
                if !matches!(it.actions.first(), Some(Action::OpenUrl { .. } | Action::SetVariable { .. } | Action::CloseOverlay)) {
                    widgets::separator(ui);
                    animation_rows(app, ui, node, k, it);
                }
                ui.add_space(8.0);
            });
    });
    if close || (ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !capturing(ctx)) {
        open_editor(ctx, None);
    }
}

fn capture_id() -> Id {
    Id::new("proto_key_capture")
}

fn capturing(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(capture_id())).unwrap_or(false)
}

fn trigger_rows(app: &mut FigApp, ui: &mut Ui, node: NodeId, k: usize, it: &Interaction) {
    let t = Tokens::get(ui.ctx());
    widgets::sublabels(ui, "Trigger", None);
    row(ui, 32.0, |ui| {
        let resp = widgets::dropdown(ui, trigger_kind(&it.trigger), ui.available_width(), None);
        egui::Popup::menu(&resp).show(|ui| {
            for (i, l) in TRIGGERS.iter().enumerate() {
                if i == 4 || i == 5 || i == 9 {
                    ui.separator();
                }
                if menu_item(ui, l, "") {
                    let tr = trigger_of(l, &it.trigger);
                    patch(app, node, k, json!({"trigger": tr}));
                    if matches!(tr, Trigger::KeyPress { ref key } if key.is_empty()) {
                        ui.ctx().data_mut(|d| d.insert_temp(capture_id(), true));
                    }
                    ui.close();
                }
            }
        });
    });
    match &it.trigger {
        Trigger::AfterDelay { ms } => {
            row(ui, 32.0, |ui| {
                widgets::label(ui, "Delay", t.text_secondary);
                if let Some(e) = widgets::number_field(ui, Id::new(("proto_delay", node, k)), Lead::None, Some(f64::from(*ms)), "ms", 96.0, 10.0) {
                    let v = match e {
                        FieldEdit::Set(v) | FieldEdit::Scrub(v, _) => v,
                    };
                    if v.is_finite() {
                        patch(app, node, k, json!({"trigger": {"type": "afterDelay", "ms": v.clamp(0.0, 600_000.0).round() as u32}}));
                    }
                }
            });
        }
        Trigger::KeyPress { key } => {
            row(ui, 32.0, |ui| {
                let cap = capturing(ui.ctx());
                let label = if cap {
                    "Press a key…".to_string()
                } else if key.is_empty() {
                    "Click to record a key".to_string()
                } else {
                    key.clone()
                };
                let w = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(w, FIELD_H), Sense::click());
                let id = capture_id().with("field");
                ui.painter().rect_filled(r, CornerRadius::same(5), t.field);
                if cap {
                    ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, t.accent), StrokeKind::Inside);
                    ui.memory_mut(|m| m.request_focus(id));
                    let _ = ui.interact(r, id, Sense::focusable_noninteractive());
                    let pressed = ui.input(|i| {
                        i.events.iter().find_map(|e| match e {
                            egui::Event::Key { key, pressed: true, .. } => Some(*key),
                            _ => None,
                        })
                    });
                    if let Some(kk) = pressed {
                        ui.ctx().data_mut(|d| d.remove::<bool>(capture_id()));
                        ui.memory_mut(|m| m.surrender_focus(id));
                        if kk != egui::Key::Escape || key.is_empty() {
                            patch(app, node, k, json!({"trigger": {"type": "keyPress", "key": kk.name()}}));
                        }
                    }
                } else if resp.hovered() {
                    ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, t.field_hover_border), StrokeKind::Inside);
                }
                ui.painter().text(
                    pos2(r.min.x + 8.0, r.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    regular(11.0),
                    if cap || key.is_empty() { t.text_secondary } else { t.text },
                );
                if resp.clicked() && !cap {
                    ui.ctx().data_mut(|d| d.insert_temp(capture_id(), true));
                }
            });
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn dest_dropdown(
    app: &mut FigApp,
    ui: &mut Ui,
    doc: &Document,
    options: &[NodeId],
    cur: Option<NodeId>,
    label: &str,
    make: impl Fn(NodeId) -> Action,
    node: NodeId,
    k: usize,
) {
    widgets::sublabels(ui, label, None);
    row(ui, 32.0, |ui| {
        let text = cur.map(|d| name_of(doc, d)).unwrap_or_else(|| "None".into());
        let resp = widgets::dropdown(ui, &text, ui.available_width(), Some(Icon::Frame));
        egui::Popup::menu(&resp).show(|ui| {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for d in options {
                    let n = name_of(doc, *d);
                    let mark = if Some(*d) == cur { format!("✓ {n}") } else { n };
                    if menu_item(ui, &mark, "") {
                        patch(app, node, k, json!({"action": make(*d)}));
                        ui.close();
                    }
                }
            });
        });
    });
}

fn action_rows(app: &mut FigApp, ui: &mut Ui, doc: &Document, page: NodeId, node: NodeId, k: usize, it: &Interaction) {
    let t = Tokens::get(ui.ctx());
    let action = it.actions.first().cloned().unwrap_or(Action::Back);
    widgets::sublabels(ui, "Action", None);
    row(ui, 32.0, |ui| {
        let resp = widgets::dropdown(ui, action.label(), ui.available_width(), None);
        egui::Popup::menu(&resp).show(|ui| {
            for (i, l) in ACTIONS.iter().enumerate() {
                if i == 2 || i == 4 || i == 5 || i == 8 {
                    ui.separator();
                }
                if menu_item(ui, l, "") {
                    match action_of(l, doc, page, node, Some(&action)) {
                        Some(a) => patch(app, node, k, json!({"action": a})),
                        None => app.toast(match *l {
                            "Change to" => "Change to needs a layer inside a variant",
                            "Set variable" => "Create a variable first",
                            _ => "There's no destination for that action",
                        }),
                    }
                    ui.close();
                }
            }
        });
    });
    let frames = frames_for(doc, page);
    match &action {
        Action::Navigate { dest } => dest_dropdown(app, ui, doc, &frames, Some(*dest), "Destination", |d| Action::Navigate { dest: d }, node, k),
        Action::SwapOverlay { dest } => dest_dropdown(app, ui, doc, &frames, Some(*dest), "Overlay", |d| Action::SwapOverlay { dest: d }, node, k),
        Action::ChangeTo { dest } => {
            let vs = variants_for(doc, node);
            dest_dropdown(app, ui, doc, &vs, Some(*dest), "Variant", |d| Action::ChangeTo { dest: d }, node, k);
        }
        Action::ScrollTo { dest } => {
            let layers: Vec<NodeId> = doc.top_level_frame(node).map(|f| doc.subtree(f).into_iter().skip(1).take(400).collect()).unwrap_or_default();
            dest_dropdown(app, ui, doc, &layers, Some(*dest), "Scroll to layer", |d| Action::ScrollTo { dest: d }, node, k);
        }
        Action::OpenUrl { url } => {
            widgets::sublabels(ui, "Link", None);
            row(ui, 32.0, |ui| {
                if let Some(u) = widgets::text_field(ui, Id::new(("proto_url", node, k)), url, ui.available_width(), "https://")
                    && &u != url
                {
                    patch(app, node, k, json!({"action": Action::OpenUrl { url: u.trim().to_string() }}));
                }
            });
        }
        Action::SetVariable { variable, value } => {
            let vars = doc.variables.variables.clone();
            widgets::sublabels(ui, "Variable", None);
            row(ui, 32.0, |ui| {
                let name = doc.variables.get(variable).map(|v| v.name.clone()).unwrap_or_else(|| "Choose variable".into());
                let resp = widgets::dropdown(ui, &name, ui.available_width(), None);
                egui::Popup::menu(&resp).show(|ui| {
                    for v in &vars {
                        if menu_item(ui, &v.name, "") {
                            patch(app, node, k, json!({"action": {"type": "setVariable", "variable": v.id, "value": value}}));
                            ui.close();
                        }
                    }
                });
            });
            row(ui, 32.0, |ui| {
                widgets::label(ui, "Value", t.text_secondary);
                let shown = match value {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                if let Some(s) = widgets::text_field(ui, Id::new(("proto_varval", node, k)), &shown, ui.available_width(), "Value") {
                    let v = serde_json::from_str::<Value>(s.trim()).ok().filter(|v| !v.is_string()).unwrap_or(Value::String(s));
                    patch(app, node, k, json!({"action": {"type": "setVariable", "variable": variable, "value": v}}));
                }
            });
        }
        Action::OpenOverlay { dest, position, close_on_outside, background } => {
            let (dest, position, close_on_outside, background) = (*dest, *position, *close_on_outside, *background);
            let mk = move |d: NodeId| Action::OpenOverlay { dest: d, position, close_on_outside, background };
            dest_dropdown(app, ui, doc, &frames, Some(dest), "Overlay", mk, node, k);
            widgets::sublabels(ui, "Overlay position", None);
            row(ui, 32.0, |ui| {
                let resp = widgets::dropdown(ui, position.label(), ui.available_width(), None);
                egui::Popup::menu(&resp).show(|ui| {
                    for p in OverlayPosition::ALL {
                        if menu_item(ui, p.label(), "") {
                            patch(app, node, k, json!({"action": Action::OpenOverlay { dest, position: p, close_on_outside, background }}));
                            ui.close();
                        }
                    }
                });
            });
            row(ui, 28.0, |ui| {
                if widgets::checkbox(ui, close_on_outside, "Close when clicking outside") {
                    patch(app, node, k, json!({"action": Action::OpenOverlay { dest, position, close_on_outside: !close_on_outside, background }}));
                }
            });
            row(ui, 28.0, |ui| {
                if widgets::checkbox(ui, background.is_some(), "Add background behind overlay") {
                    let bg = if background.is_some() { None } else { Some(figcraft_color::Color::new(0.0, 0.0, 0.0, 0.25)) };
                    patch(app, node, k, json!({"action": Action::OpenOverlay { dest, position, close_on_outside, background: bg }}));
                }
            });
            if let Some(bg) = background {
                row(ui, 32.0, |ui| {
                    widgets::swatch(ui, &Paint::solid(bg), 16.0);
                    if let Some(h) = widgets::text_field(ui, Id::new(("proto_ovbg", node, k)), &bg.to_hex(), 96.0, "")
                        && let Some(c) = figcraft_color::Color::from_hex(&h)
                    {
                        patch(app, node, k, json!({"action": Action::OpenOverlay { dest, position, close_on_outside, background: Some(c) }}));
                    }
                    let mut alpha = (bg.a * 100.0).round();
                    if let Some(e) = widgets::number_field(ui, Id::new(("proto_ovbg_a", node, k)), Lead::None, Some(f64::from(alpha)), "%", 64.0, 1.0)
                    {
                        let v = match e {
                            FieldEdit::Set(v) | FieldEdit::Scrub(v, _) => v,
                        };
                        if v.is_finite() {
                            alpha = v.clamp(0.0, 100.0) as f32;
                            patch(
                                app,
                                node,
                                k,
                                json!({"action": Action::OpenOverlay { dest, position, close_on_outside, background: Some(bg.with_alpha(alpha / 100.0)) }}),
                            );
                        }
                    }
                });
            }
        }
        Action::Back | Action::CloseOverlay => {}
    }
}

fn animation_rows(app: &mut FigApp, ui: &mut Ui, node: NodeId, k: usize, it: &Interaction) {
    let t = Tokens::get(ui.ctx());
    widgets::sublabels(ui, "Animation", None);
    row(ui, 32.0, |ui| {
        let resp = widgets::dropdown(ui, it.transition.label(), ui.available_width(), None);
        egui::Popup::menu(&resp).show(|ui| {
            for (i, l) in TRANSITIONS.iter().enumerate() {
                if i == 3 {
                    ui.separator();
                }
                if menu_item(ui, l, "") {
                    patch(app, node, k, json!({"transition": transition_of(l, it.transition)}));
                    ui.close();
                }
            }
        });
    });
    if let Some(d) = it.transition.direction() {
        row(ui, 32.0, |ui| {
            let dirs = [Direction::Left, Direction::Right, Direction::Top, Direction::Bottom];
            let items = [(Icon::ChevronLeft, "Left"), (Icon::ChevronRight, "Right"), (Icon::ChevronUp, "Up"), (Icon::Chevron, "Down")];
            let active = dirs.iter().position(|x| *x == d);
            if let Some(i) = widgets::segmented(ui, &items, active, 32.0)
                && let Some(nd) = dirs.get(i)
            {
                patch(app, node, k, json!({"transition": it.transition.with_direction(*nd)}));
            }
        });
    }
    if !matches!(it.transition, Transition::Instant) {
        row(ui, 32.0, |ui| {
            let resp = widgets::dropdown(ui, it.easing.label(), ui.available_width() - 80.0, None);
            egui::Popup::menu(&resp).width(220.0).show(|ui| {
                let custom = [Easing::CUSTOM_BEZIER, Easing::CUSTOM_SPRING];
                for (i, e) in Easing::ALL.iter().chain(custom.iter()).enumerate() {
                    if i == 7 || i == Easing::ALL.len() {
                        ui.separator();
                    }
                    let w = ui.available_width().max(200.0);
                    let (r, rr) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
                    if rr.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(5), ACCENT);
                    }
                    let col = if rr.hovered() { Color32::WHITE } else { t.text };
                    curve(ui.painter(), Rect::from_min_size(pos2(r.min.x + 6.0, r.min.y + 4.0), vec2(28.0, 18.0)), *e, col, None);
                    ui.painter().text(pos2(r.min.x + 42.0, r.center().y), Align2::LEFT_CENTER, e.label(), regular(11.5), col);
                    if rr.clicked() {
                        // Keep the current custom values when re-picking the same custom kind.
                        let same_kind = std::mem::discriminant(e) == std::mem::discriminant(&it.easing);
                        patch(app, node, k, json!({"easing": if same_kind { it.easing } else { *e }}));
                        ui.close();
                    }
                }
            });
            if let Some(e) = widgets::number_field(ui, Id::new(("proto_dur", node, k)), Lead::None, Some(f64::from(it.duration_ms)), "ms", 72.0, 10.0)
            {
                match e {
                    FieldEdit::Set(v) => {
                        if v.is_finite() {
                            patch(app, node, k, json!({"durationMs": v.clamp(1.0, 10_000.0).round()}));
                        }
                    }
                    FieldEdit::Scrub(v, done) => {
                        let _ = app.session.begin_interaction("Edit");
                        let _ = app
                            .session
                            .execute("proto.interaction.set", &json!({"ids": [node], "index": k, "durationMs": v.clamp(1.0, 10_000.0).round()}));
                        if done {
                            let _ = app.session.commit_interaction();
                        }
                    }
                }
            }
        });
        custom_easing_fields(app, ui, node, k, it.easing);
        // Curve preview, with a dot running along it.
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
        let box_r = Rect::from_min_max(pos2(r.min.x + PAD, r.min.y + 4.0), pos2(r.max.x - PAD, r.max.y - 4.0));
        ui.painter().rect_filled(box_r, CornerRadius::same(5), t.field);
        let period = (f64::from(it.duration_ms) / 1000.0).max(0.05) + 0.6;
        let phase = ((ui.input(|i| i.time) % period) / (period - 0.6)).min(1.0);
        curve(ui.painter(), box_r.shrink2(vec2(10.0, 12.0)), it.easing, ACCENT, Some(phase));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// Fields for a custom bezier (x1, y1, x2, y2) or spring (mass, stiffness, damping).
fn custom_easing_fields(app: &mut FigApp, ui: &mut Ui, node: NodeId, k: usize, e: Easing) {
    let (names, vals, steps): ([&str; 4], [f64; 4], [f64; 4]) = match e {
        Easing::CustomBezier { x1, y1, x2, y2 } => (["X1", "Y1", "X2", "Y2"], [x1, y1, x2, y2], [0.01; 4]),
        Easing::CustomSpring { mass, stiffness, damping } => {
            (["Mass", "Stiffness", "Damping", ""], [mass, stiffness, damping, 0.0], [0.1, 1.0, 0.5, 0.0])
        }
        _ => return,
    };
    let count = if matches!(e, Easing::CustomBezier { .. }) { 4 } else { 3 };
    let make = |vals: [f64; 4]| match e {
        Easing::CustomBezier { .. } => Easing::CustomBezier {
            x1: vals[0].clamp(0.0, 1.0),
            y1: vals[1].clamp(-5.0, 5.0),
            x2: vals[2].clamp(0.0, 1.0),
            y2: vals[3].clamp(-5.0, 5.0),
        },
        _ => Easing::CustomSpring { mass: vals[0].clamp(0.01, 100.0), stiffness: vals[1].clamp(1.0, 10_000.0), damping: vals[2].clamp(0.0, 1000.0) },
    };
    row(ui, 32.0, |ui| {
        let w = ((ui.available_width() - 6.0 * (count as f32 - 1.0)) / count as f32).max(36.0);
        for i in 0..count {
            let (Some(name), Some(v), Some(step)) = (names.get(i), vals.get(i), steps.get(i)) else { continue };
            let lead = if count == 4 { Lead::Text(name) } else { Lead::None };
            let edit = widgets::number_field(ui, Id::new(("proto_ease", node, k, i)), lead, Some((v * 100.0).round() / 100.0), "", w, *step).map(
                |e| match e {
                    FieldEdit::Set(x) => (x, true, false),
                    FieldEdit::Scrub(x, done) => (x, done, true),
                },
            );
            if let Some((x, done, scrub)) = edit
                && x.is_finite()
            {
                let mut nv = vals;
                if let Some(slot) = nv.get_mut(i) {
                    *slot = x;
                }
                let eased = make(nv);
                if scrub {
                    let _ = app.session.begin_interaction("Edit");
                    let _ = app.session.execute("proto.interaction.set", &json!({"ids": [node], "index": k, "easing": eased}));
                    if done {
                        let _ = app.session.commit_interaction();
                    }
                } else {
                    patch(app, node, k, json!({"easing": eased}));
                }
            }
        }
    });
    if count == 3 {
        widgets::sublabels(ui, "Mass · Stiffness · Damping", None);
    }
}

/// Draw an easing curve in `r` (x = time, y = progress, up). `dot` marks a point in time.
pub fn curve(p: &egui::Painter, r: Rect, e: Easing, col: Color32, dot: Option<f64>) {
    let n = 48;
    let at = |x: f64| {
        let y = e.eval(x);
        pos2(r.min.x + r.width() * x as f32, r.max.y - r.height() * y as f32)
    };
    let pts: Vec<Pos2> = (0..=n).map(|i| at(f64::from(i) / f64::from(n))).collect();
    p.with_clip_rect(r.expand(r.height() * 0.6)).add(Shape::line(pts, Stroke::new(1.5, col)));
    if let Some(x) = dot {
        p.circle_filled(at(x.clamp(0.0, 1.0)), 3.0, col);
    }
}

fn scroll_section(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId], node: &Node) {
    let t = Tokens::get(ui.ctx());
    let in_frame = node.parent.and_then(|p| doc.node(p)).is_some_and(|p| p.kind.is_frame_like());
    let is_frame = node.kind.is_frame_like();
    if !in_frame && !is_frame {
        return;
    }
    widgets::section_header(ui, "Scroll behavior", &[], false);
    if in_frame {
        row(ui, 32.0, |ui| {
            widgets::label(ui, "Position", t.text_secondary);
            let cur = match node.scroll_position {
                ScrollPosition::Scrolls => "Scroll with parent",
                ScrollPosition::Fixed => "Fixed",
                ScrollPosition::Sticky => "Sticky",
            };
            let resp = widgets::dropdown(ui, cur, ui.available_width(), None);
            egui::Popup::menu(&resp).show(|ui| {
                for (l, v) in [("Scroll with parent", "scrolls"), ("Fixed", "fixed"), ("Sticky", "sticky")] {
                    if menu_item(ui, l, "") {
                        app.set_props(sel, json!({"scrollPosition": v}));
                        ui.close();
                    }
                }
            });
        });
    }
    if is_frame {
        row(ui, 32.0, |ui| {
            widgets::label(ui, "Overflow", t.text_secondary);
            let cur = match node.overflow {
                Overflow::None => "No scrolling",
                Overflow::Horizontal => "Horizontal",
                Overflow::Vertical => "Vertical",
                Overflow::Both => "Both directions",
            };
            let resp = widgets::dropdown(ui, cur, ui.available_width(), None);
            egui::Popup::menu(&resp).show(|ui| {
                for (l, v) in [("No scrolling", "none"), ("Horizontal", "horizontal"), ("Vertical", "vertical"), ("Both directions", "both")] {
                    if menu_item(ui, l, "") {
                        app.set_props(sel, json!({"overflow": v}));
                        ui.close();
                    }
                }
            });
        });
    }
}
