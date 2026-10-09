//! Paint through the session: the file's colour profile (convert or assign, undo, tagged exports).

use serde_json::json;

use crate::Session;
use crate::params::id_of;
use figcraft_color::{Color, ColorProfile};

fn red_rect(s: &mut Session) -> figcraft_doc::NodeId {
    let r = id_of(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 20, "height": 10})).unwrap()["id"]).unwrap();
    s.execute("fill.set", &json!({"ids": [r], "color": "#FF0000"})).unwrap();
    r
}

fn fill(s: &Session, id: figcraft_doc::NodeId) -> Color {
    s.doc().unwrap().node(id).unwrap().fills[0].solid_color().unwrap()
}

/// Whether `bytes` holds a PNG chunk of `kind` (with `data`, when given).
fn has_chunk(bytes: &[u8], kind: &[u8; 4], data: Option<&[u8]>) -> bool {
    bytes.windows(4).enumerate().any(|(i, w)| w == kind && data.is_none_or(|d| bytes.get(i + 4..i + 4 + d.len()) == Some(d)))
}

#[test]
fn color_profile_converts_or_assigns_and_undoes() {
    let mut s = Session::new();
    let r = red_rect(&mut s);
    let v = s.execute("file.colorProfile", &json!({"profile": "displayP3"})).unwrap();
    assert_eq!(v, json!({"profile": "displayP3", "changed": true}));
    assert_eq!(s.doc().unwrap().color_profile, ColorProfile::DisplayP3);
    let p3 = fill(&s, r);
    assert_eq!(p3, ColorProfile::Srgb.conversion_to(ColorProfile::DisplayP3).apply(Color::new(1.0, 0.0, 0.0, 1.0)), "kept the appearance");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().color_profile, ColorProfile::Srgb);
    assert_eq!(fill(&s, r).to_hex(), "FF0000");
    // Keep the values: only the profile changes.
    s.execute("file.colorProfile", &json!({"profile": "Display P3", "convert": false})).unwrap();
    assert_eq!(fill(&s, r).to_hex(), "FF0000");
    assert_eq!(s.doc().unwrap().color_profile, ColorProfile::DisplayP3);
    // The same profile again changes nothing (and adds no undo step).
    let undo = s.ds().unwrap().history.undo.len();
    assert_eq!(s.execute("file.colorProfile", &json!({"profile": "p3"})).unwrap()["changed"], false);
    assert_eq!(s.ds().unwrap().history.undo.len(), undo);
    assert!(s.execute("file.colorProfile", &json!({"profile": "cmyk"})).is_err());
}

#[test]
fn color_profile_without_a_profile_asks_the_ui() {
    let mut s = Session::new();
    let v = s.execute("file.colorProfile", &json!({})).unwrap();
    assert_eq!(v["profile"], "srgb");
    assert!(s.ui_requests.iter().any(|u| u.kind == "dialog.colorProfile"));
    assert!(s.ds().unwrap().history.undo.is_empty());
}

#[test]
fn exports_are_tagged_with_the_profile() {
    let mut s = Session::new();
    let r = red_rect(&mut s);
    let png = |s: &mut Session, format: &str| {
        let v = s.execute("file.export", &json!({"ids": [r], "format": format})).unwrap();
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, v["data"].as_str().unwrap()).unwrap()
    };
    let srgb = png(&mut s, "png");
    assert!(has_chunk(&srgb, b"sRGB", None) && !has_chunk(&srgb, b"iCCP", None) && !has_chunk(&srgb, b"cICP", None));
    s.execute("file.colorProfile", &json!({"profile": "displayP3"})).unwrap();
    let p3 = png(&mut s, "png");
    assert!(has_chunk(&p3, b"iCCP", None) && has_chunk(&p3, b"cICP", Some(&[12, 13, 0, 1])) && !has_chunk(&p3, b"sRGB", None));
    for f in ["jpg", "webp"] {
        assert!(png(&mut s, f).windows(4).any(|w| w == b"mntr"), "{f} embeds the ICC profile");
    }
    // SVG colours are sRGB: the converted red goes back to #FF0000.
    let svg = String::from_utf8(png(&mut s, "svg")).unwrap().to_ascii_uppercase();
    assert!(svg.contains("#FF0000"), "{svg}");
    let pdf = png(&mut s, "pdf");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn color_profile_survives_save_and_load() {
    let mut s = Session::new();
    s.execute("file.colorProfile", &json!({"profile": "displayP3", "convert": false})).unwrap();
    let bytes = crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap();
    let (doc, _) = figcraft_format::load_bytes(&bytes).unwrap();
    assert_eq!(doc.color_profile, ColorProfile::DisplayP3);
}
