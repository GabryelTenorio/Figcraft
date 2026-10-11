//! Design panel in vector edit mode: the selected points' position and dimensions (their
//! transform box), corner radius and handle mirroring, or the selected width point with the
//! Variable width tool (in place of the layer's position and dimensions).

use egui::{Id, Ui};
use figcraft_doc::{Document, HandleMirroring};
use figcraft_engine::vectoredit::{self, VectorEdit};
use serde_json::{Value, json};

use crate::FigApp;
use crate::icons::Icon;
use crate::right::{menu_item, menu_popup, row};
use crate::widgets::{self, FieldEdit, Lead, PAD};

/// Run a point command from a field: typed values are one step, scrubs one step when released.
fn point_field(app: &mut FigApp, cmd: &str, edit: Option<FieldEdit>, params: impl Fn(f64) -> Value) {
    match edit {
        Some(FieldEdit::Set(v)) => {
            app.run(cmd, params(v));
        }
        Some(FieldEdit::Scrub(v, done)) => {
            let _ = app.session.begin_interaction("Edit points");
            let _ = app.session.execute(cmd, &params(v));
            if done {
                let _ = app.session.commit_interaction();
            }
        }
        None => {}
    }
}

/// The Variable width tool's selected width point: where it is along the path and its width.
fn width_point(app: &mut FigApp, ui: &mut Ui, doc: &Document, ve: &VectorEdit) {
    widgets::section_header(ui, "Width point", &[], false);
    let stroke = doc.node(ve.id).map(|n| &n.stroke);
    let sel = ve.width_point.and_then(|i| Some((i, *stroke?.profile()?.points.get(i)?)));
    let Some((i, p)) = sel else {
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            widgets::label(ui, "Drag the stroke to add a width point", t.text_secondary);
        });
        ui.add_space(8.0);
        return;
    };
    let weight = stroke.map_or(1.0, |s| s.weight);
    let col = widgets::col_width(ui.available_width());
    let round = |v: f64| (v * 100.0).round() / 100.0;
    widgets::sublabels(ui, "Position", Some("Width"));
    row(ui, 32.0, |ui| {
        let e = widgets::number_field(ui, Id::new("wpt_t"), Lead::None, Some(round(p.t * 100.0)), "%", col, 1.0);
        point_field(app, "vector.edit.widthPoint", e, |v| json!({"index": i, "t": v / 100.0}));
        let e = widgets::number_field(ui, Id::new("wpt_w"), Lead::Icon(Icon::StrokeIcon, "Width"), Some(round(p.scale * weight)), "", col, 1.0);
        point_field(app, "vector.edit.widthPoint", e, |v| json!({"index": i, "width": v.max(0.0)}));
    });
    // Each side's reach from the centre line (Alt-drag a handle on the canvas for one side).
    let (l, r) = p.sides();
    widgets::sublabels(ui, "Left side", Some("Right side"));
    row(ui, 32.0, |ui| {
        for (side, scale, name) in [("left", l, "Left side width"), ("right", r, "Right side width")] {
            let e = widgets::number_field(
                ui,
                Id::new(("wpt_side", side)),
                Lead::Icon(Icon::StrokeIcon, name),
                Some(round(scale * weight / 2.0)),
                "",
                col,
                1.0,
            );
            point_field(app, "vector.edit.widthPoint", e, |v| json!({"index": i, "side": side, "width": v.max(0.0)}));
        }
    });
    ui.add_space(8.0);
}

pub fn points(app: &mut FigApp, ui: &mut Ui, doc: &Document, ve: &VectorEdit) {
    if ve.tool == vectoredit::VTool::Width {
        width_point(app, ui, doc, ve);
        return;
    }
    let Some(net) = vectoredit::network(doc, ve.id) else { return };
    let pts = vectoredit::selected_points(net, ve);
    widgets::section_header(ui, if pts.len() > 1 { "Points" } else { "Point" }, &[], false);
    if pts.is_empty() {
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            widgets::label(ui, "Select points to edit their position, radius and mirroring", t.text_secondary);
        });
        ui.add_space(8.0);
        return;
    }
    let col = widgets::col_width(ui.available_width());
    let pos = vectoredit::selection_position(doc, ve);
    let round = |v: f64| (v * 100.0).round() / 100.0;
    widgets::sublabels(ui, "Position", None);
    row(ui, 32.0, |ui| {
        let e = widgets::number_field(ui, Id::new("vpt_x"), Lead::Text("X"), pos.map(|p| round(p.x)), "", col, 1.0);
        point_field(app, "vector.edit.position", e, |v| json!({"x": v}));
        let e = widgets::number_field(ui, Id::new("vpt_y"), Lead::Text("Y"), pos.map(|p| round(p.y)), "", col, 1.0);
        point_field(app, "vector.edit.position", e, |v| json!({"y": v}));
    });
    if let Some(b) = vectoredit::transform::point_box(doc, ve).filter(|b| b.points.len() > 1) {
        widgets::sublabels(ui, "Dimensions", None);
        row(ui, 32.0, |ui| {
            let e = widgets::number_field(ui, Id::new("vpt_w"), Lead::Text("W"), Some(round(b.width)), "", col, 1.0);
            point_field(app, "vector.edit.resize", e, |v| json!({"width": v.max(0.0)}));
            let e = widgets::number_field(ui, Id::new("vpt_h"), Lead::Text("H"), Some(round(b.height)), "", col, 1.0);
            point_field(app, "vector.edit.resize", e, |v| json!({"height": v.max(0.0)}));
        });
    }
    let common = |f: &dyn Fn(usize) -> Option<f64>| -> Option<f64> {
        let mut it = pts.iter().map(|v| f(*v));
        let first = it.next().flatten()?;
        it.all(|x| x == Some(first)).then_some(first)
    };
    let radius = common(&|v| net.vertices.get(v).map(|x| x.radius));
    let mirror = {
        let mut it = pts.iter().map(|v| net.mirroring(*v));
        let first = it.next();
        first.filter(|m| it.all(|x| x == *m))
    };
    widgets::sublabels(ui, "Corner radius", None);
    row(ui, 32.0, |ui| {
        let e = widgets::number_field(ui, Id::new("vpt_radius"), Lead::Icon(Icon::CornerRadius, "Corner radius"), radius.map(round), "", col, 1.0);
        point_field(app, "vector.edit.cornerRadius", e, |v| json!({"radius": v.max(0.0)}));
    });
    widgets::sublabels(ui, "Mirroring", None);
    row(ui, 32.0, |ui| {
        let label = |m: HandleMirroring| match m {
            HandleMirroring::None => "No mirroring",
            HandleMirroring::Angle => "Mirror angle",
            HandleMirroring::AngleAndLength => "Mirror angle and length",
        };
        let resp = widgets::dropdown(ui, mirror.map(label).unwrap_or("Mixed"), col * 2.0 + 8.0, None);
        menu_popup(&resp).show(|ui| {
            for m in [HandleMirroring::None, HandleMirroring::Angle, HandleMirroring::AngleAndLength] {
                if menu_item(ui, label(m), "") {
                    app.run("vector.edit.mirroring", json!({"mode": m.id()}));
                    ui.close();
                }
            }
        });
    });
    ui.add_space(8.0);
}
