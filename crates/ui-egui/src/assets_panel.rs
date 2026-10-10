//! The Assets tab: components as thumbnail tiles (search, click to place, drag onto the canvas to
//! drop an instance into the frame or auto layout slot under the pointer) and styles (click to
//! apply to the selection), for this file and for every enabled library, with a "Library updates
//! available" banner. The libraries view publishes this file as a library and enables library
//! files.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use figcraft_doc::{Document, NodeId, NodeKind, Style, StyleKind};
use figcraft_engine::library;
use serde_json::{Value, json};

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{COMPONENT, Tokens, regular, semibold};

const TILE_LABEL: f32 = 22.0;
/// Re-check thumbnails at most this often (seconds) while the document changes.
const THUMB_REFRESH: f64 = 0.4;
const MAX_THUMBS: usize = 200;
/// Look for changed library files this often (seconds) while the panel shows.
const LIBRARY_REFRESH: f64 = 2.0;
const PAD: f32 = 16.0;

/// Where an asset comes from: this file or an enabled library (its id).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Source {
    Local,
    Library(String),
}

#[derive(Clone)]
struct Tile {
    id: NodeId,
    name: String,
    set: bool,
}

#[derive(Clone)]
struct StyleRow {
    id: String,
    name: String,
    kind: StyleKind,
}

impl StyleRow {
    fn of(s: &Style) -> Self {
        Self { id: s.id.clone(), name: s.name.clone(), kind: s.kind.clone() }
    }
}

/// An enabled library as listed: its snapshot (when it could be read) and matching assets.
#[derive(Clone)]
struct Section {
    id: String,
    name: String,
    doc: Option<Arc<Document>>,
    error: Option<String>,
    tiles: Vec<Tile>,
    styles: Vec<StyleRow>,
}

/// What the panel lists; rebuilt only when the document, the search or a library changes.
struct Listing {
    key: (u64, String, u64),
    local: Vec<Tile>,
    local_styles: Vec<StyleRow>,
    libraries: Vec<Section>,
    /// Library components and styles in this file with a newer published version.
    updates: usize,
}

/// Rendered thumbnails keyed by source and component, with the fingerprint they were made from.
#[derive(Clone, Default)]
struct Thumbs {
    map: HashMap<(Source, NodeId), (u64, Option<egui::TextureHandle>)>,
    checked_at: f64,
    key: (u64, u64),
}

/// A component being dragged out of the panel.
#[derive(Clone)]
struct AssetDrag {
    src: Source,
    component: NodeId,
    set: bool,
}

/// What the user did in the list this frame.
enum Act {
    Place(Source, NodeId),
    Drag(AssetDrag),
    Style(Source, String),
    Toggle(String),
}

fn drag_id() -> Id {
    Id::new("asset_drag")
}

/// Content fingerprint of a layer subtree (re-render its thumbnail when it changes).
fn fingerprint(doc: &Document, id: NodeId) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut stack = vec![id];
    let mut n = 0;
    while let Some(i) = stack.pop() {
        n += 1;
        if n > 5000 {
            break;
        }
        let Some(node) = doc.node(i) else { continue };
        if let Ok(bytes) = serde_json::to_vec(node) {
            bytes.hash(&mut h);
        }
        stack.extend(node.children.iter().copied());
    }
    h.finish()
}

pub(crate) fn thumbnail(ctx: &egui::Context, doc: &Document, id: NodeId, px: f32) -> Option<egui::TextureHandle> {
    let b = figcraft_render::export_bounds(doc, id)?;
    let side = b.width().max(b.height());
    if !(side.is_finite() && side > 0.0) {
        return None;
    }
    let scale = (f64::from(px) / side).min(2.0);
    let pix = figcraft_render::render_node(doc, id, scale)?;
    let img = crate::render_worker::screen_image(&pix, doc.color_profile);
    Some(ctx.load_texture(format!("asset-thumb-{}", id.0), img, egui::TextureOptions::LINEAR))
}

fn matches(name: &str, q: &str) -> bool {
    q.is_empty() || name.to_lowercase().contains(q)
}

