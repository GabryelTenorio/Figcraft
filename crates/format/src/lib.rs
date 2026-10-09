//! The native `.figcraft` format.
//!
//! A zip archive holding `document.json` (the [`Document`] as JSON, with a `format`/`version`
//! header), `images/<sha256>` for every image the document (or a saved version) uses, an optional
//! `thumbnail.png` and an optional version history: `versions/index.json` (`{"versions": [{id,
//! name, description, time, images}]}`, oldest first) plus one `versions/<id>.json` snapshot per
//! version (a `document.json` of its own). Readers that don't know about versions ignore those
//! entries. Plain JSON (the `document.json` alone) also opens. Reading is lenient: structural
//! problems are repaired (see [`Document::repair`]) and reported, damaged versions are skipped.
//!
//! [`rest`] imports the public REST API file JSON.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod rest;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, Write};
use std::sync::Arc;

use figcraft_doc::{Document, ImageData};
use serde_json::{Value, json};
use sha2::Digest;

pub const EXTENSION: &str = "figcraft";
pub const FORMAT: &str = "figcraft";
pub const VERSION: u32 = 1;
/// Largest archive member we read (guards against zip bombs).
const MAX_MEMBER: u64 = 512 * 1024 * 1024;
/// Most versions a file keeps: saving more is refused, extra entries in a file are skipped.
pub const MAX_VERSIONS: usize = 100;
/// Longest version name and description kept (chars).
pub const MAX_VERSION_NAME: usize = 200;
pub const MAX_VERSION_DESCRIPTION: usize = 2000;
/// Largest version index and total size of the version snapshots read from one file.
const MAX_VERSION_INDEX: u64 = 16 * 1024 * 1024;
const MAX_VERSION_BYTES: u64 = 1024 * 1024 * 1024;
const VERSION_INDEX: &str = "versions/index.json";

#[derive(Debug)]
pub enum FormatError {
    Io(String),
    Corrupt(String),
    Newer(u32),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "{e}"),
            FormatError::Corrupt(e) => write!(f, "not a valid FigCraft file: {e}"),
            FormatError::Newer(v) => write!(f, "this file was saved by a newer FigCraft (format version {v})"),
        }
    }
}

impl std::error::Error for FormatError {}

/// One entry of a file's version history: a named snapshot of the document.
#[derive(Clone, Debug)]
pub struct Version {
    /// Unique within the file; ASCII letters, digits, `-` and `_` (see [`is_valid_version_id`]).
    pub id: String,
    pub name: String,
    pub description: String,
    /// When it was saved (Unix seconds).
    pub time: u64,
    snapshot: Snapshot,
}

#[derive(Clone, Debug)]
enum Snapshot {
    /// Saved in this session (or already read).
    Doc(Arc<Document>),
    /// As read from a file: its `document.json`-style JSON, parsed on first use, and the images
    /// it uses.
    Stored { json: Arc<Vec<u8>>, images: BTreeMap<String, ImageData> },
}

impl Version {
    /// A version of `doc`. The name and description are clipped to their maximum lengths.
    pub fn new(id: impl Into<String>, name: &str, description: &str, time: u64, doc: Arc<Document>) -> Self {
        Self {
            id: id.into(),
            name: clip(name, MAX_VERSION_NAME),
            description: clip(description, MAX_VERSION_DESCRIPTION),
            time,
            snapshot: Snapshot::Doc(doc),
        }
    }

    /// The snapshot (with its images). Versions read from a file are parsed here, so a damaged
    /// one fails now rather than when the file opens.
    pub fn document(&self) -> Result<Arc<Document>, FormatError> {
        match &self.snapshot {
            Snapshot::Doc(d) => Ok(d.clone()),
            Snapshot::Stored { json, images } => {
                let text = std::str::from_utf8(json).map_err(|e| FormatError::Corrupt(e.to_string()))?;
                let (mut doc, _) = from_json(text)?;
                doc.images = images.clone();
                Ok(Arc::new(doc))
            }
        }
    }

    /// The images the snapshot uses, by hash.
    pub fn images(&self) -> &BTreeMap<String, ImageData> {
        match &self.snapshot {
            Snapshot::Doc(d) => &d.images,
            Snapshot::Stored { images, .. } => images,
        }
    }
}

