//! The left panel: file header, File/Assets tabs, pages and the layers tree.

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{ACCENT, COMPONENT, Tokens, regular, semibold};
use crate::{FigApp, LeftTab};

pub const ROW: f32 = 32.0;

pub fn node_icon(doc: &Document, id: NodeId) -> Icon {
    let Some(n) = doc.node(id) else { return Icon::Rectangle };
    match &n.kind {
        NodeKind::Page { .. } => Icon::Page,
        NodeKind::Frame => {
            if let Some(l) = &n.layout {
                match l.mode {
                    figcraft_doc::LayoutMode::Vertical => Icon::LayoutV,
                    figcraft_doc::LayoutMode::Grid => Icon::LayoutGrid,
                    _ => Icon::LayoutH,
                }
            } else {
                Icon::Frame
            }
        }
        NodeKind::Group => Icon::Group,
        NodeKind::Section { .. } => Icon::Section,
        NodeKind::Rectangle => {
            if n.fills.iter().any(|f| matches!(f.kind, figcraft_color::PaintKind::Image(_))) {
                Icon::Image
            } else {
                Icon::Rectangle
            }
        }
        NodeKind::Ellipse { .. } => Icon::Ellipse,
        NodeKind::Polygon { .. } => Icon::Polygon,
        NodeKind::Star { .. } => Icon::Star,
        NodeKind::Line => {
            if n.stroke.cap_end.is_arrow() {
                Icon::Arrow
            } else {
                Icon::Line
            }
        }
        NodeKind::Vector { .. } => Icon::Vector,
        NodeKind::Text(_) => Icon::Text,
        NodeKind::BooleanOp { op } => match op {
            figcraft_doc::BoolOp::Union => Icon::Union,
            figcraft_doc::BoolOp::Subtract => Icon::Subtract,
            figcraft_doc::BoolOp::Intersect => Icon::Intersect,
            figcraft_doc::BoolOp::Exclude => Icon::Exclude,
        },
        NodeKind::Component { .. } => Icon::Component,
        NodeKind::ComponentSet { .. } => Icon::ComponentSet,
        NodeKind::Instance { .. } => Icon::Instance,
        NodeKind::Slice => Icon::Slice,
    }
}

