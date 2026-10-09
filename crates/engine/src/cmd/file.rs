//! File menu: documents, save/open, import/export, versions.

use std::sync::Arc;

use base64::Engine as _;
use figcraft_doc::*;
use figcraft_format::Version;
use serde_json::{Value, json};

use super::{CommandSpec, ok};
use crate::params::*;
use crate::{DocState, EngineError, Result, Session, bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("file.new", "New design file", "{name?}", false, |s, p| {
            let name = string(p, "name").unwrap_or_else(|| "Untitled".into());
            s.docs.push(DocState::new(Document::new(name), None));
            s.active = s.docs.len() - 1;
            Ok(json!({"index": s.active}))
        }),
        spec!(
            "file.open",
            "Open",
            "{path} (.figcraft, .json: FigCraft or REST API file JSON, .svg, .png, .jpg …); REST imports return `warnings`",
            false,
            open
        ),
        spec!("file.save", "Save", "{path?} (programmatic saves need a path the first time)", false, save),
        spec!("file.saveAs", "Save as", "{path}", false, save_as),
        spec!(
            "file.export",
            "Export",
            "{ids? (default selection, else the page), format? (png|jpg|svg|pdf|webp), scale? (default 1), path? (else returns base64 `data`)}",
            false,
            export
        ),
        spec!("file.exportSvg", "Export selection as SVG", "{ids?, path?}", false, |s, p| {
            let mut q = p.clone();
            if let Some(o) = q.as_object_mut() {
                o.insert("format".into(), json!("svg"));
            }
            export(s, &q)
        }),
        spec!(
            "file.exportFramesPdf",
            "Export frames to PDF",
            "{path?, page? (page id, default the current page)}  every top-level frame in reading order, one PDF page each; returns base64 `data` without a path",
            false,
            export_frames_pdf
        ),
        spec!(
            "file.import",
            "Import",
            "{path | data (base64) + name, x?, y?}  SVG becomes vectors, images become image layers, .figcraft and REST API file JSON pages are appended (REST: with `warnings`)",
            true,
            import
        ),
        spec!("file.placeImage", "Place image", "{path | data, x?, y?}", true, super::create::place_image),
        spec!("file.close", "Close tab", "{index?, force? (discard unsaved changes)}", false, close),
        spec!("file.rename", "Rename file", "{name}", true, |s, p| {
            let name = req_string("file.rename", p, "name")?;
            s.doc_mut()?.name = name;
            ok()
        }),
        spec!("file.duplicate", "Duplicate file", "{}", false, |s, _| {
            let ds = s.ds()?;
            let mut d = (*ds.doc).clone();
            d.name = format!("{} (Copy)", d.name);
            s.docs.push(DocState::new(d, None));
            s.active = s.docs.len() - 1;
            Ok(json!({"index": s.active}))
        }),
        spec!("file.revert", "Revert to saved", "{}", false, |s, _| {
            let path = s.ds()?.path.clone().ok_or_else(|| EngineError::Disabled("file.revert".into(), "the file was never saved".into()))?;
            let (loaded, _) = load_path(&path)?;
            let i = s.active;
            if let Some(slot) = s.docs.get_mut(i) {
                *slot = DocState::new(loaded.doc, Some(path)).with_versions(loaded.versions);
            }
            ok()
        }),
        spec!(
            "file.saveVersion",
            "Save to version history",
            "{name?, description?}  snapshot the document into the file's version history (kept when the file is saved); returns the version",
            false,
            save_version
        ),
        spec!("file.versionHistory", "Show version history", "{}  opens the panel; returns the versions like `version.list`", false, |s, _| {
            let list = version_list(s.ds()?);
            s.request_ui("panel.versionHistory", json!({}));
            Ok(list)
        }),
        spec!(
            "version.list",
            "List versions",
            "{}  the version history, oldest first: [{id, index, name, description, time (Unix seconds), auto?, previewing?}]",
            false,
            |s, _| Ok(version_list(s.ds()?))
        ),
        spec!(
            "version.restore",
            "Restore version",
            "{id | index}  replace the document with the version (one undo step; the file name stays)",
            true,
            restore_version
        ),
        spec!("version.rename", "Rename version", "{id | index, name, description?}", false, |s, p| {
            const CMD: &str = "version.rename";
            let name = req_string(CMD, p, "name")?;
            if name.trim().is_empty() {
                return Err(bad(CMD, "`name` can't be empty"));
            }
            let ds = s.ds_mut()?;
            let i = version_index(CMD, ds, p)?;
            let description = string(p, "description");
            let v = ds.versions_mut().get_mut(i).ok_or_else(|| bad(CMD, "no such version"))?;
            v.name = figcraft_format::clip(name.trim(), figcraft_format::MAX_VERSION_NAME);
            if let Some(d) = description {
                v.description = figcraft_format::clip(d.trim(), figcraft_format::MAX_VERSION_DESCRIPTION);
            }
            Ok(version_json(i, v))
        }),
        spec!("version.delete", "Delete version", "{id | index}", false, |s, p| {
            let ds = s.ds_mut()?;
            let i = version_index("version.delete", ds, p)?;
            let v = ds.versions_mut().remove(i);
            if ds.preview.as_ref().is_some_and(|pv| pv.id == v.id) {
                ds.end_preview();
            }
            ok()
        }),
        spec!(
            "version.preview",
            "View version",
            "{id | index}  show the version on the canvas in place of the document (read-only; `version.restore` keeps it, `version.exitPreview` goes back)",
            false,
            |s, p| {
                let ds = s.ds_mut()?;
                let i = version_index("version.preview", ds, p)?;
                let v = ds.versions.get(i).ok_or_else(|| bad("version.preview", "no such version"))?;
                let (id, name, time, out) = (v.id.clone(), v.name.clone(), v.time, version_json(i, v));
                let doc = read_version(v)?;
                ds.start_preview(&id, &name, time, doc);
                Ok(out)
            }
        ),
        spec!(
            "version.autosave",
            "Autosave version",
            "{}  save an automatic checkpoint when the document has unsaved changes since the last version (the app does this every 30 minutes); keeps the newest 20 autosaves → {saved, version?}",
            false,
            autosave
        ),
        spec!("version.exitPreview", "Back to current version", "{}  end a version preview", false, |s, _| {
            Ok(json!({"ended": s.ds_mut()?.end_preview()}))
        }),
        spec!(
            "version.duplicate",
            "Duplicate version",
            "{id | index}  open the version as a new document; returns its tab `index`",
            false,
            |s, p| {
                let ds = s.ds()?;
                let v = ds.versions.get(version_index("version.duplicate", ds, p)?).ok_or_else(|| bad("version.duplicate", "no such version"))?;
                let mut doc = (*read_version(v)?).clone();
                doc.name = format!("{} ({})", ds.doc.name, v.name);
                doc.take_dirty();
                s.docs.push(DocState::new(doc, None));
                s.active = s.docs.len() - 1;
                Ok(json!({"index": s.active}))
            }
        ),
        spec!("doc.switch", "Switch document", "{index}", false, |s, p| {
            let i = req_num("doc.switch", p, "index")? as usize;
            if i >= s.docs.len() {
                return Err(bad("doc.switch", "no such document"));
            }
            s.active = i;
            ok()
        }),
        spec!("doc.list", "List documents", "{}", false, |s, _| {
            Ok(Value::Array(
                s.docs
                    .iter()
                    .enumerate()
                    .map(|(i, d)| json!({"index": i, "name": d.doc.name, "path": d.path, "dirty": d.is_dirty(), "active": i == s.active}))
                    .collect(),
            ))
        }),
        spec!("doc.home", "Home", "{}", false, |s, _| {
            s.request_ui("home", json!({}));
            ok()
        }),
    ]
}

