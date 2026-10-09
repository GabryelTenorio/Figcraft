//! The canvas: the rendered page plus selection, hover, guides, labels and text-editing overlays.

use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::{Document, NodeId, NodeKind};
use figcraft_engine::Preedit;
use figcraft_engine::tools::{Cursor, PointerEvent, PointerKind, ToolId, selection_box, selection_frame};
use figcraft_geom::Point;
use serde_json::json;

use crate::FigApp;
use crate::render_worker::{Job, Worker};
use crate::theme::{ACCENT, COMPONENT, Tokens, c32, regular, semibold};

pub struct CanvasState {
    pub rect: Rect,
    texture: Option<egui::TextureHandle>,
    tex_view: Option<figcraft_render::View>,
    submitted: Option<JobKey>,
    generation: u64,
    /// What the last canvas render had to do (shown by `ui.inspect`).
    pub render_work: Option<figcraft_render::Work>,
    dragging: bool,
    space_pan: bool,
    middle_pan: Option<(Pos2, f64, f64)>,
    pub last_pointer: Option<Point>,
    preedit: Option<PreeditView>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            rect: Rect::NOTHING,
            texture: None,
            tex_view: None,
            submitted: None,
            generation: 0,
            render_work: None,
            dragging: false,
            space_pan: false,
            middle_pan: None,
            last_pointer: None,
            preedit: None,
        }
    }
}

/// What the canvas shows while an IME composition is in progress: the document with the
/// composition inserted at the caret (the real document only gets it on commit).
pub struct PreeditView {
    /// The document, composition and caret it was built from.
    key: (Arc<Document>, Preedit, usize),
    pub doc: Arc<Document>,
    /// The composition's char range, and the IME cursor, in `doc`'s edited text.
    pub range: (usize, usize),
    pub cursor: usize,
}

impl FigApp {
    fn preedit_key(&self) -> Option<(Arc<Document>, Preedit, usize)> {
        let ds = self.session.ds().ok()?;
        let te = ds.text_edit.as_ref()?;
        Some((ds.doc.clone(), te.preedit.clone()?, te.caret))
    }

    /// Rebuild the composition view when the document or composition changed.
    pub fn sync_preedit(&mut self) {
        if self.preedit_view().is_some() {
            return;
        }
        self.canvas.preedit = self.preedit_key().and_then(|key| {
            let (doc, range, cursor) = figcraft_engine::textedit::preedit_doc(&self.session)?;
            Some(PreeditView { key, doc: Arc::new(doc), range, cursor })
        });
    }

    /// The composition view, when it matches the current state.
    pub fn preedit_view(&self) -> Option<&PreeditView> {
        let p = self.canvas.preedit.as_ref()?;
        let (doc, pe, caret) = self.preedit_key()?;
        (Arc::ptr_eq(&p.key.0, &doc) && p.key.1 == pe && p.key.2 == caret).then_some(p)
    }
}

#[derive(Clone, PartialEq)]
struct JobKey {
    doc: usize,
    page: NodeId,
    origin: (i64, i64),
    scale: i64,
    size: (u32, u32),
    outline: bool,
    skip: Vec<NodeId>,
    /// Text lays out anew when fonts arrive (system fonts load in the background).
    fonts: u64,
}

impl FigApp {
    /// Page → screen (logical points).
    pub fn to_screen(&self, x: f64, y: f64) -> Pos2 {
        let Ok(ds) = self.session.ds() else { return Pos2::ZERO };
        let (sx, sy) = ds.view.page_to_screen(x, y);
        pos2(self.canvas.rect.min.x + sx as f32, self.canvas.rect.min.y + sy as f32)
    }
    pub fn to_page(&self, p: Pos2) -> Point {
        let Ok(ds) = self.session.ds() else { return Point::ZERO };
        let (x, y) = ds.view.screen_to_page(f64::from(p.x - self.canvas.rect.min.x), f64::from(p.y - self.canvas.rect.min.y));
        Point::new(x, y)
    }
}

fn pointer_event(app: &FigApp, kind: PointerKind, pos: Pos2, m: &egui::Modifiers) -> PointerEvent {
    let p = app.to_page(pos);
    let zoom = app.session.ds().map(|d| d.view.zoom).unwrap_or(1.0);
    PointerEvent { kind, x: p.x, y: p.y, sx: f64::from(pos.x), sy: f64::from(pos.y), shift: m.shift, alt: m.alt, cmd: m.command, ctrl: m.ctrl, zoom }
}

