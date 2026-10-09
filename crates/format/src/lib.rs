//! The native `.figcraft` format.
//!
//! A zip archive holding `document.json` (the [`Document`] as JSON, with a `format`/`version`
//! header), `images/<sha256>` for every image the document uses, and an optional
//! `thumbnail.png`. Plain JSON (the `document.json` alone) also opens. Reading is lenient:
//! structural problems are repaired (see [`Document::repair`]) and reported.
//!
//! [`rest`] imports the public REST API file JSON.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod rest;

use std::io::{Read, Write};
use std::sync::Arc;

use figcraft_doc::{Document, ImageData};
use serde_json::{Value, json};
use sha2::Digest;

pub const EXTENSION: &str = "figcraft";
pub const FORMAT: &str = "figcraft";
pub const VERSION: u32 = 1;
/// Largest archive member we read (guards against zip bombs).
const MAX_MEMBER: u64 = 512 * 1024 * 1024;

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
    let v: Value = serde_json::from_str(text).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    if let Some(ver) = v.get("version").and_then(Value::as_u64)
        && ver > u64::from(VERSION)
    {
        return Err(FormatError::Newer(ver as u32));
    }
    let mut doc: Document = serde_json::from_value(v).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    let issues = doc.validate();
    if !issues.is_empty() {
        doc.repair();
    }
    Ok((doc, issues))
}

/// Write a `.figcraft` archive to bytes.
pub fn save_bytes(doc: &Document, thumbnail_png: Option<&[u8]>) -> Result<Vec<u8>, FormatError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file("document.json", opts).map_err(|e| FormatError::Io(e.to_string()))?;
        let json = serde_json::to_vec(&to_json(doc)?).map_err(|e| FormatError::Io(e.to_string()))?;
        z.write_all(&json).map_err(|e| FormatError::Io(e.to_string()))?;
        for (hash, img) in &doc.images {
            z.start_file(format!("images/{hash}"), stored).map_err(|e| FormatError::Io(e.to_string()))?;
            z.write_all(&img.bytes).map_err(|e| FormatError::Io(e.to_string()))?;
        }
        if let Some(t) = thumbnail_png {
            z.start_file("thumbnail.png", stored).map_err(|e| FormatError::Io(e.to_string()))?;
            z.write_all(t).map_err(|e| FormatError::Io(e.to_string()))?;
        }
        z.finish().map_err(|e| FormatError::Io(e.to_string()))?;
    }
    Ok(buf.into_inner())
}

/// Read a `.figcraft` archive (or bare JSON) from bytes. Returns the document and any repairs made.
pub fn load_bytes(bytes: &[u8]) -> Result<(Document, Vec<String>), FormatError> {
    let trimmed = bytes.iter().position(|b| !b.is_ascii_whitespace()).and_then(|i| bytes.get(i));
    if trimmed == Some(&b'{') {
        let text = std::str::from_utf8(bytes).map_err(|e| FormatError::Corrupt(e.to_string()))?;
        return from_json(text);
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| FormatError::Corrupt(e.to_string()))?;
    let text = {
        let f = z.by_name("document.json").map_err(|e| FormatError::Corrupt(e.to_string()))?;
        let mut s = String::new();
        f.take(MAX_MEMBER).read_to_string(&mut s).map_err(|e| FormatError::Corrupt(e.to_string()))?;
        s
    };
    let (mut doc, issues) = from_json(&text)?;
    let names: Vec<String> = z.file_names().filter(|n| n.starts_with("images/")).map(str::to_string).collect();
    for name in names {
        let Ok(f) = z.by_name(&name) else { continue };
        let mut data = Vec::new();
        if f.take(MAX_MEMBER).read_to_end(&mut data).is_err() {
            continue;
        }
        let hash = name.trim_start_matches("images/").to_string();
        let (w, h) =
            image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok()).unwrap_or((0, 0));
        doc.images.insert(hash, ImageData { bytes: Arc::new(data), width: w, height: h });
    }
    Ok((doc, issues))
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