/// Whether `id` can name a version (and its archive entry).
pub fn is_valid_version_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `s` cut to at most `max` chars.
pub fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A file as read: the document, the repairs made (and versions skipped), and its version
/// history, oldest first.
#[derive(Debug)]
pub struct Loaded {
    pub doc: Document,
    pub issues: Vec<String>,
    pub versions: Vec<Version>,
}

impl From<Document> for Loaded {
    fn from(doc: Document) -> Self {
        Self { doc, issues: Vec::new(), versions: Vec::new() }
    }
}

/// Content hash used as an image's key.
pub fn image_hash(bytes: &[u8]) -> String {
    let d = sha2::Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Register encoded image bytes with the document; returns the hash and pixel size.
pub fn add_image(doc: &mut Document, bytes: Vec<u8>) -> Result<(String, u32, u32), FormatError> {
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| FormatError::Corrupt(e.to_string()))?
        .into_dimensions()
        .map_err(|e| FormatError::Corrupt(format!("unsupported image: {e}")))?;
    let hash = image_hash(&bytes);
    doc.images.insert(hash.clone(), ImageData { bytes: Arc::new(bytes), width: w, height: h });
    Ok((hash, w, h))
}

/// Serialize the document (without images) to the JSON value stored in `document.json`.
pub fn to_json(doc: &Document) -> Result<Value, FormatError> {
    let mut v = serde_json::to_value(doc).map_err(|e| FormatError::Io(e.to_string()))?;
    if let Some(o) = v.as_object_mut() {
        o.insert("format".into(), json!(FORMAT));
        o.insert("version".into(), json!(VERSION));
        let images: Vec<Value> = doc.images.iter().map(|(k, i)| json!({"hash": k, "width": i.width, "height": i.height})).collect();
        o.insert("imageIndex".into(), Value::Array(images));
    }
    Ok(v)
}

/// Parse `document.json` content.
pub fn from_json(text: &str) -> Result<(Document, Vec<String>), FormatError> {
    parse_json(text).map(|(doc, issues, _)| (doc, issues))
}

/// The image hashes a `document.json` lists in its `imageIndex` (`None` without one).
type ImageIndex = Option<BTreeSet<String>>;

/// Parse `document.json` content; also returns its [`ImageIndex`].
fn parse_json(text: &str) -> Result<(Document, Vec<String>, ImageIndex), FormatError> {
    let v: Value = serde_json::from_str(text).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    if let Some(ver) = v.get("version").and_then(Value::as_u64)
        && ver > u64::from(VERSION)
    {
        return Err(FormatError::Newer(ver as u32));
    }
    let index = v
        .get("imageIndex")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|i| i.get("hash").and_then(Value::as_str).map(str::to_string)).collect());
    let mut doc: Document = serde_json::from_value(v).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    let issues = doc.validate();
    if !issues.is_empty() {
        doc.repair();
    }
    Ok((doc, issues, index))
}

fn io_err(e: impl std::fmt::Display) -> FormatError {
    FormatError::Io(e.to_string())
}

/// Add one member to the archive.
fn put<W: Write + Seek>(z: &mut zip::ZipWriter<W>, name: &str, opts: zip::write::SimpleFileOptions, bytes: &[u8]) -> Result<(), FormatError> {
    z.start_file(name, opts).map_err(io_err)?;
    z.write_all(bytes).map_err(io_err)
}

/// Write a `.figcraft` archive to bytes.
pub fn save_bytes(doc: &Document, thumbnail_png: Option<&[u8]>) -> Result<Vec<u8>, FormatError> {
    save_bytes_with_versions(doc, thumbnail_png, &[])
}