fn sorted(mut tiles: Vec<Tile>) -> Vec<Tile> {
    tiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    tiles
}

/// A fingerprint of what the session read for the enabled libraries.
fn libraries_key(app: &FigApp, doc: &Document) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for l in &doc.libraries.enabled {
        l.id.hash(&mut h);
        match app.session.libraries.cached(&l.path) {
            Some(Ok(lib)) => (Arc::as_ptr(lib) as usize).hash(&mut h),
            Some(Err(e)) => e.hash(&mut h),
            None => 0u8.hash(&mut h),
        }
    }
    h.finish()
}

fn build_listing(app: &FigApp, doc: &Document, q: &str, key: (u64, String, u64)) -> Listing {
    let local = library::local_components(doc)
        .into_iter()
        .filter_map(|id| doc.node(id).map(|n| Tile { id, name: n.name.clone(), set: matches!(n.kind, NodeKind::ComponentSet { .. }) }))
        .filter(|t| matches(&t.name, q))
        .collect();
    let local_styles = library::local_styles(doc).filter(|s| matches(&s.name, q)).map(StyleRow::of).collect();
    let mut updates = 0;
    let mut libraries = Vec::new();
    for l in &doc.libraries.enabled {
        let mut sec = Section { id: l.id.clone(), name: l.name.clone(), doc: None, error: None, tiles: Vec::new(), styles: Vec::new() };
        match app.session.libraries.cached(&l.path) {
            Some(Ok(lib)) if lib.manifest.id == l.id => {
                updates += library::pending(doc, lib).count();
                let m = &lib.manifest;
                sec.name = m.name.clone();
                sec.tiles = m
                    .components
                    .iter()
                    .filter(|a| a.published && matches(&a.name, q))
                    .map(|a| Tile { id: a.id, name: a.name.clone(), set: a.set })
                    .collect();
                sec.styles =
                    lib.doc.styles.iter().filter(|s| m.style(&s.id).is_some_and(|x| x.published) && matches(&s.name, q)).map(StyleRow::of).collect();
                sec.doc = Some(lib.doc.clone());
            }
            Some(Ok(lib)) => sec.error = Some(format!("The file now holds another library (\"{}\")", lib.manifest.name)),
            Some(Err(e)) => sec.error = Some(e.to_string()),
            None => {}
        }
        // While searching, libraries without a match stay out of the way.
        if q.is_empty() || !sec.tiles.is_empty() || !sec.styles.is_empty() {
            sec.tiles = sorted(sec.tiles);
            libraries.push(sec);
        }
    }
    Listing { key, local: sorted(local), local_styles, libraries, updates }
}

/// Re-read enabled library files that changed on disk (throttled).
fn refresh_libraries(app: &mut FigApp, ui: &Ui) {
    let Ok(doc) = app.session.doc() else { return };
    if doc.libraries.enabled.is_empty() {
        return;
    }
    let paths: Vec<String> = doc.libraries.enabled.iter().map(|l| l.path.clone()).collect();
    let id = Id::new("asset_libraries_checked");
    let now = ui.input(|i| i.time);
    let last: Option<f64> = ui.data(|d| d.get_temp(id));
    if last.is_none_or(|l| now - l >= LIBRARY_REFRESH) {
        for p in &paths {
            let _ = app.session.libraries.load(p);
        }
        ui.data_mut(|d| d.insert_temp(id, now));
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(LIBRARY_REFRESH));
}