/// The slim icon rail at the far left: main menu, File, Assets, Variables.
pub fn rail(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let r = ui.max_rect();
    let mut y = r.min.y + 8.0;
    // Main menu (logo).
    let lr = Rect::from_center_size(pos2(r.center().x, y + 16.0), vec2(36.0, 32.0));
    let lresp = ui.interact(lr, ui.id().with("rail_menu"), Sense::click()).on_hover_text("Main menu");
    if lresp.hovered() || app.ui.main_menu {
        ui.painter().rect_filled(lr, CornerRadius::same(6), t.hover);
    }
    icons::paint(ui.painter(), Rect::from_center_size(lr.center(), vec2(18.0, 18.0)), Icon::Logo, t.icon);
    if lresp.clicked() {
        app.ui.main_menu = !app.ui.main_menu;
    }
    y += 44.0;
    ui.painter().line_segment([pos2(r.min.x + 18.0, y), pos2(r.max.x - 18.0, y)], Stroke::new(1.0, t.border));
    y += 12.0;
    let items = [(Icon::Page, "File", Some(LeftTab::File)), (Icon::Assets, "Assets", Some(LeftTab::Assets)), (Icon::Styles, "Variables", None)];
    for (icon, label, tab) in items {
        let ir = Rect::from_center_size(pos2(r.center().x, y + 14.0), vec2(36.0, 28.0));
        let hit = Rect::from_min_max(pos2(r.min.x, y), pos2(r.max.x, y + 48.0));
        let resp = ui.interact(hit, ui.id().with(("rail", label)), Sense::click()).on_hover_text(label);
        let active = tab.is_some_and(|tb| tb == app.ui.left_tab && !app.ui.left_collapsed);
        if active {
            ui.painter().rect_filled(ir, CornerRadius::same(6), t.selected_row);
        } else if resp.hovered() {
            ui.painter().rect_filled(ir, CornerRadius::same(6), t.hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(ir.center(), vec2(16.0, 16.0)), icon, if active { t.accent_text } else { t.icon });
        ui.painter().text(pos2(r.center().x, y + 38.0), Align2::CENTER_CENTER, label, regular(10.0), t.text);
        if resp.clicked() {
            match tab {
                Some(tb) => {
                    if app.ui.left_tab == tb && !app.ui.left_collapsed {
                        app.ui.left_collapsed = true;
                    } else {
                        app.ui.left_tab = tb;
                        app.ui.left_collapsed = false;
                    }
                }
                None => {
                    ui.data_mut(|d| d.insert_temp(Id::new("variables_open"), true));
                }
            }
        }
        y += 60.0;
    }
}

pub fn left_panel(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let full = ui.max_rect();
    // Header: file name (or "Assets") + drafts + collapse.
    let (head, _) = ui.allocate_exact_size(vec2(full.width(), 64.0), Sense::hover());
    if app.ui.left_tab == LeftTab::Assets {
        ui.painter().text(pos2(head.min.x + 16.0, head.min.y + 24.0), Align2::LEFT_CENTER, "Assets", semibold(13.0), t.text);
    } else {
        let name = app.session.doc().map(|d| d.name.clone()).unwrap_or_default();
        let nr = Rect::from_min_max(pos2(head.min.x + 14.0, head.min.y + 14.0), pos2(head.max.x - 48.0, head.min.y + 34.0));
        if let Some(new) = crate::widgets::text_field_inline(ui, Id::new("filename"), &name, nr, semibold(13.0)) {
            app.run("file.rename", json!({"name": new}));
        }
        ui.painter().text(pos2(head.min.x + 16.0, head.min.y + 46.0), Align2::LEFT_CENTER, "Drafts", regular(11.0), t.text_secondary);
    }
    let hr = Rect::from_center_size(pos2(head.max.x - 24.0, head.min.y + 24.0), vec2(24.0, 24.0));
    let hresp = ui.interact(hr, ui.id().with("hideui"), Sense::click()).on_hover_text("Collapse panel");
    if hresp.hovered() {
        ui.painter().rect_filled(hr, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), hr.shrink(5.0), Icon::Panels, t.icon);
    if hresp.clicked() {
        app.ui.left_collapsed = true;
    }
    ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, t.border));
    match app.ui.left_tab {
        LeftTab::File => {
            pages(app, ui);
            layers(app, ui);
        }
        LeftTab::Assets => assets(app, ui),
    }
}