/// Write a `.figcraft` archive with a version history (oldest first) to bytes.
pub fn save_bytes_with_versions(doc: &Document, thumbnail_png: Option<&[u8]>, versions: &[Version]) -> Result<Vec<u8>, FormatError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        put(&mut z, "document.json", deflated, &serde_json::to_vec(&to_json(doc)?).map_err(io_err)?)?;
        // Every image once, whether the document or a version uses it.
        let mut written = BTreeSet::new();
        for (hash, img) in std::iter::once(&doc.images).chain(versions.iter().map(Version::images)).flatten() {
            if written.insert(hash) {
                put(&mut z, &format!("images/{hash}"), stored, &img.bytes)?;
            }
        }
        if let Some(t) = thumbnail_png {
            put(&mut z, "thumbnail.png", stored, t)?;
        }
        if !versions.is_empty() {
            let mut ids = BTreeSet::new();
            let mut index = Vec::with_capacity(versions.len());
            for v in versions {
                if !is_valid_version_id(&v.id) || !ids.insert(v.id.as_str()) {
                    return Err(FormatError::Io(format!("invalid or duplicate version id `{}`", v.id)));
                }
                let name = format!("versions/{}.json", v.id);
                match &v.snapshot {
                    Snapshot::Doc(d) => put(&mut z, &name, deflated, &serde_json::to_vec(&to_json(d)?).map_err(io_err)?)?,
                    Snapshot::Stored { json, .. } => put(&mut z, &name, deflated, json)?,
                }
                let images: Vec<&String> = v.images().keys().collect();
                index.push(json!({"id": v.id, "name": v.name, "description": v.description, "time": v.time, "images": images}));
            }
            put(&mut z, VERSION_INDEX, deflated, &serde_json::to_vec(&json!({"versions": index})).map_err(io_err)?)?;
        }
        z.finish().map_err(io_err)?;
    }
    Ok(buf.into_inner())
}

/// Read a `.figcraft` archive (or bare JSON) from bytes. Returns the document and any repairs made.
pub fn load_bytes(bytes: &[u8]) -> Result<(Document, Vec<String>), FormatError> {
    load_bytes_with_versions(bytes).map(|l| (l.doc, l.issues))
}

/// Read archive member `name`, refusing members larger than `limit` bytes.
fn read_member<R: Read + Seek>(z: &mut zip::ZipArchive<R>, name: &str, limit: u64) -> Result<Vec<u8>, FormatError> {
    let f = z.by_name(name).map_err(|e| FormatError::Corrupt(format!("{name}: {e}")))?;
    let too_large = || FormatError::Corrupt(format!("{name} is too large"));
    if f.size() > limit {
        return Err(too_large());
    }
    let mut data = Vec::new();
    f.take(limit.saturating_add(1)).read_to_end(&mut data).map_err(|e| FormatError::Corrupt(format!("{name}: {e}")))?;
    if data.len() as u64 > limit {
        return Err(too_large());
    }
    Ok(data)
}

/// Read a `.figcraft` archive (or bare JSON) with its version history.
pub fn load_bytes_with_versions(bytes: &[u8]) -> Result<Loaded, FormatError> {
    let trimmed = bytes.iter().position(|b| !b.is_ascii_whitespace()).and_then(|i| bytes.get(i));
    if trimmed == Some(&b'{') {
        let text = std::str::from_utf8(bytes).map_err(|e| FormatError::Corrupt(e.to_string()))?;
        let (doc, issues) = from_json(text)?;
        return Ok(Loaded { issues, ..Loaded::from(doc) });
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    let text = String::from_utf8(read_member(&mut z, "document.json", MAX_MEMBER)?).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    let (mut doc, mut issues, own_images) = parse_json(&text)?;
    let names: Vec<String> = z.file_names().filter(|n| n.starts_with("images/")).map(str::to_string).collect();
    let mut images = BTreeMap::new();
    for name in names {
        let Ok(data) = read_member(&mut z, &name, MAX_MEMBER) else { continue };
        let hash = name.trim_start_matches("images/").to_string();
        let (w, h) =
            image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok()).unwrap_or((0, 0));
        images.insert(hash, ImageData { bytes: Arc::new(data), width: w, height: h });
    }
    let versions = read_versions(&mut z, &images, &mut issues);
    // Images only the version history uses stay with the versions.
    let version_only: BTreeSet<&String> =
        versions.iter().flat_map(|v| v.images().keys()).filter(|h| own_images.as_ref().is_some_and(|o| !o.contains(*h))).collect();
    doc.images = images.into_iter().filter(|(h, _)| !version_only.contains(h)).collect();
    Ok(Loaded { doc, issues, versions })
}