pub fn show(app: &mut FigApp, ui: &mut Ui, worker: &mut Worker) {
    let rect = ui.max_rect();
    app.canvas.rect = rect;
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let resp = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
    if let Ok(ds) = app.session.ds_mut()
        && ((ds.view.vw - f64::from(rect.width())).abs() > 0.5 || (ds.view.vh - f64::from(rect.height())).abs() > 0.5)
    {
        // Keep the centre; the first sizing after opening re-fits.
        let first = ds.view.vw == 1200.0 && ds.view.vh == 800.0;
        ds.view.vw = f64::from(rect.width());
        ds.view.vh = f64::from(rect.height());
        if first {
            ds.zoom_to_fit();
        }
    }
    input(app, ui, &resp);
    app.sync_preedit();
    request_render(app, worker, ppp);
    if let Some(done) = worker.poll()
        && done.generation == app.canvas.generation
    {
        match &mut app.canvas.texture {
            Some(t) => t.set(done.image, egui::TextureOptions::LINEAR),
            None => app.canvas.texture = Some(ctx.load_texture("figcraft-canvas", done.image, egui::TextureOptions::LINEAR)),
        }
        app.canvas.tex_view = Some(done.view);
        app.canvas.render_work = Some(done.work);
        app.ui.render_ms = done.ms;
    }
    let painter = ui.painter_at(rect);
    let t = Tokens::get(&ctx);
    // Page background (also behind a stale/partial texture).
    let bg = app
        .session
        .ds()
        .ok()
        .and_then(|ds| match ds.doc.node(ds.page).map(|n| &n.kind) {
            Some(NodeKind::Page { background, .. }) => Some(c32(*background)),
            _ => None,
        })
        .unwrap_or(t.canvas);
    painter.rect_filled(rect, CornerRadius::ZERO, bg);
    if let (Some(tex), Some(v)) = (&app.canvas.texture, &app.canvas.tex_view) {
        // Re-project the last render onto the current view.
        let tl = app.to_screen(v.origin.0, v.origin.1);
        let zoom = app.session.ds().map(|d| d.view.zoom).unwrap_or(1.0);
        let w = (f64::from(v.width) / v.scale * zoom) as f32;
        let h = (f64::from(v.height) / v.scale * zoom) as f32;
        painter.image(tex.id(), Rect::from_min_size(tl, vec2(w, h)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    overlays(app, &painter, &t);
    crate::handles::draw(app, &painter);
    crate::comments::draw(app, &painter);
    crate::comments::popovers(app, &ctx);
    crate::versions::preview_banner(app, &ctx, rect);
    // Cursor.
    if resp.hovered() {
        let c = if app.canvas.space_pan || app.canvas.middle_pan.is_some() {
            if ctx.input(|i| i.pointer.any_down()) { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab }
        } else {
            match app.session.tool.overlay.cursor {
                Cursor::Default => egui::CursorIcon::Default,
                Cursor::Crosshair | Cursor::Pen => egui::CursorIcon::Crosshair,
                Cursor::Move => egui::CursorIcon::Move,
                Cursor::Resize(a) => {
                    let a = ((a % 180.0) + 180.0) % 180.0;
                    if !(22.5..157.5).contains(&a) {
                        egui::CursorIcon::ResizeHorizontal
                    } else if a < 67.5 {
                        egui::CursorIcon::ResizeNwSe
                    } else if a < 112.5 {
                        egui::CursorIcon::ResizeVertical
                    } else {
                        egui::CursorIcon::ResizeNeSw
                    }
                }
                Cursor::Rotate => egui::CursorIcon::Alias,
                Cursor::Grab => egui::CursorIcon::Grab,
                Cursor::Grabbing => egui::CursorIcon::Grabbing,
                Cursor::Text => egui::CursorIcon::Text,
                Cursor::Copy => egui::CursorIcon::Copy,
            }
        };
        ctx.set_cursor_icon(c);
    }
}

fn request_render(app: &mut FigApp, worker: &mut Worker, ppp: f32) {
    let Ok(ds) = app.session.ds() else { return };
    let rect = app.canvas.rect;
    let scale = ds.view.zoom * f64::from(ppp);
    let (ox, oy) = ds.view.origin();
    let w = (rect.width() * ppp).ceil().max(1.0) as u32;
    let h = (rect.height() * ppp).ceil().max(1.0) as u32;
    // The edited text is drawn live (an IME composition included); the caret is an overlay.
    let doc = app.preedit_view().map(|p| p.doc.clone()).unwrap_or_else(|| ds.doc.clone());
    let key = JobKey {
        doc: Arc::as_ptr(&doc) as usize,
        page: ds.page,
        origin: ((ox * 1000.0) as i64, (oy * 1000.0) as i64),
        scale: (scale * 10000.0) as i64,
        size: (w, h),
        outline: ds.view.outlines,
        skip: vec![],
        fonts: figcraft_text::layout_generation(),
    };
    if app.canvas.submitted.as_ref() == Some(&key) {
        return;
    }
    app.canvas.generation += 1;
    let job = Job {
        doc,
        page: ds.page,
        view: figcraft_render::View { origin: (ox, oy), scale, width: w.min(16384), height: h.min(16384) },
        outline: ds.view.outlines,
        skip: vec![],
        generation: app.canvas.generation,
    };
    worker.submit(job);
    app.canvas.submitted = Some(key);
}

fn input(app: &mut FigApp, ui: &mut Ui, resp: &egui::Response) {
    if crate::proto_canvas::input(app, ui, resp) {
        return;
    }
    let ctx = ui.ctx().clone();
    let (mods, scroll, zoom_delta, hover_pos, space) =
        ctx.input(|i| (i.modifiers, i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos(), i.key_down(egui::Key::Space)));
    let editing_text = app.session.ds().map(|d| d.text_edit.is_some()).unwrap_or(false);
    let typing_elsewhere = ctx.memory(|m| m.focused().is_some());
    app.canvas.space_pan = space && !editing_text && !typing_elsewhere;
    // Dropped files import onto the canvas.
    let dropped = ctx.input(|i| i.raw.dropped_files.clone());
    for f in dropped {
        let path = f.path().to_path_buf();
        let pt = hover_pos.map(|h| app.to_page(h)).unwrap_or_default();
        if path.exists() {
            app.run("file.import", json!({"path": path.to_string_lossy(), "x": pt.x, "y": pt.y}));
        } else if let Some(bytes) = dropped_bytes(&f) {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped.png".into());
            app.run("file.import", json!({"data": b64, "name": name, "x": pt.x, "y": pt.y}));
        }
    }
    let Some(pos) = hover_pos.filter(|p| app.canvas.rect.contains(*p)) else {
        return;
    };
    // Zoom (pinch, Cmd/Ctrl+scroll) about the pointer; plain scroll pans.
    if resp.hovered() {
        if (zoom_delta - 1.0).abs() > 1e-4 {
            let anchor = app.to_page(pos);
            if let Ok(ds) = app.session.ds_mut() {
                let z = ds.view.zoom * f64::from(zoom_delta);
                ds.view.set_zoom_at(z, (anchor.x, anchor.y));
                ds.revision += 1;
            }
        } else if scroll != egui::Vec2::ZERO
            && let Ok(ds) = app.session.ds_mut()
        {
            let z = ds.view.zoom.max(1e-6);
            let (dx, dy) = if mods.shift && scroll.x == 0.0 { (scroll.y, 0.0) } else { (scroll.x, scroll.y) };
            ds.view.cx -= f64::from(dx) / z;
            ds.view.cy -= f64::from(dy) / z;
            ds.revision += 1;
        }
    }
    // Middle-button / space panning.
    let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
    if (middle_down || (app.canvas.space_pan && ctx.input(|i| i.pointer.primary_down()))) && resp.hovered() || app.canvas.middle_pan.is_some() {
        if let Ok(ds) = app.session.ds_mut() {
            match app.canvas.middle_pan {
                None => app.canvas.middle_pan = Some((pos, ds.view.cx, ds.view.cy)),
                Some((p0, cx, cy)) => {
                    let z = ds.view.zoom.max(1e-6);
                    ds.view.cx = cx - f64::from(pos.x - p0.x) / z;
                    ds.view.cy = cy - f64::from(pos.y - p0.y) / z;
                    ds.revision += 1;
                }
            }
        }
        if !middle_down && !ctx.input(|i| i.pointer.primary_down()) {
            app.canvas.middle_pan = None;
        }
        return;
    }
    if !app.canvas.dragging && resp.hovered() && crate::comments::input(app, &ctx, pos) {
        return;
    }
    if !app.canvas.dragging && resp.hovered() && crate::guides::input(app, &ctx, pos) {
        return;
    }
    if !app.canvas.dragging && resp.hovered() && crate::handles::input(app, &ctx, pos) {
        return;
    }
    if resp.secondary_clicked() {
        // Right-click: select what's under the pointer, then the context menu.
        let ev = pointer_event(app, PointerKind::Hover, pos, &mods);
        if let Ok(ds) = app.session.ds() {
            let (doc, page, sel) = (ds.doc.clone(), ds.page, ds.selection.clone());
            if let Some(leaf) = doc.hit_test(page, Point::new(ev.x, ev.y), 2.0 / ev.zoom) {
                let target = figcraft_doc::geometry::select_target(&doc, leaf, &sel, mods.command);
                if !sel.contains(&target) {
                    app.session.set_selection(vec![target]);
                }
            }
        }
        app.ui.context_menu = Some((pos, app.session.selection()));
        return;
    }
    if resp.double_clicked() {
        let ev = pointer_event(app, PointerKind::DoubleClick, pos, &mods);
        if let Err(e) = app.session.pointer(&ev) {
            app.toast(e.to_string());
        }
        app.canvas.dragging = false;
        return;
    }
    let primary_pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
    let primary_down = ctx.input(|i| i.pointer.primary_down());
    let primary_released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
    let kind = if primary_pressed && resp.hovered() {
        app.canvas.dragging = true;
        app.ui.context_menu = None;
        app.ui.tool_flyout = None;
        // Clicking the canvas takes focus from panel fields.
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        Some(PointerKind::Down)
    } else if app.canvas.dragging && primary_released {
        app.canvas.dragging = false;
        Some(PointerKind::Up)
    } else if app.canvas.dragging && primary_down {
        Some(PointerKind::Move)
    } else if resp.hovered() {
        Some(PointerKind::Hover)
    } else {
        None
    };
    if let Some(k) = kind {
        let ev = pointer_event(app, k, pos, &mods);
        app.canvas.last_pointer = Some(Point::new(ev.x, ev.y));
        if let Err(e) = app.session.pointer(&ev) {
            app.toast(e.to_string());
        }
        app.handle_ui_requests();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn dropped_bytes(f: &std::sync::Arc<dyn egui::DroppedFile + Send + Sync>) -> Option<Vec<u8>> {
    f.bytes().ok()
}

#[cfg(target_arch = "wasm32")]
fn dropped_bytes(_f: &std::sync::Arc<dyn egui::DroppedFile + Send + Sync>) -> Option<Vec<u8>> {
    // Browser drops arrive asynchronously; not supported yet.
    None
}

fn stroke(c: Color32, w: f32) -> Stroke {
    Stroke::new(w, c)
}

/// A node's oriented outline in screen space.
fn node_quad(app: &FigApp, doc: &Document, id: NodeId) -> Option<[Pos2; 4]> {
    let n = doc.node(id)?;
    let w = doc.world_transform(id);
    let corners = [Point::new(0.0, 0.0), Point::new(n.width, 0.0), Point::new(n.width, n.height), Point::new(0.0, n.height)];
    Some(corners.map(|c| {
        let p = w * c;
        app.to_screen(p.x, p.y)
    }))
}

fn quad(painter: &egui::Painter, q: [Pos2; 4], s: Stroke) {
    painter.add(Shape::closed_line(q.to_vec(), s));
}

/// The outline path of shapes (ellipses, vectors) for hover/selection outlines.
fn outline_shape(app: &FigApp, doc: &Document, id: NodeId) -> Option<Vec<Vec<Pos2>>> {
    let n = doc.node(id)?;
    if !matches!(
        n.kind,
        NodeKind::Ellipse { .. } | NodeKind::Vector { .. } | NodeKind::Polygon { .. } | NodeKind::Star { .. } | NodeKind::BooleanOp { .. }
    ) {
        return None;
    }
    let (path, _) = figcraft_render::node_outline(doc, n)?;
    let w = doc.world_transform(id);
    let mut out = Vec::new();
    let mut cur: Vec<Pos2> = Vec::new();
    figcraft_geom::kurbo::flatten(path.iter(), 0.5 / app.session.ds().map(|d| d.view.zoom).unwrap_or(1.0), |el| {
        use figcraft_geom::kurbo::PathEl::*;
        match el {
            MoveTo(p) => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                let q = w * p;
                cur.push(app.to_screen(q.x, q.y));
            }
            LineTo(p) => {
                let q = w * p;
                cur.push(app.to_screen(q.x, q.y));
            }
            ClosePath => {
                if let Some(f) = cur.first().copied() {
                    cur.push(f);
                }
            }
            _ => {}
        }
    });
    if cur.len() > 1 {
        out.push(cur);
    }
    Some(out)
}

fn overlays(app: &FigApp, painter: &egui::Painter, t: &Tokens) {
    let Ok(ds) = app.session.ds() else { return };
    let doc = ds.doc.clone();
    let page = ds.page;
    let sel = ds.selection.clone();
    let zoom = ds.view.zoom;
    let view = ds.view.clone();
    // Pixel grid at high zoom.
    if view.pixel_grid && zoom >= 8.0 {
        let r = app.canvas.rect;
        let p0 = app.to_page(r.min);
        let p1 = app.to_page(r.max);
        let col = Color32::from_black_alpha(18);
        let mut x = p0.x.floor();
        while x <= p1.x {
            let s = app.to_screen(x, 0.0).x;
            painter.line_segment([pos2(s, r.min.y), pos2(s, r.max.y)], stroke(col, 1.0));
            x += 1.0;
        }
        let mut y = p0.y.floor();
        while y <= p1.y {
            let s = app.to_screen(0.0, y).y;
            painter.line_segment([pos2(r.min.x, s), pos2(r.max.x, s)], stroke(col, 1.0));
            y += 1.0;
        }
    }
    let Some(pg) = doc.node(page) else { return };
    // Layout grids and frame labels for top-level frames.
    for c in &pg.children {
        let Some(n) = doc.node(*c) else { continue };
        if !n.visible {
            continue;
        }
        let Some(b) = doc.world_bounds(*c) else { continue };
        // Off-screen frames draw neither grids nor a label (labels sit just above the frame).
        let on_screen = Rect::from_two_pos(app.to_screen(b.x0, b.y0), app.to_screen(b.x1, b.y1)).expand2(vec2(0.0, 20.0));
        if !on_screen.intersects(app.canvas.rect) {
            continue;
        }
        if view.layout_grids && !n.grids.is_empty() {
            let w = doc.world_transform(*c);
            for g in n.grids.iter().filter(|g| g.visible) {
                let col = c32(g.color);
                for r in figcraft_layout::grid_lines(g, n.width, n.height) {
                    let a = w * Point::new(r.x0, r.y0);
                    let b = w * Point::new(r.x1, r.y1);
                    let (sa, sb) = (app.to_screen(a.x, a.y), app.to_screen(b.x, b.y));
                    if g.kind == figcraft_doc::GridKind::Grid {
                        painter.line_segment([sa, sb], stroke(col, 1.0));
                    } else {
                        painter.rect_filled(Rect::from_two_pos(sa, sb), CornerRadius::ZERO, col);
                    }
                }
            }
        }
        let labelled = n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. });
        if labelled {
            let tl = app.to_screen(b.x0, b.y0);
            let selected = sel.contains(c);
            let hovered = ds.hover == Some(*c);
            let is_comp = matches!(n.kind, NodeKind::Component { .. } | NodeKind::ComponentSet { .. });
            let col = if is_comp {
                COMPONENT
            } else if selected || hovered {
                ACCENT
            } else {
                t.text_secondary
            };
            if matches!(n.kind, NodeKind::Section { .. }) {
                // Section titles are pills inside the top-left.
                let galley = painter.layout_no_wrap(n.name.clone(), semibold(11.0), t.text);
                let r = Rect::from_min_size(tl + vec2(0.0, 0.0), galley.size() + vec2(16.0, 8.0));
                painter.rect_filled(r, CornerRadius::same(4), if selected { ACCENT } else { t.hover });
                painter.galley(r.min + vec2(8.0, 4.0), galley, if selected { Color32::WHITE } else { t.text });
                status_badge(painter, n.dev_status, pos2(r.max.x + 6.0, r.center().y));
            } else {
                let max_w = (b.width() * zoom) as f32;
                let mut x = tl.x;
                if is_comp {
                    crate::icons::paint(painter, Rect::from_min_size(pos2(x, tl.y - 16.0), vec2(12.0, 12.0)), crate::icons::Icon::Component, col);
                    x += 14.0;
                }
                let galley = painter.layout(n.name.clone(), regular(11.0), col, max_w.max(20.0));
                if galley.rows.len() <= 1 {
                    let end = x + galley.size().x;
                    painter.galley(pos2(x, tl.y - 16.0), galley, col);
                    status_badge(painter, n.dev_status, pos2(end + 6.0, tl.y - 9.0));
                }
            }
        }
    }
    // Prototype connections.
    crate::proto_canvas::draw(app, painter);
    // Hover outline.
    if let Some(h) = ds.hover
        && !sel.contains(&h)
        && app.session.tool.tool == ToolId::Move
        && app.session.prefs.highlight_on_hover
    {
        let col =
            if matches!(doc.node(h).map(|n| &n.kind), Some(NodeKind::Component { .. } | NodeKind::Instance { .. } | NodeKind::ComponentSet { .. })) {
                COMPONENT
            } else {
                ACCENT
            };
        match outline_shape(app, &doc, h) {
            Some(lines) => {
                for l in lines {
                    painter.add(Shape::line(l, stroke(col, 1.5)));
                }
            }
            None => {
                if let Some(q) = node_quad(app, &doc, h) {
                    quad(painter, q, stroke(col, 1.5));
                }
            }
        }
    }
    // Drop target / insertion indicator.
    let ov = &app.session.tool.overlay;
    if let Some(d) = ov.drop_target
        && let Some(q) = node_quad(app, &doc, d)
    {
        quad(painter, q, stroke(ACCENT, 1.0));
    }
    if let Some([x0, y0, x1, y1]) = ov.insert_line {
        painter.line_segment([app.to_screen(x0, y0), app.to_screen(x1, y1)], stroke(ACCENT, 2.0));
    }
    if let Some([x0, y0, x1, y1]) = ov.drop_cell {
        let r = Rect::from_two_pos(app.to_screen(x0, y0), app.to_screen(x1, y1));
        painter.rect_filled(r, CornerRadius::same(2), ACCENT.gamma_multiply(0.12));
        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        painter.add(Shape::dashed_line(&pts, stroke(ACCENT, 1.5), 4.0, 3.0));
    }
    // Vector edit mode.
    if let Some(ve) = ds.vector_edit.clone()
        && let Some(o) = figcraft_engine::vectoredit::overlay(&doc, &ve)
    {
        vector_overlay(app, painter, &o);
    }
    // Selection.
    let editing = ds.text_edit.clone();
    if !sel.is_empty() && editing.is_none() && ds.vector_edit.is_none() {
        let any_component = sel.iter().any(|i| {
            matches!(doc.node(*i).map(|n| &n.kind), Some(NodeKind::Component { .. } | NodeKind::Instance { .. } | NodeKind::ComponentSet { .. }))
        });
        let col = if any_component { COMPONENT } else { ACCENT };
        for id in &sel {
            match outline_shape(app, &doc, *id) {
                Some(lines) => {
                    for l in lines {
                        painter.add(Shape::line(l, stroke(col, 1.0)));
                    }
                }
                None => {
                    if let Some(q) = node_quad(app, &doc, *id) {
                        quad(painter, q, stroke(col, 1.0));
                    }
                }
            }
        }
        // The shape builder reshapes by regions: no transform box.
        if app.session.tool.tool != ToolId::ShapeBuilder
            && let Some(b) = selection_box(&doc, &sel)
        {
            let q = b.map(|p| app.to_screen(p.x, p.y));
            if sel.len() > 1 {
                quad(painter, q, stroke(col, 1.0));
            }
            let is_line = sel.len() == 1 && matches!(sel.first().and_then(|i| doc.node(*i)).map(|n| &n.kind), Some(NodeKind::Line));
            for (i, c) in q.iter().enumerate() {
                if is_line && i >= 2 {
                    continue;
                }
                let r = Rect::from_center_size(*c, vec2(8.0, 8.0));
                painter.rect_filled(r, CornerRadius::same(1), Color32::WHITE);
                painter.rect_stroke(r, CornerRadius::same(1), stroke(col, 1.0), StrokeKind::Inside);
            }
            // Size label under the box.
            if app.session.prefs.show_dimensions
                && let Some((_, w, h)) = selection_frame(&doc, &sel)
            {
                let label = if is_line { fmt(w) } else { format!("{} × {}", fmt(w), fmt(h)) };
                let bottom = q.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                let cx = q.iter().map(|p| p.x).sum::<f32>() / 4.0;
                let galley = painter.layout_no_wrap(label, semibold(11.0), Color32::WHITE);
                let r = Rect::from_center_size(pos2(cx, bottom + 8.0 + galley.size().y / 2.0 + 3.0), galley.size() + vec2(8.0, 4.0));
                painter.rect_filled(r, CornerRadius::same(3), col);
                painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::WHITE);
            }
        }
    }
    if app.session.tool.tool == ToolId::ShapeBuilder
        && let Some(arr) = figcraft_engine::shapebuilder::current(&app.session)
    {
        builder_overlay(app, painter, t, &arr);
    }
    // Text editing: caret and selection, or the IME composition underlined at the caret.
    let preedit = app.preedit_view();
    let edoc: &Document = preedit.map(|p| &*p.doc).unwrap_or(&doc);
    if let Some(te) = editing
        && let Some(n) = edoc.node(te.id)
        && let Some((layout, dy)) = figcraft_text::layout_node(n)
    {
        let w = edoc.world_transform(te.id);
        let to = |x: f64, y: f64| {
            let p = w * Point::new(x, y + dy);
            app.to_screen(p.x, p.y)
        };
        if let Some(q) = node_quad(app, edoc, te.id) {
            quad(painter, q, stroke(ACCENT, 1.0));
        }
        let (a, b) = (te.caret.min(te.anchor), te.caret.max(te.anchor));
        if a != b && preedit.is_none() {
            for r in layout.selection_rects(a, b) {
                let qq = [to(r.x0, r.y0), to(r.x1, r.y0), to(r.x1, r.y1), to(r.x0, r.y1)];
                painter.add(Shape::convex_polygon(qq.to_vec(), ACCENT.gamma_multiply(0.3), Stroke::NONE));
            }
        }
        if let Some(p) = preedit {
            for r in layout.selection_rects(p.range.0, p.range.1) {
                // Underline just below the baseline, across the composed chars only.
                let Some(line) = layout.lines.iter().find(|l| (l.top - r.y0).abs() < 1e-6) else { continue };
                let x1 = layout
                    .glyphs
                    .get(line.glyph_start..line.glyph_end)
                    .unwrap_or(&[])
                    .iter()
                    .filter(|g| g.char_end > g.char_start && g.char_start >= p.range.0 && g.char_end <= p.range.1)
                    .map(|g| g.x + g.advance)
                    .fold(r.x0, f64::max);
                let y = line.baseline + (line.top + line.height - line.baseline) * 0.35;
                painter.line_segment([to(r.x0, y), to(x1, y)], stroke(t.text, 1.0));
            }
        }
        let caret = preedit.map(|p| p.cursor).unwrap_or(te.caret);
        let (cx, li) = layout.caret(caret);
        if let Some(line) = layout.lines.get(li).or(layout.lines.last()) {
            let blink = (painter.ctx().input(|i| i.time) * 2.0) as i64 % 2 == 0;
            if blink || a != b {
                painter.line_segment([to(cx, line.top), to(cx, line.top + line.height)], stroke(t.text, 1.0));
            }
            painter.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        } else {
            let p = to(0.0, 0.0);
            painter.line_segment([p, p + vec2(0.0, 14.0 * zoom as f32)], stroke(t.text, 1.0));
        }
    }
    // Smart guides and measurements.
    for g in &ov.guides {
        let (a, b) = (app.to_screen(g.x0, g.y0), app.to_screen(g.x1, g.y1));
        painter.line_segment([a, b], stroke(t.guide, 1.0));
        if let Some(l) = &g.label {
            let galley = painter.layout_no_wrap(l.clone(), semibold(10.0), Color32::WHITE);
            let mid = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let r = Rect::from_center_size(mid, galley.size() + vec2(8.0, 4.0));
            painter.rect_filled(r, CornerRadius::same(3), t.guide);
            painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::WHITE);
        }
    }
    // Marquee.
    if let Some([x0, y0, x1, y1]) = ov.marquee {
        let r = Rect::from_two_pos(app.to_screen(x0, y0), app.to_screen(x1, y1));
        painter.rect_filled(r, CornerRadius::ZERO, ACCENT.gamma_multiply(0.1));
        painter.rect_stroke(r, CornerRadius::ZERO, stroke(ACCENT, 1.0), StrokeKind::Inside);
    }
    // Pen path.
    if !ov.pen.is_empty() {
        let pts: Vec<Pos2> = ov.pen.iter().map(|[x, y]| app.to_screen(*x, *y)).collect();
        if app.session.tool.tool == ToolId::Pencil {
            painter.add(Shape::line(pts, stroke(t.text, 2.0)));
        } else {
            for p in &pts {
                painter.circle_filled(*p, 3.5, Color32::WHITE);
                painter.circle_stroke(*p, 3.5, stroke(ACCENT, 1.0));
            }
            if let Some([x0, y0, x1, y1]) = ov.rubber {
                painter.line_segment([app.to_screen(x0, y0), app.to_screen(x1, y1)], stroke(ACCENT, 1.0));
            }
        }
    }
    // Eraser brush (vector edit mode).
    if let Some([x, y, r]) = ov.brush {
        let c = app.to_screen(x, y);
        let rs = (r * app.session.ds().map(|d| d.view.zoom).unwrap_or(1.0)) as f32;
        painter.circle_stroke(c, rs, stroke(Color32::WHITE, 3.0));
        painter.circle_stroke(c, rs, stroke(ACCENT, 1.5));
    }
    // Lasso (vector edit mode).
    if ov.lasso.len() > 1 {
        let mut pts: Vec<Pos2> = ov.lasso.iter().map(|[x, y]| app.to_screen(*x, *y)).collect();
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        painter.add(Shape::dashed_line(&pts, stroke(ACCENT, 1.0), 4.0, 3.0));
    }
    // Guides (under the rulers).
    crate::guides::draw(app, painter);
    // Rulers.
    if view.rulers {
        rulers(app, painter, t, &doc, &sel);
    }
}

