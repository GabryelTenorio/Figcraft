//! FigCraft desktop app.
//!
//! Usage: `figcraft [--control <port>] [files…] [figcraft://join?…]` (an invite link joins that
//! live session; the app registers itself as the opener of such links: `collab.registerLinks`).
//!
//! `--control <port>` (or `FIGCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"engine.execute","params":{"command":"shape.rectangle","params":{"x":0,"y":0,"width":100,"height":100}}}`
//! → `{"id":1,"ok":true,"result":{…}}`. Methods: `figcraft_ui_egui::control`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod args;
mod control_server;
mod services;

#[cfg(test)]
mod args_tests;

use std::sync::{Arc, Mutex};

use figcraft_engine::Session;
use figcraft_ui_egui::{ControlRequest, FigApp};
use serde_json::json;

struct App {
    fig: FigApp,
    queue: Arc<Mutex<Vec<ControlRequest>>>,
    ctx_slot: Arc<Mutex<Option<egui::Context>>>,
    /// The window title last sent: a viewport command asks for another frame, so it's only sent
    /// when the title changes (sending it every frame kept an idle window redrawing).
    title: String,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        {
            let mut slot = self.ctx_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot.is_none() {
                *slot = Some(ctx.clone());
            }
        }
        let mut q = self.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.fig.control_queue.extend(q.drain(..));
        drop(q);
        // Window title follows the document.
        let title = match self.fig.session.ds() {
            Ok(ds) => format!("{}{} – FigCraft", ds.doc.name, if ds.is_dirty() { " •" } else { "" }),
            Err(_) => "FigCraft".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Pointer and key events sent over the control channel (`ui.input`).
        raw_input.events.append(&mut self.fig.control_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Last-resort guard: a panic in the UI frame becomes a message, not a lost document.
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.fig.ui(ui)));
        if let Err(e) = res {
            let msg =
                e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into());
            log::error!("UI panic: {msg}");
            self.fig.toast(format!("Internal error (please report): {msg}"));
            ui.ctx().request_repaint();
            services::emergency_save(&self.fig.session);
        }
    }

    fn on_exit(&mut self) {
        services::save_state(&self.fig);
    }
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "FigCraft {}{}",
            env!("CARGO_PKG_VERSION"),
            option_env!("FIGCRAFT_BUILD_SHA").filter(|s| !s.is_empty()).map(|s| format!(" ({s})")).unwrap_or_default()
        );
        return Ok(());
    }
    let args = args::parse(&args);
    let port = args.control.or_else(|| std::env::var("FIGCRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok()));
    // System fonts: index them in the background, then wake the UI (an idle window draws no frames)
    // to lay text out again with them.
    let ctx_slot: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
    let fonts_slot = ctx_slot.clone();
    std::thread::spawn(move || {
        figcraft_text::scan_system_fonts();
        if let Some(c) = fonts_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
            c.request_repaint();
        }
    });
    let mut session = Session::empty();
    // Live sessions: the network threads wake the window when something arrives.
    let wake_slot = ctx_slot.clone();
    session.live.install(Box::new(figcraft_collab_net::NetLink::new(Arc::new(move || {
        if let Some(c) = wake_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
            c.request_repaint();
        }
    }))));
    for f in &args.files {
        if let Err(e) = session.execute("file.open", &json!({"path": f})) {
            log::error!("{f}: {e}");
        }
    }
    let svc = services::Desktop::load();
    if let Some(prefs) = svc.prefs.clone() {
        session.prefs = prefs;
    }
    // Invite links open this app (Windows registers per user at run time; installed builds by
    // default, never a build from source unless asked).
    session.live.links = figcraft_collab_net::links::system();
    session.invite_links_startup();
    // shortcut: no spell check on Linux and FreeBSD (their checkers are C libraries), add a
    // hunspell/enchant checker when Linux users ask.
    #[cfg(any(windows, target_os = "macos"))]
    {
        session.spelling.checker = Some(Box::new(services::SystemSpelling::default()));
    }
    if session.docs.is_empty() {
        let _ = session.execute("file.new", &json!({}));
    }
    let mut fig = FigApp::new(session, Box::new(svc));
    fig.ui.home = false;
    if let Some(link) = &args.join {
        figcraft_ui_egui::live::open_invite(&mut fig, link);
    }
    let queue: Arc<Mutex<Vec<ControlRequest>>> = Arc::new(Mutex::new(Vec::new()));
    if let Some(p) = port {
        control_server::start(p, queue.clone(), ctx_slot.clone());
    }
    let icon = services::window_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("FigCraft")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([800.0, 500.0])
        .with_drag_and_drop(true);
    // The tab strip is the title bar: macOS keeps its window buttons inset into it; elsewhere the
    // window is frameless and FigCraft draws the buttons and resize edges.
    if cfg!(target_os = "macos") {
        viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
        fig.ui.title_bar = figcraft_ui_egui::TitleBar::Inset;
    } else {
        viewport = viewport.with_decorations(false);
        fig.ui.title_bar = figcraft_ui_egui::TitleBar::Custom;
    }
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Wgpu, ..Default::default() };
    eframe::run_native("FigCraft", options, Box::new(move |_cc| Ok(Box::new(App { fig, queue, ctx_slot, title: String::new() }))))
}
