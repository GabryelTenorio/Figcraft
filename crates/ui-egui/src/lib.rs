//! The FigCraft egui frontend.
//!
//! A thin shell over [`figcraft_engine::Session`]: panels read engine state and act by running
//! commands; the canvas forwards pointer events to the engine's tools and draws overlays. Nothing
//! below this crate knows about egui, so the frontend can be replaced.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod assets_panel;
pub mod canvas;
pub mod colorpicker;
pub mod comments;
pub mod component_panel;
pub mod control;
pub mod credits;
pub mod dialogs;
pub mod export_panel;
pub mod frame_presets;
pub mod guides;
pub mod handles;
pub mod home;
pub mod icons;
pub mod layers;
pub mod layout_panel;
pub mod menus;
pub mod present;
pub mod proto_canvas;
pub mod proto_panel;
pub mod render_worker;
pub mod right;
pub mod section_panel;
pub mod shortcuts;
pub mod tabs;
pub mod theme;
pub mod toolbar;
pub mod typography;
pub mod variable_ui;
pub mod variables_panel;
pub mod vector_panel;
pub mod versions;
pub mod widgets;

#[cfg(test)]
mod layers_drag_tests;
#[cfg(test)]
mod perf_tests;

use std::collections::BTreeSet;
use std::path::PathBuf;

use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::{Value, json};

pub use control::{ControlReply, ControlRequest};

/// Platform services the app crate provides (file dialogs, clipboard, URLs).
pub trait Services {
    fn pick_open(&mut self, _title: &str, _exts: &[&str]) -> Option<PathBuf> {
        None
    }
    fn pick_save(&mut self, _title: &str, _name: &str, _exts: &[&str]) -> Option<PathBuf> {
        None
    }
    /// A folder to write several files into.
    fn pick_folder(&mut self, _title: &str) -> Option<PathBuf> {
        None
    }
    fn clipboard_set_text(&mut self, _s: &str) {}
    fn clipboard_get_text(&mut self) -> Option<String> {
        None
    }
    fn clipboard_set_png(&mut self, _w: u32, _h: u32, _rgba: &[u8]) {}
    /// An image on the system clipboard as (width, height, straight RGBA).
    fn clipboard_get_image(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        None
    }
    fn open_url(&mut self, _url: &str) {}
    /// Recently opened files (persisted by the app).
    fn recent_files(&self) -> Vec<String> {
        Vec::new()
    }
    fn add_recent(&mut self, _path: &str) {}
}

/// No-op services (tests, headless screenshots).
pub struct NoServices;
impl Services for NoServices {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum RightTab {
    Design,
    Prototype,
    Inspect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum LeftTab {
    File,
    Assets,
}

/// How often open documents get an autosave checkpoint (seconds).
pub const AUTOSAVE_SECS: f64 = 30.0 * 60.0;

/// UI-only state (serialisable for `ui.inspect`).
pub struct UiState {
    pub left_width: f32,
    pub right_width: f32,
    pub right_tab: RightTab,
    pub left_tab: LeftTab,
    pub pages_open: bool,
    pub expanded: BTreeSet<NodeId>,
    pub rename: Option<(NodeId, String)>,
    pub rename_page: Option<(NodeId, String)>,
    pub main_menu: bool,
    pub palette: Option<String>,
    /// The Variables editor, when open.
    pub variables: Option<variables_panel::Editor>,
    pub about: bool,
    pub preferences: bool,
    pub shortcuts_panel: bool,
    pub home: bool,
    pub toast: Option<(String, f64)>,
    pub present: Option<present::Presenter>,
    pub picker: Option<colorpicker::PickerTarget>,
    pub context_menu: Option<(egui::Pos2, Vec<NodeId>)>,
    /// Layers panel search: open, the filter text, the shift-click range anchor and the selection
    /// the tree last revealed (ancestors expand and the row scrolls into view when it changes).
    pub layer_search: bool,
    pub layer_filter: String,
    pub layer_anchor: Option<NodeId>,
    pub layers_seen_sel: Vec<NodeId>,
    pub dark: bool,
    pub follow_system: bool,
    pub tool_flyout: Option<&'static str>,
    pub last_error: Option<String>,
    pub screenshot_requests: Vec<(u64, String)>,
    pub frame_ms: f64,
    pub render_ms: f64,
    pub quit: bool,
    /// The Export dialog is open.
    pub export_preview: bool,
    /// The Design panel's Export section shows its preview.
    pub export_thumb: bool,
    pub zoom_menu: bool,
    pub find: Option<(String, String)>,
    pub left_collapsed: bool,
    pub comments: comments::CommentUi,
    pub versions: versions::VersionUi,