/// The search field with the libraries button; returns the lowercase query.
fn search_row(app: &mut FigApp, ui: &mut Ui, w: f32) -> String {
    let t = Tokens::get(ui.ctx());
    let filter_id = Id::new("asset_filter");
    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    let (fr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let field = Rect::from_min_max(pos2(fr.min.x + 12.0, fr.min.y + 8.0), pos2(fr.max.x - 44.0, fr.max.y - 4.0));
    ui.painter().rect_filled(field, CornerRadius::same(5), t.field);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(field.min.x + 12.0, field.center().y), vec2(12.0, 12.0)), Icon::Search, t.icon_secondary);
    let te_rect = Rect::from_min_max(pos2(field.min.x + 24.0, field.center().y - 7.0), pos2(field.max.x - 6.0, field.center().y + 7.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(te_rect));
    let r = child.add(
        egui::TextEdit::singleline(&mut filter)
            .id(Id::new("asset_filter_field"))
            .frame(egui::Frame::NONE)
            .font(regular(11.0))
            .hint_text("Search assets")
            .desired_width(te_rect.width()),
    );
    if r.has_focus() {
        ui.painter().rect_stroke(field, CornerRadius::same(5), Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            filter.clear();
        }
    }
    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    let br = Rect::from_center_size(pos2(fr.max.x - 26.0, field.center().y), vec2(24.0, 24.0));
    let bresp = ui.interact(br, Id::new("asset_libraries_btn"), Sense::click()).on_hover_text("Libraries");
    if bresp.hovered() {
        ui.painter().rect_filled(br, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), br.shrink(4.0), Icon::Library, t.icon);
    if bresp.clicked() {
        app.ui.libraries = true;
    }
    filter.trim().to_lowercase()
}

/// "Library updates available" with a button that pulls them in.
fn updates_banner(app: &mut FigApp, ui: &mut Ui, w: f32, count: usize) {
    let t = Tokens::get(ui.ctx());
    let (row, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
    let r = Rect::from_min_max(pos2(row.min.x + 12.0, row.min.y + 4.0), pos2(row.max.x - 12.0, row.max.y - 4.0));
    ui.painter().rect_filled(r, CornerRadius::same(6), t.selected_row);
    let what = if count == 1 {
        "1 component or style has a newer version".to_string()
    } else {
        format!("{count} components and styles have newer versions")
    };
    ui.interact(r, Id::new("asset_updates"), Sense::hover()).on_hover_text(what);
    ui.painter().text(pos2(r.min.x + 10.0, r.center().y), Align2::LEFT_CENTER, "Library updates available", semibold(11.0), t.text);
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(r.max.x - 70.0, r.min.y + 6.0), pos2(r.max.x - 6.0, r.max.y))));
    if crate::widgets::button(&mut child, "Update", true, true).clicked()
        && let Some(v) = app.run("library.acceptUpdates", json!({}))
    {
        let (c, s) = (v["components"].as_u64().unwrap_or(0), v["styles"].as_u64().unwrap_or(0));
        app.toast(format!("Updated {c} component{} and {s} style{}", if c == 1 { "" } else { "s" }, if s == 1 { "" } else { "s" }));
    }
}