/// Vector edit mode: the network's segments, handles and points, the hovered part, the closed
/// area under the paint bucket and the pen's rubber band.
fn vector_overlay(app: &FigApp, painter: &egui::Painter, o: &figcraft_engine::vectoredit::EditOverlay) {
    use figcraft_engine::vectoredit::VHit;
    let ov = &app.session.tool.overlay;
    let sp = |p: &[f64; 2]| app.to_screen(p[0], p[1]);
    // Paint bucket target: hatched like a fill to come.
    if !ov.face.is_empty() {
        let polys: Vec<Vec<Pos2>> = ov.face.iter().map(|poly| poly.iter().map(sp).collect()).collect();
        hatched(painter, &polys, ACCENT);
    }
    let hover_seg = match ov.vector_hover {
        Some(VHit::Segment { seg, .. }) => Some(seg),
        _ => None,
    };
    for (i, seg) in o.segments.iter().enumerate() {
        let [a, b, c, d] = [sp(&seg[0]), sp(&seg[1]), sp(&seg[2]), sp(&seg[3])];
        let w = if o.selected_segments.contains(&i) {
            3.0
        } else if hover_seg == Some(i) {
            2.5
        } else {
            1.5
        };
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke([a, b, c, d], false, Color32::TRANSPARENT, stroke(ACCENT, w)));
    }
    for h in &o.handles {
        let (v, e) = (sp(&h.vertex), sp(&h.handle));
        painter.line_segment([v, e], stroke(ACCENT, 1.0));
        let hovered = ov.vector_hover == Some(VHit::Handle { seg: h.seg, start: h.start });
        let r = if hovered { 4.5 } else { 3.5 };
        painter.circle_filled(e, r, Color32::WHITE);
        painter.circle_stroke(e, r, stroke(ACCENT, 1.0));
    }
    for (i, v) in o.vertices.iter().enumerate() {
        let p = sp(v);
        let hovered = matches!(ov.vector_hover, Some(VHit::Vertex { index }) if index == i);
        let size = if hovered { 9.0 } else { 7.0 };
        let r = Rect::from_center_size(p, vec2(size, size));
        let on = o.selected_vertices.contains(&i);
        painter.rect_filled(r, CornerRadius::same(5), if on { ACCENT } else { Color32::WHITE });
        painter.rect_stroke(r, CornerRadius::same(5), stroke(ACCENT, 1.0), StrokeKind::Inside);
        if o.anchor == Some(i) {
            painter.circle_stroke(p, 7.5, stroke(ACCENT, 1.0));
        }
    }
    if let Some([x0, y0, x1, y1]) = ov.rubber {
        painter.add(Shape::dashed_line(&[app.to_screen(x0, y0), app.to_screen(x1, y1)], stroke(ACCENT, 1.0), 4.0, 3.0));
    }
}

