//! Desktop services: native file dialogs, clipboard, URLs, persisted preferences.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use figcraft_engine::{Prefs, Session};
use figcraft_ui_egui::{FigApp, Services};
use serde_json::{Value, json};

pub struct Desktop {
    clipboard: Option<arboard::Clipboard>,
    pub prefs: Option<Prefs>,
    /// The system's reduce-motion setting (read in the background at startup).
    reduce_motion: Arc<AtomicBool>,
    /// The system's interface language (read at startup).
    language: Option<String>,
    /// Recovery copies of unsaved files (written at once after a panic).
    recovery: crate::recovery::Shared,
}

pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|b| b.join("FigCraft"))
}

impl Desktop {
    pub fn load(recovery: crate::recovery::Shared) -> Self {
        let state: Value = config_dir()
            .and_then(|d| std::fs::read_to_string(d.join("state.json")).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(json!({}));
        let prefs = state.get("prefs").and_then(|p| serde_json::from_value::<Prefs>(p.clone()).ok()).map(|mut p| {
            // Recent files lived next to the preferences before they moved into them.
            if p.recent_files.is_empty()
                && let Some(old) = state.get("recents").and_then(Value::as_array)
            {
                p.recent_files = old.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
            }
            p.recent_files.truncate(figcraft_engine::cmd::file::MAX_RECENT);
            p
        });
        let reduce_motion = Arc::new(AtomicBool::new(false));
        let flag = reduce_motion.clone();
        std::thread::spawn(move || flag.store(system_reduce_motion(), Ordering::Relaxed));
        Self { clipboard: arboard::Clipboard::new().ok(), prefs, reduce_motion, language: system_language(), recovery }
    }
}

/// The system's preferred interface language as a tag ("es-ES", "es_ES.UTF-8"): Windows' language
/// list, macOS's AppleLanguages, the locale variables elsewhere.
fn system_language() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        windows::System::UserProfile::GlobalizationPreferences::Languages()
            .and_then(|l| l.GetAt(0))
            .map(|s| s.to_string())
            .inspect_err(|e| log::info!("language: can't read the system languages: {e}"))
            .ok()
    }
    // `defaults` prints a list: ( "es-ES", en ).
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output().ok()?;
        String::from_utf8_lossy(&out.stdout).split(['(', ',', ')']).map(|s| s.trim().trim_matches('"').to_string()).find(|s| !s.is_empty())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty() && v != "C" && v != "POSIX")
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

/// Whether the system's accessibility settings ask for reduced motion: Windows' animation
/// effects (`UISettings.AnimationsEnabled`), macOS's Reduce motion, GNOME's animations.
fn system_reduce_motion() -> bool {
    #[cfg(target_os = "windows")]
    {
        match windows::UI::ViewManagement::UISettings::new().and_then(|s| s.AnimationsEnabled()) {
            Ok(on) => !on,
            Err(e) => {
                log::info!("reduce motion: can't read the animation setting: {e}");
                false
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        command_says("defaults", &["read", "com.apple.universalaccess", "reduceMotion"], "1")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command_says("gsettings", &["get", "org.gnome.desktop.interface", "enable-animations"], "false")
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

/// Whether `program args` runs and prints `want` (trimmed).
#[cfg(unix)]
fn command_says(program: &str, args: &[&str], want: &str) -> bool {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == want)
}

pub fn save_state(app: &FigApp) {
    let Some(dir) = config_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let v = json!({"prefs": app.session.prefs});
    let tmp = dir.join("state.json.tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("state.json"));
    }
}

impl Services for Desktop {
    fn pick_open(&mut self, title: &str, exts: &[&str]) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).add_filter("Supported files", exts).pick_file()
    }
    fn pick_save(&mut self, title: &str, name: &str, exts: &[&str]) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).set_file_name(name).add_filter("File", exts).save_file()
    }
    fn pick_folder(&mut self, title: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }
    fn clipboard_set_text(&mut self, s: &str) {
        if let Some(c) = self.clipboard.as_mut() {
            let _ = c.set_text(s.to_string());
        }
    }
    fn clipboard_get_text(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }
    fn clipboard_set_png(&mut self, w: u32, h: u32, rgba: &[u8]) {
        if let Some(c) = self.clipboard.as_mut() {
            let _ = c.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Borrowed(rgba) });
        }
    }
    fn clipboard_get_image(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let img = self.clipboard.as_mut()?.get_image().ok()?;
        let (w, h) = (u32::try_from(img.width).ok()?, u32::try_from(img.height).ok()?);
        // Refuse absurd sizes (and mismatched buffers) rather than allocate.
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 100_000_000 || img.bytes.len() != (w as usize) * (h as usize) * 4 {
            return None;
        }
        Some((w, h, img.bytes.into_owned()))
    }
    fn open_url(&mut self, url: &str) {
        let _ = webbrowser::open(url);
    }
    fn emergency_save(&mut self, session: &Session) {
        if let Some(r) = crate::recovery::lock(&self.recovery).as_mut() {
            r.write(session, true);
        }
    }
    fn reduce_motion(&self) -> bool {
        self.reduce_motion.load(Ordering::Relaxed)
    }
    fn system_language(&self) -> Option<String> {
        self.language.clone()
    }
}

/// The window icon from the bundled app icon (if present).
pub fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../assets/app-icon/figcraft-256.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}