/// A section title row.
fn title(ui: &mut Ui, w: f32, text: &str) {
    let t = Tokens::get(ui.ctx());
    let (hr, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    ui.painter().text(pos2(hr.min.x + PAD, hr.center().y), Align2::LEFT_CENTER, text, semibold(11.0), t.text);
}

/// A library's title row with its open/closed chevron; returns whether it was clicked.
fn library_title(ui: &mut Ui, w: f32, name: &str, open: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (hr, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let chevron = Rect::from_center_size(pos2(hr.min.x + PAD + 5.0, hr.center().y), vec2(12.0, 12.0));
    icons::paint(ui.painter(), chevron, if open { Icon::Chevron } else { Icon::ChevronRight }, t.icon_secondary);
    crate::bidi::paint_name(
        ui.painter(),
        pos2(chevron.max.x + 6.0, hr.center().y),
        name,
        semibold(11.0),
        t.text,
        hr.max.x - chevron.max.x - 6.0 - PAD,
    );
    resp.on_hover_text(if open { "Library: click to collapse" } else { "Library: click to expand" }).clicked()
}

/// Secondary text, wrapped to the panel.
fn hint(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let galley = ui.painter().layout(text.to_string(), regular(11.0), t.text_secondary, (w - PAD * 2.0).max(40.0));
    let (r, _) = ui.allocate_exact_size(vec2(w, galley.size().y + 12.0), Sense::hover());
    ui.painter().galley(pos2(r.min.x + PAD, r.min.y + 2.0), galley, t.text_secondary);
}

/// Two-column thumbnail tiles.
fn tiles(ui: &mut Ui, w: f32, src: &Source, list: &[Tile], thumbs: &Thumbs, act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    let gap = 8.0;
    let tile = ((w - PAD * 2.0 - gap) / 2.0).max(40.0);
    for pair in list.chunks(2) {
        let (row, _) = ui.allocate_exact_size(vec2(w, tile + TILE_LABEL + gap), Sense::hover());
        for (k, item) in pair.iter().enumerate() {
            let x = row.min.x + PAD + k as f32 * (tile + gap);
            let r = Rect::from_min_size(pos2(x, row.min.y), vec2(tile, tile));
            let resp = ui.interact(
                r.union(Rect::from_min_size(r.left_bottom(), vec2(tile, TILE_LABEL))),
                Id::new(("asset", src, item.id.0)),
                Sense::click_and_drag(),
            );
            ui.painter().rect_filled(r, CornerRadius::same(6), t.field);
            if resp.hovered() {
                ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, COMPONENT), egui::StrokeKind::Inside);
            }
            match thumbs.map.get(&(src.clone(), item.id)).and_then(|(_, tex)| tex.as_ref()) {
                Some(tex) => {
                    let size = tex.size_vec2();
                    // Fit the tile, never past 1:1 physical pixels (small components stay crisp).
                    let inner = r.shrink(14.0);
                    let fit = (inner.width() / size.x).min(inner.height() / size.y).min(1.0 / ui.ctx().pixels_per_point());
                    let img = Rect::from_center_size(r.center(), size * fit);
                    ui.painter().image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                None => icons::paint(
                    ui.painter(),
                    Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
                    if item.set { Icon::ComponentSet } else { Icon::Component },
                    COMPONENT,
                ),
            }
            let label = Rect::from_min_size(pos2(x, r.max.y + 2.0), vec2(tile, TILE_LABEL - 4.0));
            crate::bidi::paint_name(ui.painter(), pos2(x, label.center().y), &item.name, regular(11.0), t.text, tile);
            let resp = resp.on_hover_text(format!("{}\nDrag onto the canvas, or click to place in the middle of the view", item.name));
            if resp.drag_started() {
                *act = Some(Act::Drag(AssetDrag { src: src.clone(), component: item.id, set: item.set }));
            }
            if resp.clicked() {
                *act = Some(Act::Place(src.clone(), item.id));
            }
        }
    }
}

/// Style rows (a swatch for paint styles); a click applies the style to the selection.
fn style_rows(ui: &mut Ui, w: f32, src: &Source, list: &[StyleRow], act: &mut Option<Act>) {
    let t = Tokens::get(ui.ctx());
    for s in list {
        let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(8.0, 1.0)), CornerRadius::same(5), t.hover);
        }
        let icon_rect = Rect::from_center_size(pos2(r.min.x + 22.0, r.center().y), vec2(14.0, 14.0));
        let icon = match &s.kind {
            StyleKind::Paint { paints } => {
                if let Some(p) = paints.first() {
                    crate::widgets::paint_swatch(ui.painter(), icon_rect, p);
                }
                None
            }
            StyleKind::Text { .. } => Some(Icon::Text),
            StyleKind::Effect { .. } => Some(Icon::Effects),
            StyleKind::Grid { .. } => Some(Icon::Grid),
        };
        if let Some(i) = icon {
            icons::paint(ui.painter(), icon_rect, i, t.icon_secondary);
        }
        crate::bidi::paint_name(ui.painter(), pos2(r.min.x + 40.0, r.center().y), &s.name, regular(11.0), t.text, w - 40.0 - PAD);
        if resp.on_hover_text(format!("{}\nClick to apply to the selection", s.name)).clicked() {
            *act = Some(Act::Style(src.clone(), s.id.clone()));
        }
    }
}