/// Shape builder: every region's edges, and the hovered or collected regions hatched (in the
/// danger colour when releasing removes them).
fn builder_overlay(app: &FigApp, painter: &egui::Painter, t: &Tokens, arr: &figcraft_engine::shapebuilder::Arrangement) {
    let ov = &app.session.tool.overlay;
    let screen = |r: &figcraft_engine::shapebuilder::Region| -> Vec<Vec<Pos2>> {
        r.polys.iter().map(|poly| poly.iter().map(|p| app.to_screen(p.x, p.y)).collect()).collect()
    };
    // Edges read on any fill: a light halo under a thin accent line.
    for r in &arr.regions {
        for poly in screen(r) {
            painter.add(Shape::closed_line(poly.clone(), stroke(Color32::from_white_alpha(150), 2.5)));
            painter.add(Shape::closed_line(poly, stroke(ACCENT, 1.0)));
        }
    }
    let col = if ov.builder_remove { t.danger } else { ACCENT };
    for r in ov.builder_regions.iter().filter_map(|i| arr.regions.get(*i)) {
        hatched(painter, &screen(r), col);
    }
}

/// Polygons hatched and outlined in `col` (an area about to be filled, merged or removed), each
/// line on a light halo so it reads on any fill, even one in `col`.
fn hatched(painter: &egui::Painter, polys: &[Vec<Pos2>], col: Color32) {
    let halo = Color32::from_white_alpha(200);
    for l in hatch(polys, 6.0) {
        painter.line_segment(l, stroke(halo, 3.0));
        painter.line_segment(l, stroke(col, 1.25));
    }
    for poly in polys {
        painter.add(Shape::closed_line(poly.clone(), stroke(halo, 4.0)));
        painter.add(Shape::closed_line(poly.clone(), stroke(col, 2.0)));
    }
}