fn pages(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let tresp = ui.interact(Rect::from_min_size(hr.min, vec2(w - 40.0, 40.0)), ui.id().with("pages_toggle"), Sense::click());
    ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Pages", semibold(11.0), t.text);
    if tresp.clicked() {
        app.ui.pages_open = !app.ui.pages_open;
    }
    let sr = Rect::from_center_size(pos2(hr.max.x - 50.0, hr.center().y), vec2(24.0, 24.0));
    let sresp = ui.interact(sr, ui.id().with("lsearch"), Sense::click()).on_hover_text("Find  ⌘F");
    if sresp.hovered() {
        ui.painter().rect_filled(sr, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), sr.shrink(5.0), Icon::Search, t.icon);
    if sresp.clicked() {
        app.ui.find = Some((String::new(), String::new()));
    }
    let ar = Rect::from_center_size(pos2(hr.max.x - 22.0, hr.center().y), vec2(24.0, 24.0));
    let aresp = ui.interact(ar, ui.id().with("addpage"), Sense::click()).on_hover_text("Add new page");
    if aresp.hovered() {
        ui.painter().rect_filled(ar, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), ar.shrink(5.0), Icon::Plus, t.icon);
    if aresp.clicked() {
        app.run("page.add", json!({}));
    }
    if !app.ui.pages_open {
        return;
    }
    let Ok(ds) = app.session.ds() else { return };
    let pages: Vec<(NodeId, String)> = ds.doc.pages.iter().filter_map(|p| ds.doc.node(*p).map(|n| (*p, n.name.clone()))).collect();
    let cur = ds.page;
    let mut select = None;
    let mut rename_done: Option<(NodeId, String)> = None;
    for (id, name) in pages {
        let (r, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
        let r = r.shrink2(vec2(8.0, 0.0));
        if resp.hovered() || id == cur {
            ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
        }
        if app.ui.rename_page.as_ref().is_some_and(|(i, _)| *i == id) {
            let tr = Rect::from_min_max(pos2(r.min.x + 8.0, r.min.y + 4.0), pos2(r.max.x - 4.0, r.max.y - 4.0));
            if let Some(new) =
                crate::widgets::text_field_inline(ui, Id::new(("pagename", id)), &name, tr, if id == cur { semibold(11.0) } else { regular(11.0) })
            {
                rename_done = Some((id, new));
            }
        } else {
            ui.painter().text(
                pos2(r.min.x + 10.0, r.center().y),
                Align2::LEFT_CENTER,
                &name,
                if id == cur { semibold(11.0) } else { regular(11.0) },
                t.text,
            );
        }
        if resp.clicked() {
            select = Some(id);
        }
        if resp.double_clicked() {
            app.ui.rename_page = Some((id, name.clone()));
            ui.memory_mut(|m| m.request_focus(Id::new(("pagename", id)).with("te")));
        }
        resp.context_menu(|ui| {
            if ui.button("Rename page").clicked() {
                app.ui.rename_page = Some((id, name.clone()));
                ui.close();
            }
            if ui.button("Duplicate page").clicked() {
                app.run("page.duplicate", json!({"id": id}));
                ui.close();
            }
            if ui.button("Delete page").clicked() {
                app.run("page.delete", json!({"id": id}));
                ui.close();
            }
        });
    }
    if let Some((id, new)) = rename_done {
        app.ui.rename_page = None;
        app.run("page.rename", json!({"id": id, "name": new}));
    }
    if let Some(p) = select
        && p != cur
    {
        app.run("page.select", json!({"id": p}));
    }
    crate::widgets::separator(ui);
}

struct Row {
    id: NodeId,
    depth: usize,
    has_kids: bool,
}

fn collect(doc: &Document, id: NodeId, depth: usize, expanded: &std::collections::BTreeSet<NodeId>, out: &mut Vec<Row>) {
    let Some(n) = doc.node(id) else { return };
    // Layers panel lists top of the stack first.
    for c in n.children.iter().rev() {
        let Some(cn) = doc.node(*c) else { continue };
        let has_kids = !cn.children.is_empty() || matches!(cn.kind, NodeKind::BooleanOp { .. });
        out.push(Row { id: *c, depth, has_kids });
        if expanded.contains(c) && depth < 64 {
            collect(doc, *c, depth + 1, expanded, out);
        }
    }
}

/// Layers whose name contains `q` (lowercase), top of the stack first, as a flat list.
fn matching(doc: &Document, id: NodeId, q: &str, out: &mut Vec<Row>) {
    let Some(n) = doc.node(id) else { return };
    for c in n.children.iter().rev() {
        let Some(cn) = doc.node(*c) else { continue };
        let name = if cn.name.is_empty() { cn.kind.label() } else { cn.name.as_str() };
        if name.to_lowercase().contains(q) {
            out.push(Row { id: *c, depth: 0, has_kids: false });
        }
        if out.len() < 5000 {
            matching(doc, *c, q, out);
        }
    }
}

/// Expand or collapse `id` and everything inside it (Alt-click on a disclosure triangle).
fn set_expanded_deep(doc: &Document, id: NodeId, open: bool, expanded: &mut std::collections::BTreeSet<NodeId>) {
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        if open {
            expanded.insert(n);
        } else {
            expanded.remove(&n);
        }
        if let Some(node) = doc.node(n) {
            stack.extend(node.children.iter().copied());
        }
    }
}