/// Render missing thumbnails now; re-check changed ones (throttled while the document changes).
fn update_thumbs(ui: &Ui, w: f32, doc: &Document, revision: u64, listing: &Listing, thumbs: &mut Thumbs) {
    let mut live: Vec<(Source, NodeId, &Document)> = listing.local.iter().map(|t| (Source::Local, t.id, doc)).collect();
    for sec in &listing.libraries {
        if let Some(d) = &sec.doc {
            live.extend(sec.tiles.iter().map(|t| (Source::Library(sec.id.clone()), t.id, d.as_ref())));
        }
    }
    live.truncate(MAX_THUMBS);
    let now = ui.input(|i| i.time);
    let key = (revision, listing.key.2);
    let changed = thumbs.key != key;
    let due = changed && now - thumbs.checked_at >= THUMB_REFRESH;
    let missing = live.iter().any(|(s, id, _)| !thumbs.map.contains_key(&(s.clone(), *id)));
    if !(due || missing) {
        if changed {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(THUMB_REFRESH));
        }
        return;
    }
    let px = ((w - 40.0) / 2.0).max(32.0) * ui.ctx().pixels_per_point();
    if due {
        let keep: HashSet<(Source, NodeId)> = live.iter().map(|(s, id, _)| (s.clone(), *id)).collect();
        thumbs.map.retain(|k, _| keep.contains(k));
        thumbs.checked_at = now;
        thumbs.key = key;
    }
    for (src, id, d) in live {
        let k = (src, id);
        if !due && thumbs.map.contains_key(&k) {
            continue;
        }
        // Library snapshots only change when the file is read again.
        let f = match &k.0 {
            Source::Local => fingerprint(d, id),
            Source::Library(_) => std::ptr::from_ref(d) as usize as u64,
        };
        if thumbs.map.get(&k).is_none_or(|(old, _)| *old != f) {
            thumbs.map.insert(k, (f, thumbnail(ui.ctx(), d, id, px)));
        }
    }
}

pub fn show(app: &mut FigApp, ui: &mut Ui) {
    refresh_libraries(app, ui);
    if app.ui.libraries {
        libraries_view(app, ui);
        return;
    }
    let w = ui.available_width();
    let Ok(ds) = app.session.ds() else { return };
    let doc = ds.doc.clone();
    let revision = ds.revision;
    let q = search_row(app, ui, w);
    let key = (revision, q.clone(), libraries_key(app, &doc));
    let listing_id = Id::new("asset_listing");
    let listing = match ui.data(|d| d.get_temp::<Arc<Listing>>(listing_id)) {
        Some(l) if l.key == key => l,
        _ => {
            let l = Arc::new(build_listing(app, &doc, &q, key));
            ui.data_mut(|d| d.insert_temp(listing_id, l.clone()));
            l
        }
    };
    if listing.updates > 0 {
        updates_banner(app, ui, w, listing.updates);
    }
    let mut thumbs: Thumbs = ui.data(|d| d.get_temp(Id::new("asset_thumbs"))).unwrap_or_default();
    update_thumbs(ui, w, &doc, revision, &listing, &mut thumbs);
    let collapsed_id = Id::new("asset_collapsed_libraries");
    let collapsed: HashSet<String> = ui.data(|d| d.get_temp(collapsed_id)).unwrap_or_default();
    let mut act = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        title(ui, w, "Local components");
        if listing.local.is_empty() {
            if q.is_empty() {
                let keys = crate::toolbar::pretty_shortcut(crate::shortcuts::shortcut("component.create").unwrap_or(""));
                hint(ui, &format!("No components yet. Select layers and press {keys}."));
            } else {
                hint(ui, "No matching components");
            }
        }
        tiles(ui, w, &Source::Local, &listing.local, &thumbs, &mut act);
        crate::widgets::separator(ui);
        ui.add_space(4.0);
        title(ui, w, "Local styles");
        if listing.local_styles.is_empty() {
            hint(ui, if q.is_empty() { "No styles yet." } else { "No matching styles" });
        }
        style_rows(ui, w, &Source::Local, &listing.local_styles, &mut act);
        for sec in &listing.libraries {
            crate::widgets::separator(ui);
            ui.add_space(4.0);
            let open = !q.is_empty() || !collapsed.contains(&sec.id);
            if library_title(ui, w, &sec.name, open) {
                act = Some(Act::Toggle(sec.id.clone()));
            }
            if !open {
                continue;
            }
            if let Some(e) = &sec.error {
                hint(ui, &format!("Can't read this library: {e}"));
                continue;
            }
            if sec.doc.is_none() {
                hint(ui, "Reading the library…");
                continue;
            }
            let src = Source::Library(sec.id.clone());
            tiles(ui, w, &src, &sec.tiles, &thumbs, &mut act);
            style_rows(ui, w, &src, &sec.styles, &mut act);
        }
    });
    let drag = ui.data(|d| d.get_temp::<AssetDrag>(drag_id()));
    let tex = drag.as_ref().and_then(|d| thumbs.map.get(&(d.src.clone(), d.component)).and_then(|(_, t)| t.clone()));
    ui.data_mut(|d| d.insert_temp(Id::new("asset_thumbs"), thumbs));
    match act {
        Some(Act::Drag(d)) => ui.data_mut(|m| {
            m.insert_temp(drag_id(), d);
        }),
        Some(Act::Place(src, id)) => {
            let (cx, cy) = app.session.ds().map(|d| (d.view.cx, d.view.cy)).unwrap_or((0.0, 0.0));
            place(app, &src, id, cx, cy);
        }
        Some(Act::Style(src, style)) => apply_style(app, &src, &style),
        Some(Act::Toggle(id)) => {
            let mut c = collapsed;
            if !c.remove(&id) {
                c.insert(id);
            }
            ui.data_mut(|d| d.insert_temp(collapsed_id, c));
        }
        None => {}
    }
    drag_feedback(app, ui, &doc, tex);
}