fn version_json(index: usize, v: &Version) -> Value {
    let mut j = json!({"id": v.id, "index": index, "name": v.name, "description": v.description, "time": v.time});
    if v.auto {
        j["auto"] = json!(true);
    }
    j
}

/// Autosaves kept per file; older ones make room for new ones.
pub const MAX_AUTOSAVES: usize = 20;

/// `version.autosave`: a checkpoint of a document with unsaved changes since the last version.
fn autosave(s: &mut Session, _: &Value) -> Result<Value> {
    let ds = s.ds_mut()?;
    if ds.preview.is_some() || !ds.is_dirty() || !ds.changed_since_checkpoint() {
        return Ok(json!({"saved": false}));
    }
    // Make room: the oldest autosave goes first; user versions are never dropped.
    let autos = ds.versions.iter().filter(|v| v.auto).count();
    if (autos >= MAX_AUTOSAVES || ds.versions.len() >= figcraft_format::MAX_VERSIONS)
        && let Some(oldest) = ds.versions.iter().position(|v| v.auto)
    {
        ds.versions_mut().remove(oldest);
    }
    if ds.versions.len() >= figcraft_format::MAX_VERSIONS {
        return Ok(json!({"saved": false}));
    }
    let mut v = snapshot_version(ds, "Autosave", "");
    v.auto = true;
    let out = version_json(ds.versions.len(), &v);
    ds.versions_mut().push(v);
    ds.mark_checkpoint();
    Ok(json!({"saved": true, "version": out}))
}