/// 45° hatch lines `gap` apart inside polygons (even-odd), in screen space.
fn hatch(polys: &[Vec<Pos2>], gap: f32) -> Vec<[Pos2; 2]> {
    // Rotate so the hatch is horizontal, scan, rotate back.
    let rot = |p: Pos2| pos2((p.x + p.y) * std::f32::consts::FRAC_1_SQRT_2, (p.y - p.x) * std::f32::consts::FRAC_1_SQRT_2);
    let back = |x: f32, y: f32| pos2((x - y) * std::f32::consts::FRAC_1_SQRT_2, (x + y) * std::f32::consts::FRAC_1_SQRT_2);
    let edges: Vec<(Pos2, Pos2)> = polys
        .iter()
        .flat_map(|poly| {
            let pts: Vec<Pos2> = poly.iter().map(|p| rot(*p)).collect();
            let n = pts.len();
            (0..n).filter_map(move |k| Some((*pts.get(k)?, *pts.get((k + 1) % n)?)))
        })
        .collect();
    let (y0, y1) = edges.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (a, b)| (lo.min(a.y).min(b.y), hi.max(a.y).max(b.y)));
    let mut out = Vec::new();
    if !(y0.is_finite() && y1.is_finite()) || (y1 - y0) / gap > 4000.0 {
        return out;
    }
    let mut y = (y0 / gap).ceil() * gap;
    while y < y1 {
        let mut xs: Vec<f32> = edges
            .iter()
            .filter(|(a, b)| (a.y <= y && y < b.y) || (b.y <= y && y < a.y))
            .map(|(a, b)| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y))
            .collect();
        xs.sort_by(f32::total_cmp);
        for pair in xs.chunks_exact(2) {
            if let [a, b] = pair {
                out.push([back(*a, y), back(*b, y)]);
            }
        }
        y += gap;
    }
    out
}

