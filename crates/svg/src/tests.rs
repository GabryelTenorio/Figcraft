use super::*;
use figcraft_color::{Color, Paint};
use figcraft_doc::{Document, Effect, NodeKind};

#[test]
fn export_basic_shapes() {
    let mut d = Document::new("t");
    let p = d.first_page().unwrap();
    let f = d.create(p, NodeKind::Frame, "Card", 0.0, 0.0, 200.0, 100.0).unwrap();
    let r = d.create(f, NodeKind::Rectangle, "R", 10.0, 10.0, 50.0, 50.0).unwrap();
    d.get_mut(r).unwrap().fills = vec![Paint::solid(Color::from_hex("FF0000").unwrap())];
    d.get_mut(r).unwrap().effects.push(Effect::drop_shadow());
    let svg = export_svg(&d, &[f], &SvgOptions::default()).unwrap();
    assert!(svg.starts_with("<svg width=\"200\" height=\"100\""));
    assert!(svg.contains("#FF0000"));
    assert!(svg.contains("<filter"));
    assert!(svg.contains("clip-path"));
    // Round-trip through import.
    let mut d2 = Document::new("t2");
    let p2 = d2.first_page().unwrap();
    let frame = import_svg(&mut d2, svg.as_bytes(), p2, 0.0, 0.0, "Imported").unwrap();
    assert!(!d2.subtree(frame).is_empty());
    assert!(d2.validate().is_empty());
}

#[test]
fn import_rejects_garbage() {
    let mut d = Document::new("t");
    let p = d.first_page().unwrap();
    assert!(import_svg(&mut d, b"not svg", p, 0.0, 0.0, "x").is_err());
}

#[test]
fn import_gradient_and_groups() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
      <defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient></defs>
      <g id="grp"><rect x="10" y="10" width="30" height="30" fill="url(#g)" stroke="#000" stroke-width="2"/></g>
      <circle cx="70" cy="25" r="20" fill="#0f0" fill-opacity="0.5"/></svg>"##;
    let mut d = Document::new("t");
    let p = d.first_page().unwrap();
    let f = import_svg(&mut d, svg, p, 0.0, 0.0, "x").unwrap();
    let kinds: Vec<&str> = d.subtree(f).iter().map(|i| d.node(*i).unwrap().kind.type_name()).collect();
    assert!(kinds.contains(&"GROUP"));
    assert!(kinds.contains(&"RECTANGLE") && kinds.contains(&"ELLIPSE"), "{kinds:?}");
}