/// Place component `id` of `src` centred on page point (`x`, `y`).
fn place(app: &mut FigApp, src: &Source, id: NodeId, x: f64, y: f64) {
    match src {
        Source::Local => app.run("instance.place", json!({"component": id, "x": x, "y": y})),
        Source::Library(lib) => app.run("library.place", json!({"library": lib, "component": id, "x": x, "y": y})),
    };
}

fn apply_style(app: &mut FigApp, src: &Source, style: &str) {
    if app.session.selection().is_empty() {
        app.toast("Select layers to apply the style to");
        return;
    }
    match src {
        Source::Local => app.run("style.apply", json!({"style": style})),
        Source::Library(lib) => app.run("library.applyStyle", json!({"library": lib, "style": style})),
    };
}

/// While a component is dragged: a ghost at the pointer and, over the canvas, the frame or auto
/// layout slot it would drop into. Releasing over the canvas places the instance.
fn drag_feedback(app: &mut FigApp, ui: &mut Ui, doc: &Document, tex: Option<egui::TextureHandle>) {
    let Some(drag) = ui.data(|d| d.get_temp::<AssetDrag>(drag_id())) else { return };
    let (pointer, released) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.any_released()));
    let over_canvas = pointer.filter(|p| app.canvas.rect.contains(*p));
    let page = app.session.ds().map(|d| d.page).ok();
    let slot = over_canvas.zip(page).map(|(p, page)| {
        let pt = app.to_page(p);
        (pt, figcraft_engine::drop::slot(doc, page, pt))
    });
    let ov = &mut app.session.tool.overlay;
    ov.drop_target = slot.as_ref().and_then(|(_, s)| (Some(s.parent) != page).then_some(s.parent));
    ov.insert_line = slot.as_ref().and_then(|(_, s)| s.line);
    if let Some(p) = pointer {
        ghost(ui, p, drag.set, tex);
    }
    if released {
        ui.data_mut(|d| d.remove::<AssetDrag>(drag_id()));
        let ov = &mut app.session.tool.overlay;
        ov.drop_target = None;
        ov.insert_line = None;
        if let Some((pt, _)) = slot {
            place(app, &drag.src, drag.component, pt.x, pt.y);
        }
    }
    ui.ctx().request_repaint();
}

