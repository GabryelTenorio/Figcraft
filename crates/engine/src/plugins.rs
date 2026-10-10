//! Plugins: WebAssembly modules that act on the document through the same commands as the menus,
//! CLI and MCP server, inside the sandbox of `figcraft-plugin`. This module is the policy: which
//! commands a plugin may call (its manifest's permissions, and never files, other documents, the
//! undo history, dialogs or plugins), one undo step per run, and the installed-plugin list kept in
//! the preferences. See `docs/plugins.md`.

use std::collections::HashMap;
use std::sync::Arc;

use figcraft_plugin::{Limits, PluginError};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, find_command};
use crate::params::{get, num, string};
use crate::{EngineError, Result, Session, bad, spec};

pub use figcraft_plugin::{Access, MANIFEST_FILE, Manifest, Param, ParamKind};

/// An installed plugin (in the preferences).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub manifest: Manifest,
    /// Its manifest file; `None` when it was imported without one (its module then lives in this
    /// session only and isn't saved with the preferences).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Saves only the plugins imported from a manifest file (the others can't be loaded again).
pub(crate) fn persisted<S: Serializer>(list: &[InstalledPlugin], s: S) -> std::result::Result<S::Ok, S::Error> {
    s.collect_seq(list.iter().filter(|p| p.path.is_some()))
}

/// Per-session plugin state.
#[derive(Clone, Debug, Default)]
pub struct Runtime {
    /// Modules of plugins imported without a manifest file, by plugin id.
    modules: HashMap<String, Arc<Vec<u8>>>,
    /// The last run (plugin id, command id, params): "Run last plugin".
    pub last: Option<(String, String, Value)>,
}

/// Command prefixes no plugin may call, whatever its permissions: files and the app (plugins get
/// no file system), other documents, version history, the undo history, plugins themselves and
/// nested batches (a run already is one undo step).
const DENIED: &[&str] = &[
    "file.",
    "doc.",
    "version.",
    "app.",
    "pref.",
    "help.",
    "plugin.",
    "plugins.",
    "libraries.",
    "library.add",
    "edit.undo",
    "edit.redo",
    "edit.repeat",
    "command.batch",
    "view.theme",
];
/// Read-only exceptions to [`DENIED`].
const ALLOWED: &[&str] = &["doc.list", "version.list"];

/// Why a plugin may not call `command` with `params` (before running it), if it may not.
fn refusal(m: &Manifest, command: &str, params: &Value) -> Option<String> {
    if DENIED.iter().any(|p| command.starts_with(p)) && !ALLOWED.contains(&command) {
        return Some(format!("`{command}` isn't available to plugins"));
    }
    if command == "image.place" && get(params, "path").is_some() {
        return Some("plugins can't read files: pass the image as `data`".into());
    }
    if !m.permissions.allows(command) {
        return Some(format!("`{command}` isn't in the commands {} asked permission for", m.name));
    }
    if m.permissions.access == Access::Read && find_command(command).is_some_and(|c| c.history) {
        return Some(format!("`{command}` edits the document, and {} may only read it", m.name));
    }
    None
}

/// The sandbox's view of the session: commands go through [`refusal`] and run as nested groups,
/// so a refused one leaves nothing behind.
struct EngineHost<'s> {
    s: &'s mut Session,
    manifest: &'s Manifest,
}

impl figcraft_plugin::Host for EngineHost<'_> {
    fn call(&mut self, command: &str, params: &str) -> std::result::Result<String, String> {
        let params: Value = serde_json::from_str(params).map_err(|e| format!("params for `{command}` aren't JSON: {e}"))?;
        if let Some(why) = refusal(self.manifest, command, &params) {
            return Err(why);
        }
        let read_only = self.manifest.permissions.access == Access::Read;
        let ui_before = self.s.ui_requests.len();
        let r = self.s.grouped(command, |s| {
            let doc = s.ds()?.doc.clone();
            let v = s.execute(command, &params)?;
            let opened_ui = s.ui_requests.len() > ui_before;
            s.ui_requests.truncate(ui_before);
            if opened_ui {
                return Err(EngineError::Disabled(command.into(), "it opens a dialog or panel, which plugins can't".into()));
            }
            if read_only && !Arc::ptr_eq(&doc, &s.ds()?.doc) {
                return Err(EngineError::Disabled(command.into(), "it changed the document, and this plugin may only read it".into()));
            }
            Ok(v)
        });
        r.map(|v| v.to_string()).map_err(|e| e.to_string())
    }

    fn selection(&mut self) -> String {
        json!(self.s.selection()).to_string()
    }

    fn notify(&mut self, message: &str) {
        self.s.message = Some(message.to_string());
    }

    fn log(&mut self, line: &str) {
        log::info!("plugin {}: {line}", self.manifest.id);
    }
}