/// The search row under the Layers header (live filter; Escape closes it).
fn search_field(app: &mut FigApp, ui: &mut Ui, t: &Tokens, w: f32, id: Id) {
    let (fr, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    let field = Rect::from_min_max(pos2(fr.min.x + 12.0, fr.min.y + 2.0), pos2(fr.max.x - 12.0, fr.max.y - 6.0));
    ui.painter().rect_filled(field, CornerRadius::same(5), t.field);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(field.min.x + 12.0, field.center().y), vec2(12.0, 12.0)), Icon::Search, t.icon_secondary);
    let te_rect = Rect::from_min_max(pos2(field.min.x + 24.0, field.center().y - 7.0), pos2(field.max.x - 6.0, field.center().y + 7.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(te_rect));
    let r = child.add(
        egui::TextEdit::singleline(&mut app.ui.layer_filter)
            .id(id)
            .frame(egui::Frame::NONE)
            .font(regular(11.0))
            .hint_text("Find layers")
            .vertical_align(egui::Align::Center)
            .desired_width(te_rect.width()),
    );
    if r.has_focus() {
        ui.painter().rect_stroke(field, CornerRadius::same(5), Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    }
    if (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.ui.layer_search = false;
        app.ui.layer_filter.clear();
    }
}

fn layers(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Layers", semibold(11.0), t.text);
    let sr = Rect::from_center_size(pos2(hr.max.x - 22.0, hr.center().y), vec2(24.0, 24.0));
    let sresp = ui.interact(sr, ui.id().with("layer_search_btn"), Sense::click()).on_hover_text("Search layers");
    if sresp.hovered() || app.ui.layer_search {
        ui.painter().rect_filled(sr, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), sr.shrink(5.0), Icon::Search, t.icon);
    let search_id = Id::new("layer_search_field");
    if sresp.clicked() {
        app.ui.layer_search = !app.ui.layer_search;
        app.ui.layer_filter.clear();
        if app.ui.layer_search {
            ui.memory_mut(|m| m.request_focus(search_id));
        }
    }
    if app.ui.layer_search {
        search_field(app, ui, &t, w, search_id);
    }
    let Ok(ds) = app.session.ds() else { return };
    let doc = ds.doc.clone();
    let page = ds.page;
    let sel = ds.selection.clone();
    let hover = ds.hover;
    // Reveal a new selection: expand its ancestors and scroll its row into view, once, so
    // "Collapse layers" and manual collapsing stick.
    let reveal = sel != app.ui.layers_seen_sel;
    if reveal {
        for s in &sel {
            for a in doc.ancestors(*s) {
                if a != page {
                    app.ui.expanded.insert(a);
                }
            }
        }
        app.ui.layers_seen_sel = sel.clone();
    }
    let filter = app.ui.layer_filter.trim().to_lowercase();
    let mut rows = Vec::new();
    if filter.is_empty() {
        collect(&doc, page, 0, &app.ui.expanded, &mut rows);
    } else {
        matching(&doc, page, &filter, &mut rows);
    }
    let scroll_to = if reveal { sel.first().copied() } else { None };
    let mut clicked: Option<(NodeId, egui::Modifiers)> = None;
    let mut toggle: Option<NodeId> = None;
    let mut deep: Option<(NodeId, bool)> = None;
    let mut rename_next: Option<(usize, bool)> = None;
    let mut vis: Option<NodeId> = None;
    let mut lock: Option<NodeId> = None;
    let mut rename_done: Option<(NodeId, String)> = None;
    let mut hovered_row: Option<NodeId> = None;
    let drag_id = Id::new("layer_drag");
    let mut drop: Option<(NodeId, i8)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if rows.is_empty() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                let hint = if filter.is_empty() { "Add a frame (F) or a shape (R) to get started" } else { "No matching layers" };
                crate::widgets::label(ui, hint, t.text_secondary);
            });
        }
        let dragging: Option<NodeId> = ui.data(|d| d.get_temp(drag_id));
        for (ri, row) in rows.iter().enumerate() {
            let Some(n) = doc.node(row.id) else { continue };
            let (r, resp) = ui.allocate_exact_size(vec2(w, ROW), Sense::click_and_drag());
            if scroll_to == Some(row.id) {
                ui.scroll_to_rect(r, None);
            }
            let selected = sel.contains(&row.id);
            let in_selected = !selected && doc.ancestors(row.id).iter().any(|a| sel.contains(a));
            let is_comp = matches!(n.kind, NodeKind::Component { .. } | NodeKind::ComponentSet { .. } | NodeKind::Instance { .. });
            let rr = r.shrink2(vec2(8.0, 0.0));
            if selected {
                ui.painter().rect_filled(rr, CornerRadius::same(5), if is_comp { t.component_row } else { t.selected_row });
            } else if in_selected {
                ui.painter().rect_filled(rr, CornerRadius::ZERO, t.selected_row_child);
            } else if resp.hovered() || hover == Some(row.id) {
                ui.painter().rect_stroke(
                    rr,
                    CornerRadius::same(5),
                    Stroke::new(1.0, if is_comp { COMPONENT } else { ACCENT }),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.hovered() {
                hovered_row = Some(row.id);
            }
            let x0 = rr.min.x + 4.0 + row.depth as f32 * 16.0;
            // Expand triangle.
            if row.has_kids {
                let tri = Rect::from_center_size(pos2(x0 + 6.0, r.center().y), vec2(14.0, 14.0));
                let tresp = ui.interact(tri, ui.id().with(("tri", row.id)), Sense::click());
                let open = app.ui.expanded.contains(&row.id);
                icons::paint(ui.painter(), tri.shrink(1.0), if open { Icon::Chevron } else { Icon::ChevronRight }, t.icon_secondary);
                if tresp.clicked() {
                    if ui.input(|i| i.modifiers.alt) {
                        deep = Some((row.id, !open));
                    } else {
                        toggle = Some(row.id);
                    }
                }
            }
            let icon_col = if is_comp {
                COMPONENT
            } else if n.visible {
                t.icon
            } else {
                t.text_tertiary
            };
            icons::paint(ui.painter(), Rect::from_center_size(pos2(x0 + 22.0, r.center().y), vec2(14.0, 14.0)), node_icon(&doc, row.id), icon_col);
            let name_x = x0 + 36.0;
            let editing = app.ui.rename.as_ref().is_some_and(|(i, _)| *i == row.id);
            if editing {
                // Tab / Shift+Tab: commit and rename the next / previous layer.
                if let Some(back) = ui.input(|i| i.key_pressed(egui::Key::Tab).then_some(i.modifiers.shift)) {
                    rename_next = Some((ri, back));
                }
                let tr = Rect::from_min_max(pos2(name_x - 4.0, r.min.y + 4.0), pos2(rr.max.x - 4.0, r.max.y - 4.0));
                if let Some(new) = crate::widgets::text_field_inline(ui, Id::new(("rename", row.id)), &n.name, tr, regular(11.0)) {
                    rename_done = Some((row.id, new));
                }
            } else {
                let col = if is_comp {
                    COMPONENT
                } else if n.visible {
                    t.text
                } else {
                    t.text_tertiary
                };
                let font = if doc.node(n.parent.unwrap_or(page)).is_some_and(|p| p.is_page())
                    && (n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. }))
                {
                    semibold(11.0)
                } else {
                    regular(11.0)
                };
                let name = if n.name.is_empty() { n.kind.label().to_string() } else { n.name.clone() };
                let max_w = (rr.max.x - 56.0 - name_x).max(20.0);
                let galley = ui.painter().layout(name, font, col, max_w);
                ui.painter().with_clip_rect(Rect::from_min_max(pos2(name_x, r.min.y), pos2(rr.max.x - 52.0, r.max.y))).galley(
                    pos2(name_x, r.center().y - galley.size().y / 2.0),
                    galley,
                    col,
                );
            }
            // Lock / eye on hover (or when set).
            let show_lock = resp.hovered() || n.locked;
            let show_eye = resp.hovered() || !n.visible;
            let er = Rect::from_center_size(pos2(rr.max.x - 14.0, r.center().y), vec2(20.0, 20.0));
            let lr = Rect::from_center_size(pos2(rr.max.x - 36.0, r.center().y), vec2(20.0, 20.0));
            if show_eye {
                let eresp = ui.interact(er, ui.id().with(("eye", row.id)), Sense::click());
                icons::paint(ui.painter(), er.shrink(3.0), if n.visible { Icon::Eye } else { Icon::EyeOff }, t.icon_secondary);
                if eresp.clicked() {
                    vis = Some(row.id);
                }
            }
            if show_lock {
                let lresp = ui.interact(lr, ui.id().with(("lock", row.id)), Sense::click());
                icons::paint(ui.painter(), lr.shrink(3.0), if n.locked { Icon::Lock } else { Icon::Unlock }, t.icon_secondary);
                if lresp.clicked() {
                    lock = Some(row.id);
                }
            }
            if resp.clicked() {
                clicked = Some((row.id, ui.input(|i| i.modifiers)));
            }
            if resp.double_clicked() {
                app.ui.rename = Some((row.id, n.name.clone()));
                ui.memory_mut(|m| m.request_focus(Id::new(("rename", row.id)).with("te")));
            }
            if resp.secondary_clicked() {
                if !sel.contains(&row.id) {
                    clicked = Some((row.id, egui::Modifiers::NONE));
                }
                let pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(r.center());
                app.ui.context_menu = Some((pos, vec![row.id]));
            }
            // Drag to reorder.
            if resp.drag_started() {
                ui.data_mut(|d| d.insert_temp(drag_id, row.id));
            }
            if let Some(dg) = dragging
                && dg != row.id
                && let Some(p) = ui.input(|i| i.pointer.hover_pos())
                && r.contains(p)
            {
                let rel = (p.y - r.min.y) / r.height();
                let into = n.kind.is_container() && !matches!(n.kind, NodeKind::Instance { .. }) && (0.25..0.75).contains(&rel);
                let zone: i8 = if into {
                    0
                } else if rel < 0.5 {
                    -1
                } else {
                    1
                };
                let y = match zone {
                    -1 => r.min.y,
                    1 => r.max.y,
                    _ => r.center().y,
                };
                if zone == 0 {
                    ui.painter().rect_stroke(rr, CornerRadius::same(5), Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
                } else {
                    ui.painter().line_segment([pos2(x0, y), pos2(rr.max.x, y)], Stroke::new(2.0, ACCENT));
                }
                if ui.input(|i| i.pointer.any_released()) {
                    drop = Some((row.id, zone));
                }
            }
        }
        // Empty space below clears selection.
        let rest = ui.available_rect_before_wrap();
        if rest.height() > 0.0 {
            let resp = ui.allocate_rect(rest, Sense::click());
            if resp.clicked() {
                app.run("select.none", json!({}));
            }
        }
    });
    if ui.input(|i| i.pointer.any_released()) {
        let dragged: Option<NodeId> = ui.data(|d| d.get_temp(drag_id));
        ui.data_mut(|d| d.remove::<NodeId>(drag_id));
        if let (Some(dg), Some((target, zone))) = (dragged, drop) {
            let ids = if sel.contains(&dg) { sel.clone() } else { vec![dg] };
            match zone {
                0 => {
                    app.run("node.reparent", json!({"ids": ids, "parent": target}));
                }
                z => {
                    // Above in the list = higher z = after the target among siblings.
                    if let Some(parent) = doc.node(target).and_then(|n| n.parent) {
                        let idx = doc.index_in_parent(target).unwrap_or(0);
                        let index = if z < 0 { idx + 1 } else { idx };
                        app.run("node.reparent", json!({"ids": ids, "parent": parent, "index": index}));
                    }
                }
            }
        }
    }
    if let Ok(ds) = app.session.ds_mut() {
        let h = hovered_row;
        if h.is_some() && ds.hover != h {
            ds.hover = h;
        }
    }
    if let Some(id) = toggle
        && !app.ui.expanded.remove(&id)
    {
        app.ui.expanded.insert(id);
    }
    if let Some((id, open)) = deep {
        set_expanded_deep(&doc, id, open, &mut app.ui.expanded);
    }
    if let Some(id) = vis {
        app.run("object.toggleVisible", json!({"ids": [id]}));
    }
    if let Some(id) = lock {
        app.run("object.toggleLock", json!({"ids": [id]}));
    }
    if let Some((id, new)) = rename_done {
        app.ui.rename = None;
        app.run("object.rename", json!({"ids": [id], "name": new}));
    }
    if let Some((i, back)) = rename_next {
        app.ui.rename = None;
        let next = if back { i.checked_sub(1) } else { i.checked_add(1) }.and_then(|j| rows.get(j));
        if let Some(row) = next
            && let Some(n) = doc.node(row.id)
        {
            app.ui.rename = Some((row.id, n.name.clone()));
            app.run("select.set", json!({"ids": [row.id]}));
            app.ui.layers_seen_sel = app.session.selection();
            ui.memory_mut(|m| m.request_focus(Id::new(("rename", row.id)).with("te")));
        }
    }
    if let Some((id, m)) = clicked {
        let order: Vec<NodeId> = rows.iter().map(|r| r.id).collect();
        let pos = |x: NodeId| order.iter().position(|o| *o == x);
        let anchor = app.ui.layer_anchor.filter(|a| sel.contains(a)).and_then(pos);
        if m.shift
            && let (Some(ai), Some(bi)) = (anchor, pos(id))
        {
            // Shift: the visible rows from the anchor to here (added to the selection with Cmd).
            let (lo, hi) = (ai.min(bi), ai.max(bi));
            let mut s: Vec<NodeId> = if m.command { sel.clone() } else { vec![] };
            for x in order.get(lo..=hi).unwrap_or_default() {
                if !s.contains(x) {
                    s.push(*x);
                }
            }
            app.run("select.set", json!({"ids": s}));
        } else if m.shift || m.command {
            let mut s = sel.clone();
            if let Some(pos) = s.iter().position(|x| *x == id) {
                s.remove(pos);
            } else {
                s.push(id);
            }
            app.ui.layer_anchor = Some(id);
            app.run("select.set", json!({"ids": s}));
        } else {
            app.ui.layer_anchor = Some(id);
            app.run("select.set", json!({"ids": [id]}));
        }
        // A click in the panel needs no reveal scroll.
        app.ui.layers_seen_sel = app.session.selection();
        if let Ok(ds) = app.session.ds_mut()
            && ds.text_edit.is_some()
        {
            let _ = figcraft_engine::textedit::end(&mut app.session);
        }
    }
    let _ = Color32::TRANSPARENT;
}