    /// What FigCraft last put on the system clipboard: when the clipboard still holds it, a paste
    /// uses our copied layers; anything else there (an image, SVG, text) came from another app.
    pub last_copy: Option<String>,
    /// When (UI clock, seconds) open documents next get an autosave checkpoint.
    pub next_autosave: f64,
    /// The Assets panel shows the libraries view (publish this file, enabled libraries).
    pub libraries: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            left_width: 240.0,
            right_width: 240.0,
            right_tab: RightTab::Design,
            left_tab: LeftTab::File,
            pages_open: true,
            expanded: BTreeSet::new(),
            rename: None,
            rename_page: None,
            main_menu: false,
            palette: None,
            variables: None,
            about: false,
            preferences: false,
            shortcuts_panel: false,
            home: false,
            toast: None,
            present: None,
            picker: None,
            context_menu: None,
            layer_search: false,
            layer_filter: String::new(),
            layer_anchor: None,
            layers_seen_sel: Vec::new(),
            dark: false,
            follow_system: false,
            tool_flyout: None,
            last_error: None,
            screenshot_requests: Vec::new(),
            frame_ms: 0.0,
            render_ms: 0.0,
            quit: false,
            export_preview: false,
            export_thumb: false,
            zoom_menu: false,
            find: None,
            left_collapsed: false,
            comments: comments::CommentUi::default(),
            versions: versions::VersionUi::default(),

            last_copy: None,
            next_autosave: 0.0,
            libraries: false,
        }
    }
}

/// The whole FigCraft UI.
pub struct FigApp {
    pub session: Session,
    pub ui: UiState,
    pub canvas: canvas::CanvasState,
    pub services: Box<dyn Services>,
    worker: Option<render_worker::Worker>,
    fonts_installed: bool,
    /// Control-channel requests waiting for the next frame.
    pub control_queue: Vec<ControlRequest>,
}

impl FigApp {
    pub fn new(session: Session, services: Box<dyn Services>) -> Self {
        Self {
            session,
            ui: UiState::default(),
            canvas: canvas::CanvasState::default(),
            services,
            worker: None,
            fonts_installed: false,
            control_queue: Vec::new(),
        }
    }

