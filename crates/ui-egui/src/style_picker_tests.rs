//! The style picker: name folders, new style names, and the popover drawn headless (list, grid,
//! editor), committing a typed name when it closes.

use egui::{Pos2, RawInput, Rect, vec2};
use figcraft_engine::Session;
use serde_json::json;

use super::*;
use crate::{FigApp, NoServices};

#[test]
fn names_split_into_folders() {
    assert_eq!((folder("Brand/Blue/500"), leaf("Brand/Blue/500")), ("Brand/Blue", "500"));
    assert_eq!((folder("Danger"), leaf("Danger")), ("", "Danger"));
    assert_eq!((folder(" Brand / Primary "), leaf(" Brand / Primary ")), ("Brand", "Primary"));
    let mut d = Document::new("d");
    assert_eq!(next_name(&d, "Color"), "Color 1");
    for n in ["Color 1", "Color 2"] {
        d.styles.push(Style { id: n.into(), name: n.into(), description: String::new(), kind: StyleKind::Paint { paints: vec![] } });
    }
    assert_eq!(next_name(&d, "Color"), "Color 3");
    assert!(StyleSlot::Fill.holds(&StyleKind::Paint { paints: vec![] }) && !StyleSlot::Effect.holds(&StyleKind::Paint { paints: vec![] }));
    for s in [StyleSlot::Fill, StyleSlot::Stroke, StyleSlot::Effect, StyleSlot::Grid] {
        assert_eq!(StyleSlot::parse(s.key()), Some(s));
    }
}

fn frame(ctx: &egui::Context, app: &mut FigApp, t: &mut f64) {
    *t += 1.0 / 60.0;
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), time: Some(*t), ..Default::default() };
    let mut out = ctx.run_ui(input, |ui| app.ui(ui));
    out.textures_delta.clear();
}

#[test]
fn picker_draws_every_view_and_commits_a_typed_name_on_close() {
    let mut s = Session::new();
    let r = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap()["id"].clone();
    for (name, c) in [("Brand/Primary", "#0D99FF"), ("Brand/Accent", "#FFC700"), ("Danger", "#F24822")] {
        s.execute("style.create", &json!({"type": "paint", "name": name, "value": [{"type": "solid", "color": c}]})).unwrap();
    }
    s.execute("select.set", &json!({"ids": [r]})).unwrap();
    s.execute("style.apply", &json!({"ids": [r], "style": "Brand/Primary", "target": "fill"})).unwrap();
    let mut app = FigApp::new(s, Box::new(NoServices));
    let ctx = egui::Context::default();
    let mut t = 0.0;
    for slot in [StyleSlot::Fill, StyleSlot::Stroke, StyleSlot::Effect, StyleSlot::Grid] {
        open_for_selection(&mut app, slot, None);
        for grid in [false, true] {
            app.ui.style_grid = grid;
            for _ in 0..3 {
                frame(&ctx, &mut app, &mut t);
            }
            assert_eq!(app.ui.style_picker.as_ref().map(Picker::slot), Some(slot), "{slot:?} stays open");
        }
    }
    // Folders fold.
    app.ui.style_folds.insert("Brand".into());
    frame(&ctx, &mut app, &mut t);
    // The editor; a name typed there is written when the picker closes (here: a new selection).
    open_for_selection(&mut app, StyleSlot::Fill, Some("Brand/Accent"));
    frame(&ctx, &mut app, &mut t);
    let edit = app.ui.style_picker.as_mut().and_then(|p| p.edit.as_mut()).expect("editing");
    edit.name = "Brand/Gold".into();
    app.run("select.none", json!({}));
    frame(&ctx, &mut app, &mut t);
    assert!(app.ui.style_picker.is_none(), "a new selection closes the picker");
    let doc = app.session.doc().unwrap();
    assert!(doc.style("Brand/Gold").is_some(), "{:?}", doc.styles.iter().map(|s| &s.name).collect::<Vec<_>>());
}
