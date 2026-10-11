//! Desktop services: native file dialogs, clipboard, URLs, persisted preferences and recents.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use figcraft_engine::{Prefs, Session};
use figcraft_ui_egui::{FigApp, Services};
use serde_json::{Value, json};

pub struct Desktop {
    clipboard: Option<arboard::Clipboard>,
    recents: Vec<String>,
    pub prefs: Option<Prefs>,
    /// The system's reduce-motion setting (read in the background at startup).
    reduce_motion: Arc<AtomicBool>,
}

fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|b| b.join("FigCraft"))
}

impl Desktop {
    pub fn load() -> Self {
        let state: Value = config_dir()
            .and_then(|d| std::fs::read_to_string(d.join("state.json")).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(json!({}));
        let recents = state
            .get("recents")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let prefs = state.get("prefs").and_then(|p| serde_json::from_value(p.clone()).ok());
        let reduce_motion = Arc::new(AtomicBool::new(false));
        let flag = reduce_motion.clone();
        std::thread::spawn(move || flag.store(system_reduce_motion(), Ordering::Relaxed));
        Self { clipboard: arboard::Clipboard::new().ok(), recents, prefs, reduce_motion }
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
    let v = json!({"recents": app.services.recent_files(), "prefs": app.session.prefs});
    let tmp = dir.join("state.json.tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("state.json"));
    }
}

/// After an unexpected panic: write every dirty document to the recovery folder.
pub fn emergency_save(s: &Session) {
    let Some(dir) = config_dir().map(|d| d.join("Recovery")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    for d in &s.docs {
        if d.is_dirty()
            && let Ok(bytes) = figcraft_format_save(&d.doc)
        {
            let name = d.doc.name.replace(['/', '\\', ':'], "_");
            let _ = std::fs::write(dir.join(format!("{name}-{}.figcraft", d.uid)), bytes);
        }
    }
}

fn figcraft_format_save(doc: &figcraft_engine::doc::Document) -> Result<Vec<u8>, String> {
    let v = serde_json::to_vec(doc).map_err(|e| e.to_string())?;
    Ok(v)
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
    fn recent_files(&self) -> Vec<String> {
        self.recents.clone()
    }
    fn add_recent(&mut self, path: &str) {
        self.recents.retain(|p| p != path);
        self.recents.insert(0, path.to_string());
        self.recents.truncate(20);
    }
    fn reduce_motion(&self) -> bool {
        self.reduce_motion.load(Ordering::Relaxed)
    }
}

/// The system's spell checker, connected on first use from the UI thread (on Windows, COM is set
/// up there once the window exists).
#[cfg(any(windows, target_os = "macos"))]
#[derive(Default)]
pub struct SystemSpelling(Option<os_spellcheck::SpellChecker>);

#[cfg(any(windows, target_os = "macos"))]
impl figcraft_engine::spelling::SpellChecker for SystemSpelling {
    fn misspelled(&mut self, word: &str) -> bool {
        !self.0.get_or_insert_with(os_spellcheck::SpellChecker::new).check(word).is_empty()
    }
    fn suggest(&mut self, word: &str) -> Vec<String> {
        self.0.get_or_insert_with(os_spellcheck::SpellChecker::new).suggestions(word)
    }
}

/// The window icon from the bundled app icon (if present).
pub fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../assets/app-icon/figcraft-256.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}