    /// Run a command, surfacing errors as a toast. Returns the result.
    pub fn run(&mut self, id: &str, params: Value) -> Option<Value> {
        match self.session.execute(id, &params) {
            Ok(v) => {
                self.handle_ui_requests();
                Some(v)
            }
            Err(e) => {
                self.handle_ui_requests();
                let msg = e.to_string();
                log::warn!("{id}: {msg}");
                self.toast(msg.clone());
                self.ui.last_error = Some(msg);
                None
            }
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.ui.toast = Some((msg.into(), 0.0));
    }

    /// Act on UI requests queued by commands (dialogs, panels).
    pub fn handle_ui_requests(&mut self) {
        let reqs = std::mem::take(&mut self.session.ui_requests);
        for r in reqs {
            match r.kind.as_str() {
                "dialog.about" => self.ui.about = true,
                "dialog.preferences" => self.ui.preferences = true,
                "panel.shortcuts" => self.ui.shortcuts_panel = !self.ui.shortcuts_panel,
                "palette.actions" => self.ui.palette = Some(String::new()),
                "panel.design" => self.ui.right_tab = RightTab::Design,
                "panel.prototype" => self.ui.right_tab = RightTab::Prototype,
                "panel.layers" => self.ui.left_tab = LeftTab::File,
                "panel.assets" => self.ui.left_tab = LeftTab::Assets,
                "panel.versionHistory" => versions::open(self),
                "panel.libraries" => {
                    self.ui.left_tab = LeftTab::Assets;
                    self.ui.left_collapsed = false;
                    self.ui.libraries = true;
                }
                "home" => self.ui.home = true,
                "dialog.saveAs" => self.save_as_dialog(),
                "present" => {
                    if let Some(start) = r.data.get("start").and_then(figcraft_engine::params::id_of) {
                        self.ui.present = Some(present::Presenter::new(start));
                    }
                }
                "open.help" => self.services.open_url("https://getartcraft.com/apps/figcraft"),
                "layers.collapseAll" => self.ui.expanded.clear(),
                "comment.new" => {
                    let x = r.data.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = r.data.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    comments::start_draft(self, x, y);
                }
                "dialog.unsaved" => self.toast("This file has unsaved changes"),
                _ => {}
            }
        }
        if let Some(m) = self.session.message.take() {
            self.toast(m);
        }
    }

    pub fn save_as_dialog(&mut self) {
        let name = self.session.doc().map(|d| d.name.clone()).unwrap_or_else(|_| "Untitled".into());
        if let Some(p) = self.services.pick_save("Save as", &format!("{name}.figcraft"), &["figcraft"]) {
            let ps = p.to_string_lossy().to_string();
            if self.run("file.saveAs", json!({"path": ps})).is_some() {
                self.services.add_recent(&ps);
                self.toast(format!("Saved {}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()));
            }
        }
    }

    pub fn save(&mut self) {
        if self.session.ds().map(|d| d.path.is_some()).unwrap_or(false) {
            if self.run("file.save", json!({})).is_some() {
                self.toast("Saved");
            }
        } else {
            self.save_as_dialog();
        }
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = self.services.pick_open("Open", &["figcraft", "json", "svg", "png", "jpg", "jpeg", "webp", "gif"]) {
            let ps = p.to_string_lossy().to_string();
            if self.run("file.open", json!({"path": ps})).is_some() {
                self.services.add_recent(&ps);
                self.ui.home = false;
            }
        }
    }

    pub fn import_dialog(&mut self, place: bool) {
        let exts: &[&str] = if place { &["png", "jpg", "jpeg", "webp", "gif"] } else { &["svg", "png", "jpg", "jpeg", "webp", "gif", "figcraft"] };
        if let Some(p) = self.services.pick_open(if place { "Place image" } else { "Import" }, exts) {
            let ps = p.to_string_lossy().to_string();
            self.run("file.import", json!({"path": ps}));
        }
    }

    /// Per-frame logic that doesn't draw (control requests, theme).
    pub fn logic(&mut self, ctx: &egui::Context) {
        if !self.fonts_installed {
            theme::install_fonts(ctx);
            self.fonts_installed = true;
        }
        let dark = match self.session.prefs.theme.as_str() {
            "dark" => true,
            "system" => ctx.global_style().visuals.dark_mode,
            _ => false,
        };
        self.ui.dark = dark;
        theme::apply(ctx, if dark { theme::Tokens::dark() } else { theme::Tokens::light() });
        if self.worker.is_none() {
            self.worker = Some(render_worker::Worker::new(ctx.clone()));
        }
    }

    /// Every [`AUTOSAVE_SECS`], each open document with unsaved changes since its last version
    /// gets an autosave checkpoint in its version history.
    fn autosave_tick(&mut self, now: f64) {
        if now < self.ui.next_autosave {
            return;
        }
        if self.ui.next_autosave > 0.0 {
            let active = self.session.active;
            for i in 0..self.session.docs.len() {
                self.session.active = i;
                let _ = self.session.execute("version.autosave", &json!({}));
            }
            self.session.active = active;
        }
        self.ui.next_autosave = now + AUTOSAVE_SECS;
    }

    /// Draw the whole UI.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        #[cfg(not(target_arch = "wasm32"))]
        let t0 = std::time::Instant::now();
        let ctx = ui.ctx().clone();
        let first = !self.fonts_installed;
        self.logic(&ctx);
        self.autosave_tick(ctx.input(|i| i.time));
        if first {
            // New fonts apply from the next frame; draw nothing until then.
            ctx.request_repaint();
            return;
        }
        let t = theme::Tokens::get(&ctx);
        // Control requests run first so their results reflect this frame.
        control::drain(self, &ctx);
        if self.ui.present.is_some() {
            present::show(self, ui);
            return;
        }
        if self.session.docs.is_empty() || self.ui.home {
            tabs::tab_bar(self, ui);
            home::show(self, ui);
            dialogs::show(self, &ctx);
            return;
        }
        let hidden = self.session.ds().map(|d| d.view.ui_hidden).unwrap_or(false);
        tabs::tab_bar(self, ui);
        if !hidden {
            egui::Panel::left("left_rail")
                .exact_size(56.0)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(t.panel).stroke(egui::Stroke::new(1.0, t.border)))
                .show(ui, |ui| layers::rail(self, ui));
        }
        if !hidden && !self.ui.left_collapsed {
            egui::Panel::left("left_panel")
                .resizable(true)
                .default_size(self.ui.left_width)
                .size_range(200.0..=480.0)
                .frame(egui::Frame::NONE.fill(t.panel).stroke(egui::Stroke::new(1.0, t.border)))
                .show(ui, |ui| layers::left_panel(self, ui));
        }
        if !hidden {
            egui::Panel::right("right_panel")
                .resizable(true)
                .default_size(self.ui.right_width)
                .size_range(240.0..=480.0)
                .frame(egui::Frame::NONE.fill(t.panel).stroke(egui::Stroke::new(1.0, t.border)))
                .show(ui, |ui| right::right_panel(self, ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
            let worker = self.worker.take();
            let mut worker = worker.unwrap_or_else(|| render_worker::Worker::new(ctx.clone()));
            canvas::show(self, ui, &mut worker);
            self.worker = Some(worker);
            if !hidden {
                toolbar::show(self, ui);
            }
        });
        shortcuts::handle(self, &ctx);
        menus::popups(self, &ctx);
        variable_ui::picker(self, &ctx);
        dialogs::show(self, &ctx);
        self.toast_ui(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ui.frame_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, age)) = self.ui.toast.clone() else { return };
        let dt = ctx.input(|i| i.stable_dt) as f64;
        let age = age + dt;
        if age > 3.0 {
            self.ui.toast = None;
            return;
        }
        self.ui.toast = Some((msg.clone(), age));
        ctx.request_repaint();
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("toast"))
            .fixed_pos(egui::pos2(screen.center().x, screen.max.y - 96.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(0x1E, 0x1E, 0x1E))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(msg).color(egui::Color32::WHITE).font(theme::regular(12.0)));
                    });
            });
    }

    /// UI state as JSON (control channel `ui.inspect`).
    pub fn inspect(&self) -> Value {
        json!({
            "rightTab": self.ui.right_tab,
            "leftTab": self.ui.left_tab,
            "home": self.ui.home,
            "dark": self.ui.dark,
            "presenting": self.ui.present.is_some(),
            "mainMenu": self.ui.main_menu,
            "palette": self.ui.palette.is_some(),
            "variables": self.ui.variables.is_some(),
            "versionHistory": self.ui.versions.open,
            "comments": {"open": self.ui.comments.open, "draft": self.ui.comments.draft.as_ref().map(|d| [d.0, d.1]), "showResolved": self.ui.comments.show_resolved},
            "frameMs": self.ui.frame_ms,
            "renderMs": self.ui.render_ms,
            "canvas": {
                "rect": [self.canvas.rect.min.x, self.canvas.rect.min.y, self.canvas.rect.max.x, self.canvas.rect.max.y],
                "renderWork": self.canvas.render_work.map(|w| match w {
                    figcraft_render::Work::Reused => json!({"kind": "reused"}),
                    figcraft_render::Work::Partial { pixels } => json!({"kind": "partial", "pixels": pixels}),
                    figcraft_render::Work::Full => json!({"kind": "full"}),
                }),
            },
            "tool": self.session.tool_info(),
            "status": self.session.status(),
            "lastError": self.ui.last_error,
        })
    }
}