/// The installed plugin `id`.
fn installed<'a>(s: &'a Session, cmd: &str, id: &str) -> Result<&'a InstalledPlugin> {
    s.prefs.plugins.iter().find(|p| p.manifest.id == id).ok_or_else(|| bad(cmd, format!("no plugin `{id}` is installed")))
}

/// Read a file of at most `max` bytes.
#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &std::path::Path, max: usize) -> Result<Vec<u8>> {
    let shown = path.display();
    let meta = std::fs::metadata(path).map_err(|e| EngineError::Other(format!("{shown}: {e}")))?;
    if !meta.is_file() || meta.len() > max as u64 {
        return Err(EngineError::Other(format!("{shown}: not a file of at most {} KB", max / 1024)));
    }
    std::fs::read(path).map_err(|e| EngineError::Other(format!("{shown}: {e}")))
}

/// The manifest at `path` (the file, or the folder holding `figcraft-plugin.json`), its absolute
/// path and its module.
#[cfg(not(target_arch = "wasm32"))]
fn load_from_disk(cmd: &str, path: &str) -> Result<(Manifest, String, Vec<u8>)> {
    let mut file = std::path::PathBuf::from(crate::library::absolute_path(path.trim()));
    if file.is_dir() {
        file.push(MANIFEST_FILE);
    }
    let text = read_file(&file, figcraft_plugin::manifest::MAX_MANIFEST_BYTES)?;
    let text = String::from_utf8(text).map_err(|_| bad(cmd, "the manifest isn't UTF-8 text"))?;
    let manifest = Manifest::parse(&text).map_err(|e| bad(cmd, e))?;
    let wasm = module_file(&file, &manifest)?;
    Ok((manifest, file.to_string_lossy().into_owned(), wasm))
}

#[cfg(target_arch = "wasm32")]
fn load_from_disk(cmd: &str, _path: &str) -> Result<(Manifest, String, Vec<u8>)> {
    Err(bad(cmd, "there are no files in the browser: pass `manifest` and `wasm` (base64)"))
}

/// The module a manifest at `manifest_file` names.
#[cfg(not(target_arch = "wasm32"))]
fn module_file(manifest_file: &std::path::Path, m: &Manifest) -> Result<Vec<u8>> {
    let dir = manifest_file.parent().unwrap_or(std::path::Path::new("."));
    read_file(&dir.join(&m.main), Limits::default().module_bytes)
}

/// The module of an installed plugin: from the session, else read again from disk (so a rebuilt
/// development plugin runs its new code).
fn module_of(s: &Session, p: &InstalledPlugin) -> Result<Arc<Vec<u8>>> {
    if let Some(m) = s.plugins.modules.get(&p.manifest.id) {
        return Ok(m.clone());
    }
    let Some(path) = &p.path else {
        return Err(EngineError::Other(format!("{} was imported without a manifest file: import it again", p.manifest.name)));
    };
    #[cfg(not(target_arch = "wasm32"))]
    return module_file(std::path::Path::new(path), &p.manifest).map(Arc::new);
    #[cfg(target_arch = "wasm32")]
    return Err(EngineError::Other(format!("{path}: there are no files in the browser")));
}

fn plugin_error(name: &str, e: PluginError) -> EngineError {
    EngineError::Other(format!("{name}: {e}"))
}

/// A plugin as `plugin.list` reports it.
fn describe(s: &Session, p: &InstalledPlugin) -> Value {
    let mut v = serde_json::to_value(&p.manifest).unwrap_or_else(|_| json!({"id": p.manifest.id}));
    v["path"] = json!(p.path);
    v["loaded"] = json!(p.path.is_some() || s.plugins.modules.contains_key(&p.manifest.id));
    v
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    let plugins: Vec<Value> = s.prefs.plugins.iter().map(|p| describe(s, p)).collect();
    let last = s.plugins.last.as_ref().map(|(id, command, _)| json!({"id": id, "command": command}));
    Ok(json!({"plugins": plugins, "last": last}))
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "plugin.import";
    let (manifest, path, wasm) = match (string(p, "path"), get(p, "manifest")) {
        (Some(path), _) => {
            let (m, path, wasm) = load_from_disk(cmd, &path)?;
            (m, Some(path), wasm)
        }
        (None, Some(m)) => {
            let m = match m {
                Value::String(text) => Manifest::parse(text),
                other => Manifest::from_value(other),
            }
            .map_err(|e| bad(cmd, e))?;
            use base64::Engine as _;
            let b64 = string(p, "wasm").ok_or_else(|| bad(cmd, "pass the module as `wasm` (base64) with `manifest`"))?;
            if b64.len() > Limits::default().module_bytes / 3 * 4 + 4 {
                return Err(bad(cmd, "the module is too large"));
            }
            let wasm = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| bad(cmd, format!("`wasm`: {e}")))?;
            (m, None, wasm)
        }
        (None, None) => return Err(bad(cmd, "pass `path` (a figcraft-plugin.json or its folder), or `manifest` and `wasm`")),
    };
    figcraft_plugin::check_module(&wasm, &Limits::default()).map_err(|e| plugin_error(&manifest.name, e))?;
    let id = manifest.id.clone();
    if path.is_none() {
        s.plugins.modules.insert(id.clone(), Arc::new(wasm));
    } else {
        s.plugins.modules.remove(&id);
    }
    let entry = InstalledPlugin { manifest, path };
    let mut out = describe(s, &entry);
    let list = &mut s.prefs.plugins;
    let replaced = match list.iter_mut().find(|x| x.manifest.id == id) {
        Some(slot) => {
            *slot = entry;
            true
        }
        None => {
            list.push(entry);
            false
        }
    };
    out["replaced"] = json!(replaced);
    Ok(out)
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "plugin.remove";
    let id = string(p, "id").ok_or_else(|| bad(cmd, "`id` required"))?;
    installed(s, cmd, &id)?;
    s.prefs.plugins.retain(|x| x.manifest.id != id);
    s.plugins.modules.remove(&id);
    if s.plugins.last.as_ref().is_some_and(|(last, ..)| *last == id) {
        s.plugins.last = None;
    }
    Ok(json!({"removed": id}))
}