fn ghost(ui: &Ui, p: Pos2, set: bool, tex: Option<egui::TextureHandle>) {
    let t = Tokens::get(ui.ctx());
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("asset_ghost")));
    let r = Rect::from_center_size(p, vec2(64.0, 64.0));
    painter.rect_filled(r, CornerRadius::same(6), t.panel.gamma_multiply(0.9));
    painter.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, COMPONENT), egui::StrokeKind::Inside);
    match tex {
        Some(tex) => {
            let size = tex.size_vec2();
            let fit = (56.0 / size.x).min(56.0 / size.y);
            painter.image(
                tex.id(),
                Rect::from_center_size(r.center(), size * fit),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE.gamma_multiply(0.85),
            );
        }
        None => icons::paint(
            &painter,
            Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
            if set { Icon::ComponentSet } else { Icon::Component },
            COMPONENT,
        ),
    }
}

/// Publish choices the user changed (`c<id>` / `s<id>` → offered) and the description.
#[derive(Clone, Default)]
struct PublishForm {
    picked: HashMap<String, bool>,
    description: Option<String>,
}

/// The libraries view: publish this file, and the library files this file uses.
fn libraries_view(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let back = Rect::from_center_size(pos2(hr.min.x + 24.0, hr.center().y), vec2(24.0, 24.0));
    let bresp = ui.interact(back, Id::new("asset_libraries_back"), Sense::click()).on_hover_text("Back to assets");
    if bresp.hovered() {
        ui.painter().rect_filled(back, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), back.shrink(5.0), Icon::ChevronLeft, t.icon);
    ui.painter().text(pos2(back.max.x + 6.0, hr.center().y), Align2::LEFT_CENTER, "Libraries", semibold(12.0), t.text);
    if bresp.clicked() {
        app.ui.libraries = false;
        return;
    }
    crate::widgets::separator(ui);
    let Ok(doc) = app.session.doc().cloned() else { return };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        publish_form(app, ui, w, &doc);
        crate::widgets::separator(ui);
        enabled_libraries(app, ui, w, &doc);
    });
}

fn publish_form(app: &mut FigApp, ui: &mut Ui, w: f32, doc: &Document) {
    let form_id = Id::new("asset_publish_form");
    let mut form: PublishForm = ui.data(|d| d.get_temp(form_id)).unwrap_or_default();
    title(ui, w, "This file");
    let m = doc.published.as_ref().map(|p| &p.manifest);
    let status = match m {
        Some(m) => format!("{} · published version {}", doc.name, m.version),
        None => format!("{} · not published", doc.name),
    };
    hint(ui, &status);
    let all = library::publishable(doc);
    if all.is_empty() {
        hint(ui, "Create components, styles or variables to publish them as a library.");
        return;
    }
    let def = library::default_selection(doc);
    let mut include: Vec<Value> = Vec::new();
    let mut check = |ui: &mut Ui, key: String, default: bool, label: &str| -> bool {
        let on = form.picked.get(&key).copied().unwrap_or(default);
        let mut clicked = false;
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            clicked = crate::widgets::checkbox(ui, on, label);
        });
        let on = if clicked { !on } else { on };
        if clicked {
            form.picked.insert(key, on);
        }
        on
    };
    if !all.components.is_empty() {
        crate::widgets::sublabels(ui, "Components", None);
        for c in &all.components {
            let name = doc.node(*c).map(|n| n.name.as_str()).unwrap_or_default();
            if check(ui, format!("c{}", c.0), def.components.contains(c), name) {
                include.push(json!(c.0));
            }
        }
    }
    if !all.styles.is_empty() {
        crate::widgets::sublabels(ui, "Styles", None);
        for s in &all.styles {
            let name = doc.style(s).map(|x| x.name.as_str()).unwrap_or_default();
            if check(ui, format!("s{s}"), def.styles.contains(s), name) {
                include.push(json!(s));
            }
        }
    }
    if !all.collections.is_empty() {
        crate::widgets::sublabels(ui, "Variable collections", None);
        for c in &all.collections {
            let name = doc.variables.collection(c).map(|x| x.name.as_str()).unwrap_or_default();
            if check(ui, format!("v{c}"), def.collections.contains(c), name) {
                include.push(json!(c));
            }
        }
    }
    ui.add_space(8.0);
    let description = form.description.clone().unwrap_or_else(|| m.map(|m| m.description.clone()).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        if let Some(d) = crate::widgets::text_field(ui, Id::new("asset_publish_description"), &description, w - PAD * 2.0, "Description (optional)") {
            form.description = Some(d);
        }
    });
    ui.add_space(8.0);
    let any = !include.is_empty();
    let mut publish = false;
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        publish = crate::widgets::button(ui, if m.is_some() { "Publish changes" } else { "Publish" }, true, any).clicked();
    });
    ui.add_space(12.0);
    if publish {
        let description = form.description.clone().unwrap_or(description);
        if let Some(r) = app.run("library.publish", json!({"description": description, "include": include})) {
            form = PublishForm::default();
            // Other files read the saved file.
            app.save();
            let saved = app.session.ds().is_ok_and(|d| d.path.is_some() && !d.is_dirty());
            let v = r["version"].as_u64().unwrap_or(0);
            app.toast(if saved {
                format!("Published version {v}")
            } else {
                format!("Published version {v}: save the file so other files can use it")
            });
        }
    }
    ui.data_mut(|d| d.insert_temp(form_id, form));
}