fn assets(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let Ok(doc) = app.session.doc() else { return };
    let comps: Vec<(NodeId, String, bool)> = doc
        .nodes()
        .filter(|n| {
            matches!(n.kind, NodeKind::Component { .. })
                && !doc.node(n.parent.unwrap_or(NodeId(0))).is_some_and(|p| matches!(p.kind, NodeKind::ComponentSet { .. }))
                || matches!(n.kind, NodeKind::ComponentSet { .. })
        })
        .map(|n| (n.id, n.name.clone(), matches!(n.kind, NodeKind::ComponentSet { .. })))
        .collect();
    let styles: Vec<(String, String, String)> = doc.styles.iter().map(|s| (s.id.clone(), s.name.clone(), s.kind.type_name().to_string())).collect();
    let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Local components", semibold(11.0), t.text);
    let mut place = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if comps.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                crate::widgets::label(ui, "No components yet. Select layers and press ⌥⌘K.", t.text_secondary);
            });
        }
        for (id, name, set) in &comps {
            let (r, resp) = ui.allocate_exact_size(vec2(w, ROW), Sense::click());
            let rr = r.shrink2(vec2(8.0, 0.0));
            if resp.hovered() {
                ui.painter().rect_filled(rr, CornerRadius::same(5), t.hover);
            }
            icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(rr.min.x + 14.0, r.center().y), vec2(14.0, 14.0)),
                if *set { Icon::ComponentSet } else { Icon::Component },
                COMPONENT,
            );
            ui.painter().text(pos2(rr.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, name, regular(11.0), t.text);
            let resp = resp.on_hover_text("Click to place an instance at the centre of the view");
            if resp.clicked() {
                place = Some((*id, *set));
            }
        }
        ui.add_space(8.0);
        crate::widgets::separator(ui);
        let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), Align2::LEFT_CENTER, "Local styles", semibold(11.0), t.text);
        for (_, name, ty) in &styles {
            let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            let icon = match ty.as_str() {
                "text" => Icon::Text,
                "effect" => Icon::Effects,
                "grid" => Icon::Grid,
                _ => Icon::Fill,
            };
            icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 22.0, r.center().y), vec2(14.0, 14.0)), icon, t.icon_secondary);
            ui.painter().text(pos2(r.min.x + 40.0, r.center().y), Align2::LEFT_CENTER, name, regular(11.0), t.text);
        }
    });
    if let Some((id, set)) = place {
        let comp = if set { app.session.doc().ok().and_then(|d| d.node(id)).and_then(|n| n.children.first().copied()).unwrap_or(id) } else { id };
        let (cx, cy) = app.session.ds().map(|d| (d.view.cx, d.view.cy)).unwrap_or((0.0, 0.0));
        app.run("instance.create", json!({"component": comp, "x": cx, "y": cy}));
    }
}