/// A version of `ds`'s document as it is now, opening on the page shown.
fn snapshot_version(ds: &DocState, name: &str, description: &str) -> Version {
    let mut snap = ds.doc.clone();
    if snap.current_page != Some(ds.page) {
        Arc::make_mut(&mut snap).current_page = Some(ds.page);
    }
    Version::new(new_version_id(&ds.versions), name, description, super::comment::now_secs(), snap)
}

fn version_list(ds: &DocState) -> Value {
    let previewing = ds.preview.as_ref().map(|p| p.id.as_str());
    Value::Array(
        ds.versions
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let mut j = version_json(i, v);
                if previewing == Some(v.id.as_str()) {
                    j["previewing"] = json!(true);
                }
                j
            })
            .collect(),
    )
}

/// The version named by `id`, else by `index` (validated: never negative, never past the end).
fn version_index(cmd: &str, ds: &DocState, p: &Value) -> Result<usize> {
    if let Some(id) = string(p, "id") {
        return ds.versions.iter().position(|v| v.id == id).ok_or_else(|| bad(cmd, format!("no version `{id}`")));
    }
    if get(p, "index").is_none() {
        return Err(bad(cmd, "pass the version's `id` or `index`"));
    }
    let i = req_index(cmd, p, "index")?;
    if i < ds.versions.len() { Ok(i) } else { Err(bad(cmd, format!("no version at index {i}"))) }
}

fn read_version(v: &Version) -> Result<Arc<Document>> {
    v.document().map_err(|e| EngineError::Other(format!("version “{}” can't be read: {e}", v.name)))
}

/// A new version id: the time in milliseconds (hex), unique in the history.
fn new_version_id(versions: &[Version]) -> String {
    let mut n = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    loop {
        let id = format!("{n:x}");
        if !versions.iter().any(|v| v.id == id) {
            return id;
        }
        n = n.wrapping_add(1);
    }
}

fn save_version(s: &mut Session, p: &Value) -> Result<Value> {
    let ds = s.ds_mut()?;
    if ds.versions.len() >= figcraft_format::MAX_VERSIONS {
        return Err(EngineError::Disabled(
            "file.saveVersion".into(),
            format!("the version history is full ({} versions): delete old versions first", figcraft_format::MAX_VERSIONS),
        ));
    }
    let name =
        string(p, "name").map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Version {}", ds.versions.len() + 1));
    let description = string(p, "description").unwrap_or_default();
    // The version opens on the page shown when it was saved.
    let v = snapshot_version(ds, &name, description.trim());
    let out = version_json(ds.versions.len(), &v);
    ds.versions_mut().push(v);
    ds.mark_checkpoint();
    Ok(out)
}