fn enabled_libraries(app: &mut FigApp, ui: &mut Ui, w: f32, doc: &Document) {
    let t = Tokens::get(ui.ctx());
    title(ui, w, "Libraries in this file");
    if doc.libraries.enabled.is_empty() {
        hint(ui, "No libraries yet. Add a .figcraft file that was published as a library.");
    }
    let mut remove = None;
    for l in &doc.libraries.enabled {
        let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        let status = match app.session.libraries.cached(&l.path) {
            Some(Ok(lib)) if lib.manifest.id == l.id => {
                let m = &lib.manifest;
                let n = m.components.iter().filter(|a| a.published).count();
                let s = m.styles.iter().filter(|a| a.published).count();
                let v = m.variables.iter().filter(|a| a.published).count();
                (format!("Version {} · {n} components · {s} styles · {v} variables", m.version), t.text_secondary)
            }
            Some(Ok(_)) => ("The file now holds another library".into(), t.danger),
            Some(Err(e)) => (e.to_string(), t.danger),
            None => ("Not read yet".into(), t.text_secondary),
        };
        let text_w = w - PAD - 44.0;
        let name = ui.painter().layout_no_wrap(l.name.clone(), semibold(11.0), t.text);
        let line = ui.painter().layout(status.0, regular(10.0), status.1, text_w);
        let clip = Rect::from_min_size(pos2(r.min.x + PAD, r.min.y), vec2(text_w, r.height()));
        ui.painter().with_clip_rect(clip).galley(pos2(r.min.x + PAD, r.min.y + 6.0), name, t.text);
        ui.painter().with_clip_rect(clip).galley(pos2(r.min.x + PAD, r.min.y + 23.0), line, t.text_secondary);
        ui.interact(clip, Id::new(("asset_lib_row", &l.id)), Sense::hover()).on_hover_text(&l.path);
        let x = Rect::from_center_size(pos2(r.max.x - 24.0, r.center().y), vec2(24.0, 24.0));
        let xr = ui.interact(x, Id::new(("asset_lib_remove", &l.id)), Sense::click()).on_hover_text("Remove library from this file");
        if xr.hovered() {
            ui.painter().rect_filled(x, CornerRadius::same(5), t.hover);
        }
        icons::paint(ui.painter(), x.shrink(6.0), Icon::Close, t.icon);
        if xr.clicked() {
            remove = Some(l.id.clone());
        }
    }
    ui.add_space(4.0);
    let mut add = false;
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        add = crate::widgets::button(ui, "Add library file…", false, true).clicked();
    });
    if let Some(id) = remove {
        app.run("library.remove", json!({"id": id}));
    }
    if add && let Some(p) = app.services.pick_open("Add library", &["figcraft"]) {
        let path = p.to_string_lossy().to_string();
        if let Some(r) = app.run("library.add", json!({"path": path})) {
            app.toast(format!("Added the library \"{}\"", r["name"].as_str().unwrap_or_default()));
        }
    }
}