/// Run plugin `id`'s `command` with `params` as one undo step.
pub fn run(s: &mut Session, id: &str, command: Option<&str>, params: &Value, fuel: Option<f64>) -> Result<Value> {
    let cmd = "plugin.run";
    let mut p = installed(s, cmd, id)?.clone();
    // Checked again: the preferences file may have been edited by hand.
    p.manifest = Manifest::from_value(&json!(p.manifest)).map_err(|e| bad(cmd, e))?;
    let m = &p.manifest;
    let c = m.command(command).ok_or_else(|| bad(cmd, format!("{} has no command `{}`", m.name, command.unwrap_or_default())))?;
    let params = c.resolve_params(params).map_err(|e| bad(cmd, e))?;
    let wasm = module_of(s, &p)?;
    let mut limits = Limits::default();
    if let Some(f) = fuel {
        limits.fuel = (f.max(1000.0) as u64).min(limits.fuel);
    }
    let input = json!({"api": figcraft_plugin::API_VERSION, "command": c.id, "params": params, "selection": s.selection()}).to_string();
    s.plugins.last = Some((m.id.clone(), c.id.clone(), params));
    let label = if m.commands.len() > 1 { format!("{}: {}", m.name, c.label) } else { m.name.clone() };
    let out = s.grouped(&label, |s| {
        let mut host = EngineHost { s, manifest: m };
        figcraft_plugin::run(&wasm, &input, &limits, &mut host).map_err(|e| plugin_error(&m.name, e))
    })?;
    let output = serde_json::from_str::<Value>(&out.output).unwrap_or(Value::String(out.output));
    Ok(json!({"plugin": m.id, "command": c.id, "output": output, "logs": out.logs, "calls": out.calls, "fuel": out.fuel}))
}

fn run_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let id = string(p, "id").ok_or_else(|| bad("plugin.run", "`id` required (see plugin.list)"))?;
    let command = string(p, "command");
    run(s, &id, command.as_deref(), get(p, "params").unwrap_or(&Value::Null), num(p, "fuel"))
}

fn run_last(s: &mut Session, _p: &Value) -> Result<Value> {
    let Some((id, command, params)) = s.plugins.last.clone() else {
        return Err(EngineError::Disabled("plugins.runLast".into(), "no plugin has run yet".into()));
    };
    run(s, &id, Some(&command), &params, None)
}

fn manage(s: &mut Session, p: &Value) -> Result<Value> {
    s.request_ui("dialog.plugins", json!({}));
    list(s, p)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("plugin.list", "List plugins", "{} → {plugins: [{id, name, version, main, permissions, commands, path, loaded}], last}", false, list),
        spec!(
            "plugin.import",
            "Import plugin from manifest…",
            "{path (figcraft-plugin.json or its folder) | manifest (object or JSON text), wasm (base64)} replaces a plugin with the same id",
            false,
            import
        ),
        spec!("plugin.remove", "Remove plugin", "{id}", false, remove),
        spec!(
            "plugin.run",
            "Run plugin",
            "{id, command? (default: the first), params? (the command's parameters), fuel? (lower instruction budget)} one undo step → {output, logs, calls, fuel}",
            false,
            run_cmd
        ),
        spec!("plugins.runLast", "Run last plugin", "{}", false, run_last),
        spec!("plugins.manage", "Manage plugins…", "{}  opens the plugins window; returns the list like `plugin.list`", false, manage),
    ]
}