fn restore_version(s: &mut Session, p: &Value) -> Result<Value> {
    let ds = s.ds_mut()?;
    let i = version_index("version.restore", ds, p)?;
    let v = ds.versions.get(i).ok_or_else(|| bad("version.restore", "no such version"))?;
    let mut doc = read_version(v)?;
    let out = version_json(i, v);
    let d = Arc::make_mut(&mut doc);
    d.name = ds.doc.name.clone();
    // What the file published belongs to the file, not to the version.
    d.published = ds.doc.published.clone();
    // The snapshot is already consistent: nothing to re-normalise.
    d.take_dirty();
    // Stay on the current page when the version has it.
    ds.page = Some(ds.page)
        .filter(|pg| d.pages.contains(pg))
        .or_else(|| d.current_page.filter(|pg| d.pages.contains(pg)))
        .or_else(|| d.first_page())
        .unwrap_or(ds.page);
    ds.doc = doc;
    ds.selection.clear();
    ds.text_edit = None;
    ds.vector_edit = None;
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn read(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}

#[cfg(target_arch = "wasm32")]
fn read(path: &str) -> Result<Vec<u8>> {
    Err(EngineError::Other(format!("{path}: no file system in the browser")))
}

#[cfg(not(target_arch = "wasm32"))]
fn write(path: &str, bytes: &[u8]) -> Result<()> {
    let tmp = format!("{path}.tmp-figcraft");
    std::fs::write(&tmp, bytes).map_err(|e| EngineError::Other(format!("{tmp}: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}

#[cfg(target_arch = "wasm32")]
fn write(path: &str, _bytes: &[u8]) -> Result<()> {
    Err(EngineError::Other(format!("{path}: no file system in the browser")))
}

fn ext(path: &str) -> String {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().and_then(|e| e.to_str()).unwrap_or("Untitled").to_string()
}

/// Load a document from any supported file (REST API file JSON images come from an `images`
/// folder next to the file).
pub fn load(path: &str) -> Result<(Document, Vec<String>)> {
    load_path(path).map(|(l, _)| (l.doc, l.issues))
}

pub fn load_bytes(bytes: &[u8], name_hint: &str) -> Result<(Document, Vec<String>)> {
    load_with(bytes, name_hint, &figcraft_format::rest::no_images).map(|(l, _)| (l.doc, l.issues))
}

/// [`load`] with the version history, also telling whether the file was REST API file JSON.
pub fn load_path(path: &str) -> Result<(figcraft_format::Loaded, bool)> {
    let bytes = read(path)?;
    load_with(&bytes, path, &rest_images(path))
}

/// Load from bytes. Returns the document with its notes (repairs of a native file, or import
/// warnings) and version history, and whether it was imported from REST API file JSON (such a file
/// is never saved over).
fn load_with(bytes: &[u8], name_hint: &str, images: figcraft_format::rest::ImageResolver) -> Result<(figcraft_format::Loaded, bool)> {
    match ext(name_hint).as_str() {
        "svg" => {
            let mut doc = Document::new(file_stem(name_hint));
            let page = doc.first_page().ok_or(EngineError::NoDocument)?;
            figcraft_svg::import_svg(&mut doc, bytes, page, 0.0, 0.0, &file_stem(name_hint)).map_err(|e| EngineError::Other(e.to_string()))?;
            Ok((doc.into(), false))
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => {
            let mut doc = Document::new(file_stem(name_hint));
            let page = doc.first_page().ok_or(EngineError::NoDocument)?;
            let (hash, w, h) = figcraft_format::add_image(&mut doc, bytes.to_vec()).map_err(|e| EngineError::Other(e.to_string()))?;
            let id = doc.create(page, NodeKind::Rectangle, &file_stem(name_hint), 0.0, 0.0, f64::from(w), f64::from(h))?;
            doc.get_mut(id)?.fills = vec![figcraft_color::Paint::image(hash, figcraft_color::ImageScaleMode::Fill)];
            Ok((doc.into(), false))
        }
        _ => {
            if let Some(r) = figcraft_format::rest::try_import(bytes, images) {
                let (mut doc, warnings) = r.map_err(|e| EngineError::Other(e.to_string()))?;
                link_instances(&mut doc);
                return Ok((figcraft_format::Loaded { issues: warnings, ..doc.into() }, true));
            }
            let mut loaded = figcraft_format::load_bytes_with_versions(bytes).map_err(|e| EngineError::Other(e.to_string()))?;
            if loaded.doc.name.is_empty() {
                loaded.doc.name = file_stem(name_hint);
            }
            Ok((loaded, false))
        }
    }
}

/// Link imported instances to their mains (layer sources, overrides found by comparing values),
/// innermost first so outer instances settle last.
fn link_instances(doc: &mut Document) {
    let mut insts: Vec<(usize, NodeId)> =
        doc.nodes().filter(|n| matches!(n.kind, NodeKind::Instance { .. })).map(|n| (doc.ancestors(n.id).len(), n.id)).collect();
    insts.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, i) in insts {
        crate::components::rebase_instance(doc, i);
    }
}

/// Largest image file read for a REST API file JSON import.
#[cfg(not(target_arch = "wasm32"))]
const MAX_IMAGE_FILE: u64 = 256 * 1024 * 1024;

/// Image bytes for a REST API file JSON's `imageRef`: `images/<imageRef>[.png|.jpg|…]` next to
/// the file.
#[cfg(not(target_arch = "wasm32"))]
fn rest_images(path: &str) -> impl Fn(&str) -> Option<Vec<u8>> {
    let dir = std::path::Path::new(path).parent().map(|d| d.join("images"));
    move |r: &str| {
        // Image refs are content hashes; anything else never becomes a path.
        if r.is_empty() || r.len() > 128 || !r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return None;
        }
        let dir = dir.as_ref()?;
        ["", ".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().find_map(|e| {
            let p = dir.join(format!("{r}{e}"));
            let meta = std::fs::metadata(&p).ok()?;
            if meta.is_file() && meta.len() <= MAX_IMAGE_FILE { std::fs::read(&p).ok() } else { None }
        })
    }
}

#[cfg(target_arch = "wasm32")]
fn rest_images(_path: &str) -> impl Fn(&str) -> Option<Vec<u8>> {
    figcraft_format::rest::no_images
}

fn open(s: &mut Session, p: &Value) -> Result<Value> {
    let path = req_string("file.open", p, "path")?;
    // Already open: switch to it.
    if let Some(i) = s.docs.iter().position(|d| d.path.as_deref() == Some(path.as_str())) {
        s.active = i;
        return Ok(json!({"index": i}));
    }
    let (loaded, rest) = load_path(&path)?;
    let native = !rest && matches!(ext(&path).as_str(), "figcraft" | "json");
    s.docs.push(DocState::new(loaded.doc, native.then_some(path)).with_versions(loaded.versions));
    s.active = s.docs.len() - 1;
    let mut out = json!({"index": s.active});
    out[if rest { "warnings" } else { "repairs" }] = json!(loaded.issues);
    Ok(out)
}

/// Thumbnail PNG of the first page's content (for file browsers / the home screen).
pub fn thumbnail(doc: &Document) -> Option<Vec<u8>> {
    let page = doc.first_page()?;
    let b = doc.page_content_bounds(page)?;
    let scale = (400.0 / b.width().max(b.height()).max(1.0)).min(2.0);
    let view = figcraft_render::View {
        origin: (b.x0, b.y0),
        scale,
        width: (b.width() * scale).ceil().max(1.0) as u32,
        height: (b.height() * scale).ceil().max(1.0) as u32,
    };
    let pix = figcraft_render::render_view(doc, page, &view, &Default::default())?;
    figcraft_render::encode_png(&pix).ok()
}

/// The `.figcraft` file for a document: content, thumbnail and version history.
pub fn file_bytes(ds: &DocState) -> Result<Vec<u8>> {
    let mut doc = (*ds.doc).clone();
    doc.current_page = Some(ds.page);
    let thumb = thumbnail(&doc);
    figcraft_format::save_bytes_with_versions(&doc, thumb.as_deref(), &ds.versions).map_err(|e| EngineError::Other(e.to_string()))
}

fn save_to(s: &mut Session, path: String) -> Result<Value> {
    let ds = s.ds_mut()?;
    let bytes = file_bytes(ds)?;
    write(&path, &bytes)?;
    ds.path = Some(path.clone());
    ds.mark_saved();
    ds.revision += 1;
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(path) = string(p, "path") {
        return save_to(s, path);
    }
    match s.ds()?.path.clone() {
        Some(path) => save_to(s, path),
        None => {
            s.request_ui("dialog.saveAs", json!({}));
            Err(EngineError::Disabled("file.save".into(), "the file has no path yet: pass `path`".into()))
        }
    }
}

fn save_as(s: &mut Session, p: &Value) -> Result<Value> {
    match string(p, "path") {
        Some(mut path) => {
            if ext(&path).is_empty() {
                path.push_str(".figcraft");
            }
            save_to(s, path)
        }
        None => {
            s.request_ui("dialog.saveAs", json!({}));
            Err(EngineError::Disabled("file.saveAs".into(), "pass `path`".into()))
        }
    }
}

/// Encode `ids` (or the page) in `format` at `scale`.
pub fn export_bytes(doc: &Document, ids: &[NodeId], format: ExportFormat, scale: f64) -> Result<Vec<u8>> {
    if ids.is_empty() {
        return Err(EngineError::Disabled("file.export".into(), "nothing to export".into()));
    }
    match format {
        ExportFormat::Svg => figcraft_svg::export_svg(doc, ids, &Default::default())
            .map(String::into_bytes)
            .ok_or_else(|| EngineError::Other("nothing to export".into())),
        ExportFormat::Pdf => {
            // Frames export one page each; anything else shares one page.
            let all_frames = ids.iter().all(|i| doc.node(*i).is_some_and(|n| n.kind.is_frame_like()));
            let r = if all_frames {
                figcraft_pdf::export_pdf(doc, ids, &figcraft_pdf::PdfOptions::default())
            } else {
                figcraft_pdf::export_nodes_pdf(doc, &super::edit::z_sorted(doc, ids))
            };
            r.map_err(|e| EngineError::Other(e.to_string()))
        }
        _ => {
            let pix = if ids.len() == 1 {
                ids.first().and_then(|i| figcraft_render::render_node(doc, *i, scale))
            } else {
                let b = ids.iter().filter_map(|i| figcraft_render::export_bounds(doc, *i)).reduce(|a, b| a.union(b));
                b.and_then(|b| {
                    let view = figcraft_render::View {
                        origin: (b.x0, b.y0),
                        scale,
                        width: (b.width() * scale).ceil().max(1.0) as u32,
                        height: (b.height() * scale).ceil().max(1.0) as u32,
                    };
                    let mut pix = figcraft_render::Pixmap::new(view.width.clamp(1, 16384), view.height.clamp(1, 16384))?;
                    let ordered = super::edit::z_sorted(doc, ids);
                    figcraft_render::render_nodes_into(doc, &ordered, &view, &mut pix);
                    Some(pix)
                })
            };
            let pix = pix.ok_or_else(|| EngineError::Other("render failed (empty or too large)".into()))?;
            match format {
                ExportFormat::Jpg => figcraft_render::encode_jpeg(&pix, figcraft_color::Color::WHITE, 90),
                ExportFormat::Webp => figcraft_render::encode_webp(&pix),
                _ => figcraft_render::encode_png(&pix),
            }
            .map_err(EngineError::Other)
        }
    }
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let doc = s.doc()?;
    let mut ids = targets(s, p);
    if ids.is_empty() {
        ids = doc.get(s.page()?)?.children.clone();
    }
    let format = string(p, "format")
        .and_then(|f| ExportFormat::from_ext(&f))
        .or_else(|| string(p, "path").and_then(|pp| ExportFormat::from_ext(&ext(&pp))))
        .unwrap_or_default();
    let scale = num_or(p, "scale", 1.0).clamp(0.01, 64.0);
    let bytes = export_bytes(doc, &ids, format, scale)?;
    match string(p, "path") {
        Some(path) => {
            write(&path, &bytes)?;
            Ok(json!({"path": path, "bytes": bytes.len(), "format": format.ext()}))
        }
        None => Ok(json!({"format": format.ext(), "data": base64::engine::general_purpose::STANDARD.encode(&bytes)})),
    }
}

/// Every top-level frame of a page (reading order), one PDF page each.
fn export_frames_pdf(s: &mut Session, p: &Value) -> Result<Value> {
    let doc = s.doc()?;
    let page = match id(p, "page") {
        Some(pg) if doc.node(pg).is_some_and(Node::is_page) => pg,
        Some(_) => return Err(bad("file.exportFramesPdf", "`page` is not a page")),
        None => s.page()?,
    };
    let frames = doc.top_frames(page);
    if frames.is_empty() {
        return Err(EngineError::Disabled("file.exportFramesPdf".into(), "the page has no frames".into()));
    }
    let title = Some(doc.name.clone());
    let bytes = figcraft_pdf::export_pdf(doc, &frames, &figcraft_pdf::PdfOptions { title, ..Default::default() })
        .map_err(|e| EngineError::Other(e.to_string()))?;
    match string(p, "path") {
        Some(path) => {
            write(&path, &bytes)?;
            Ok(json!({"path": path, "bytes": bytes.len(), "pages": frames.len()}))
        }
        None => Ok(json!({"format": "pdf", "pages": frames.len(), "data": base64::engine::general_purpose::STANDARD.encode(&bytes)})),
    }
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let path = string(p, "path");
    let (bytes, name) = if let Some(path) = &path {
        (read(path)?, path.clone())
    } else if let Some(d) = string(p, "data") {
        let d = d.split_once(',').map(|(_, b)| b.to_string()).unwrap_or(d);
        (
            base64::engine::general_purpose::STANDARD.decode(d.trim()).map_err(|e| bad("file.import", e.to_string()))?,
            string(p, "name").unwrap_or_else(|| "import.png".into()),
        )
    } else {
        return Err(bad("file.import", "pass `path` or `data`"));
    };
    let page = s.page()?;
    let (x, y) = (num(p, "x"), num(p, "y"));
    let center = s.ds().map(|d| (d.view.cx, d.view.cy)).unwrap_or((0.0, 0.0));
    match ext(&name).as_str() {
        "svg" => {
            let doc = s.doc_mut()?;
            let id = figcraft_svg::import_svg(doc, &bytes, page, x.unwrap_or(center.0), y.unwrap_or(center.1), &file_stem(&name))
                .map_err(|e| EngineError::Other(e.to_string()))?;
            s.set_selection(vec![id]);
            Ok(json!({"id": id}))
        }
        "figcraft" | "json" => {
            let resolver = path.as_deref().map(rest_images);
            let images: figcraft_format::rest::ImageResolver = match &resolver {
                Some(r) => r,
                None => &figcraft_format::rest::no_images,
            };
            let (other, rest) = load_with(&bytes, &name, images)?;
            let pages = append_pages(s.doc_mut()?, &other.doc)?;
            let mut out = json!({"pages": pages});
            if rest {
                out["warnings"] = json!(other.issues);
            }
            Ok(out)
        }
        _ => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            super::create::place_image(s, &json!({"data": b64, "x": x.unwrap_or(center.0), "y": y.unwrap_or(center.1), "name": file_stem(&name)}))
        }
    }
}

/// Append `other`'s pages (with their backgrounds, its images and styles) to `doc`. Ids are
/// remapped across all pages at once, so instances keep their main and instance layers their
/// source wherever those are.
fn append_pages(doc: &mut Document, other: &Document) -> Result<Vec<NodeId>> {
    for (k, v) in &other.images {
        doc.images.entry(k.clone()).or_insert_with(|| v.clone());
    }
    for st in &other.styles {
        if !doc.styles.iter().any(|s| s.id == st.id) {
            doc.styles.push(st.clone());
        }
    }
    let mut map = std::collections::BTreeMap::new();
    for pg in &other.pages {
        for id in other.subtree(*pg).into_iter().skip(1) {
            map.insert(id, doc.alloc_id());
        }
    }
    let mut new_pages = Vec::new();
    for pg in &other.pages {
        let Some(pn) = other.node(*pg) else { continue };
        let np = doc.add_page(&pn.name);
        if let (NodeKind::Page { background: from, .. }, Some(NodeKind::Page { background, .. })) = (&pn.kind, doc.node_mut(np).map(|n| &mut n.kind))
        {
            *background = *from;
        }
        for c in &pn.children {
            let nodes: Vec<Node> = other.subtree(*c).into_iter().filter_map(|i| other.node(i).cloned()).collect();
            doc.paste_nodes(&nodes, &map, *c, np, None)?;
        }
        new_pages.push(np);
    }
    for new in map.values() {
        let src = doc.node(*new).and_then(|n| n.source).and_then(|s| map.get(&s).copied());
        if let (Some(src), Some(n)) = (src, doc.node_mut(*new)) {
            n.source = Some(src);
        }
    }
    crate::library::merge_store(doc, other, &map)?;
    Ok(new_pages)
}

fn close(s: &mut Session, p: &Value) -> Result<Value> {
    let i = num(p, "index").map(|x| x as usize).unwrap_or(s.active);
    let Some(ds) = s.docs.get(i) else { return Err(bad("file.close", "no such document")) };
    if ds.is_dirty() && !boolean(p, "force").unwrap_or(false) {
        s.request_ui("dialog.unsaved", json!({"index": i}));
        return Err(EngineError::Disabled("file.close".into(), "the document has unsaved changes (pass force: true to discard)".into()));
    }
    s.docs.remove(i);
    if s.active >= s.docs.len() {
        s.active = s.docs.len().saturating_sub(1);
    }
    ok()
}

#[cfg(test)]
mod rest_import_tests {
    use super::*;

    /// A synthetic REST API file JSON: a component with a text layer, an instance of it whose text
    /// is overridden, and an image fill.
    fn kit() -> Value {
        let at = |x: f64, y: f64| json!([[1, 0, x], [0, 1, y]]);
        let label = |id: &str, text: &str| {
            json!({"id": id, "name": "Label", "type": "TEXT", "relativeTransform": at(8.0, 8.0), "size": {"x": 40, "y": 20}, "characters": text,
                   "style": {"fontFamily": "Inter", "fontSize": 14}, "fills": [{"type": "SOLID", "color": {"r": 0, "g": 0, "b": 0, "a": 1}}]})
        };
        json!({"name": "Kit", "document": {"id": "0:0", "type": "DOCUMENT", "children": [
            {"id": "0:1", "name": "Main", "type": "CANVAS", "backgroundColor": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1}, "children": [
                {"id": "1:1", "name": "Button", "type": "COMPONENT", "relativeTransform": at(0.0, 0.0), "size": {"x": 100, "y": 40},
                 "fills": [{"type": "SOLID", "color": {"r": 0.2, "g": 0.4, "b": 1, "a": 1}}], "children": [label("1:2", "OK")]},
                {"id": "2:1", "name": "Button", "type": "INSTANCE", "componentId": "1:1", "relativeTransform": at(0.0, 100.0), "size": {"x": 100, "y": 40},
                 "fills": [{"type": "SOLID", "color": {"r": 0.2, "g": 0.4, "b": 1, "a": 1}}], "children": [label("I2:1;1:2", "Go")]},
                {"id": "3:1", "name": "Photo", "type": "RECTANGLE", "relativeTransform": at(200.0, 0.0), "size": {"x": 30, "y": 20},
                 "fills": [{"type": "IMAGE", "imageRef": "photo1", "scaleMode": "FILL"}]}
            ]}
        ]}, "components": {"1:1": {"key": "k", "name": "Button", "description": ""}}, "styles": {}})
    }

    fn instance(doc: &Document) -> &Node {
        doc.nodes().find(|n| matches!(n.kind, NodeKind::Instance { .. })).unwrap()
    }

    #[test]
    fn rest_json_loads_with_linked_instances() {
        let bytes = serde_json::to_vec(&kit()).unwrap();
        let (doc, warnings) = load_bytes(&bytes, "kit.json").unwrap();
        assert_eq!(doc.name, "Kit");
        assert!(warnings.iter().any(|w| w.contains("placeholder")), "{warnings:?}");
        let inst = instance(&doc);
        let NodeKind::Instance { component, .. } = inst.kind else { panic!() };
        assert_eq!(inst.source, Some(component));
        let main_label = doc.node(component).unwrap().children[0];
        let label = doc.node(inst.children[0]).unwrap();
        assert_eq!(label.source, Some(main_label));
        assert!(label.overrides.contains("characters"), "{:?}", label.overrides);
        assert!(crate::components::mirror_issues(&doc).is_empty());
        assert!(thumbnail(&doc).is_some());
    }

    #[test]
    fn open_and_import_rest_json() {
        let dir = std::env::temp_dir().join(format!("figcraft-rest-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("images")).unwrap();
        let png = {
            let mut d = Document::new("x");
            let p = d.first_page().unwrap();
            let id = d.create(p, NodeKind::Rectangle, "R", 0.0, 0.0, 3.0, 2.0).unwrap();
            export_bytes(&d, &[id], ExportFormat::Png, 1.0).unwrap()
        };
        std::fs::write(dir.join("images").join("photo1.png"), &png).unwrap();
        let path = dir.join("kit.json");
        std::fs::write(&path, serde_json::to_vec(&kit()).unwrap()).unwrap();
        let path = path.to_string_lossy().to_string();

        let mut s = Session::new();
        let r = s.execute("file.open", &json!({"path": path})).unwrap();
        assert!(r["warnings"].as_array().is_some_and(|w| w.iter().all(|w| !w.as_str().unwrap_or("").contains("placeholder"))), "{r}");
        assert!(r.get("repairs").is_none());
        assert_eq!(s.ds().unwrap().path, None, "an import is never saved over its JSON");
        let doc = s.doc().unwrap();
        let photo = doc.nodes().find(|n| n.name == "Photo").unwrap();
        let figcraft_color::PaintKind::Image(img) = &photo.fills[0].kind else { panic!() };
        assert_eq!(doc.images.get(&img.image).map(|i| (i.width, i.height)), Some((3, 2)), "read from images/ next to the file");

        // Import into another document: pages are appended and references follow the new ids.
        let mut s = Session::new();
        let data = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&kit()).unwrap());
        let r = s.execute("file.import", &json!({"data": data, "name": "kit.json"})).unwrap();
        assert_eq!(r["pages"].as_array().map(Vec::len), Some(1));
        assert!(r["warnings"].is_array(), "{r}");
        let doc = s.doc().unwrap();
        assert_eq!(doc.pages.len(), 2);
        let page = doc.node(doc.pages[1]).unwrap();
        assert!(matches!(page.kind, NodeKind::Page { background, .. } if (background.r - 0.9).abs() < 1e-6));
        let inst = instance(doc);
        let NodeKind::Instance { component, .. } = inst.kind else { panic!() };
        assert!(doc.node(component).is_some_and(|c| matches!(c.kind, NodeKind::Component { .. })));
        let label = doc.node(inst.children[0]).unwrap();
        assert!(label.source.is_some_and(|src| doc.is_ancestor(component, src)));
        assert!(crate::components::mirror_issues(doc).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