/// The version history of an archive (none when it has no index). Damaged entries are skipped and
/// reported in `issues`; nothing here fails the load.
fn read_versions<R: Read + Seek>(z: &mut zip::ZipArchive<R>, images: &BTreeMap<String, ImageData>, issues: &mut Vec<String>) -> Vec<Version> {
    if z.index_for_name(VERSION_INDEX).is_none() {
        return Vec::new();
    }
    let index = match read_member(z, VERSION_INDEX, MAX_VERSION_INDEX)
        .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| FormatError::Corrupt(e.to_string())))
    {
        Ok(v) => v,
        Err(e) => {
            issues.push(format!("the version history was skipped: {e}"));
            return Vec::new();
        }
    };
    let entries = index.get("versions").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let mut out: Vec<Version> = Vec::new();
    let mut budget = MAX_VERSION_BYTES;
    for e in entries {
        if out.len() >= MAX_VERSIONS {
            issues.push(format!("only the first {MAX_VERSIONS} versions were kept"));
            break;
        }
        let id = e.get("id").and_then(Value::as_str).unwrap_or_default();
        if !is_valid_version_id(id) || out.iter().any(|v| v.id == id) {
            issues.push("a version with an invalid id was skipped".into());
            continue;
        }
        let json = match read_member(z, &format!("versions/{id}.json"), budget.min(MAX_MEMBER)) {
            Ok(j) => j,
            Err(err) => {
                issues.push(format!("version `{id}` was skipped: {err}"));
                continue;
            }
        };
        budget = budget.saturating_sub(json.len() as u64);
        let text = |k: &str, max: usize| e.get(k).and_then(Value::as_str).map(|s| clip(s, max)).unwrap_or_default();
        let images = e
            .get("images")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|h| images.get(h).map(|i| (h.to_string(), i.clone())))
            .collect();
        out.push(Version {
            id: id.to_string(),
            name: text("name", MAX_VERSION_NAME),
            description: text("description", MAX_VERSION_DESCRIPTION),
            time: e.get("time").and_then(Value::as_u64).unwrap_or(0),
            snapshot: Snapshot::Stored { json: Arc::new(json), images },
        });
    }
    out
}

/// Save atomically: write a temp file next to `path`, then rename over it.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_file(doc: &Document, path: &std::path::Path, thumbnail_png: Option<&[u8]>) -> Result<(), FormatError> {
    let bytes = save_bytes(doc, thumbnail_png)?;
    let tmp = path.with_extension("figcraft.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| FormatError::Io(format!("{}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| FormatError::Io(format!("{}: {e}", path.display())))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_file(path: &std::path::Path) -> Result<(Document, Vec<String>), FormatError> {
    let bytes = std::fs::read(path).map_err(|e| FormatError::Io(format!("{}: {e}", path.display())))?;
    load_bytes(&bytes)
}

#[cfg(test)]
mod version_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_doc::NodeKind;

    fn png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn roundtrip_with_images() {
        let mut d = Document::new("My file");
        let p = d.first_page().unwrap();
        d.create(p, NodeKind::Rectangle, "R", 1.0, 2.0, 3.0, 4.0).unwrap();
        let (hash, w, h) = add_image(&mut d, png()).unwrap();
        assert_eq!((w, h), (3, 2));
        let bytes = save_bytes(&d, None).unwrap();
        let (back, issues) = load_bytes(&bytes).unwrap();
        assert!(issues.is_empty());
        assert_eq!(back.name, "My file");
        assert_eq!(back.node_count(), d.node_count());
        assert!(back.images.contains_key(&hash));
        // Bare JSON opens too.
        let json = serde_json::to_vec(&to_json(&d).unwrap()).unwrap();
        assert!(load_bytes(&json).is_ok());
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(load_bytes(b"not a zip").is_err());
        assert!(load_bytes(b"{\"nope\":1}").is_err());
        assert!(matches!(load_bytes(b"{\"version\":999}"), Err(FormatError::Newer(999))));
        assert!(add_image(&mut Document::new("x"), b"zzz".to_vec()).is_err());
    }
}