/// A frame's or section's handoff status as a small pill left-centred on `at`.
fn status_badge(painter: &egui::Painter, status: figcraft_doc::DevStatus, at: Pos2) {
    let (label, fill) = match status {
        figcraft_doc::DevStatus::None => return,
        figcraft_doc::DevStatus::ReadyForDev => ("Ready for dev", Color32::from_rgb(0x14, 0xAE, 0x5C)),
        figcraft_doc::DevStatus::Completed => ("Completed", Color32::from_rgb(0x6E, 0x6E, 0x73)),
    };
    let galley = painter.layout_no_wrap(label.to_string(), semibold(10.0), Color32::WHITE);
    let r = Rect::from_min_size(pos2(at.x, at.y - galley.size().y / 2.0 - 2.0), galley.size() + vec2(10.0, 4.0));
    painter.rect_filled(r, CornerRadius::same(4), fill);
    painter.galley(r.min + vec2(5.0, 2.0), galley, Color32::WHITE);
}

fn rulers(app: &FigApp, painter: &egui::Painter, t: &Tokens, doc: &Document, sel: &[NodeId]) {
    let r = app.canvas.rect;
    let size = 20.0;
    painter.rect_filled(Rect::from_min_size(r.min, vec2(r.width(), size)), CornerRadius::ZERO, t.panel);
    painter.rect_filled(Rect::from_min_size(r.min, vec2(size, r.height())), CornerRadius::ZERO, t.panel);
    let zoom = app.session.ds().map(|d| d.view.zoom).unwrap_or(1.0);
    let step =
        [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0].into_iter().find(|s| s * zoom >= 50.0).unwrap_or(10000.0);
    let p0 = app.to_page(r.min);
    let p1 = app.to_page(r.max);
    // Highlight the selection's span.
    if let Some(b) = doc.bounds_of(sel) {
        let (a, bb) = (app.to_screen(b.x0, b.y0), app.to_screen(b.x1, b.y1));
        painter.rect_filled(Rect::from_min_max(pos2(a.x, r.min.y), pos2(bb.x, r.min.y + size)), CornerRadius::ZERO, ACCENT.gamma_multiply(0.15));
        painter.rect_filled(Rect::from_min_max(pos2(r.min.x, a.y), pos2(r.min.x + size, bb.y)), CornerRadius::ZERO, ACCENT.gamma_multiply(0.15));
    }
    let mut x = (p0.x / step).floor() * step;
    while x <= p1.x {
        let s = app.to_screen(x, 0.0).x;
        painter.line_segment([pos2(s, r.min.y + size - 6.0), pos2(s, r.min.y + size)], stroke(t.text_tertiary, 1.0));
        painter.text(pos2(s + 2.0, r.min.y + 3.0), Align2::LEFT_TOP, fmt(x), regular(9.0), t.text_secondary);
        x += step;
    }
    let mut y = (p0.y / step).floor() * step;
    while y <= p1.y {
        let s = app.to_screen(0.0, y).y;
        painter.line_segment([pos2(r.min.x + size - 6.0, s), pos2(r.min.x + size, s)], stroke(t.text_tertiary, 1.0));
        painter.text(pos2(r.min.x + 3.0, s + 2.0), Align2::LEFT_TOP, fmt(y), regular(9.0), t.text_secondary);
        y += step;
    }
    painter.line_segment([pos2(r.min.x, r.min.y + size), pos2(r.max.x, r.min.y + size)], stroke(t.border, 1.0));
    painter.line_segment([pos2(r.min.x + size, r.min.y), pos2(r.min.x + size, r.max.y)], stroke(t.border, 1.0));
}

pub fn fmt(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r}") }
}
