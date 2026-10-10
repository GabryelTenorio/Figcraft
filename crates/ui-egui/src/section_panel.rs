//! Design panel extras for frames and sections: section colours, handoff status and resize to
//! fit.

use egui::{CornerRadius, Sense, Stroke, Ui, vec2};
use figcraft_doc::{DevStatus, Document, NodeId, NodeKind};
use serde_json::json;

use crate::FigApp;
use crate::icons::Icon;
use crate::right::{common, menu_item, row};
use crate::widgets;

/// Section fill presets: light tints that keep dark text readable.
const SECTION_COLORS: [(&str, &str); 8] = [
    ("White", "#FFFFFF"),
    ("Gray", "#EDEDED"),
    ("Red", "#FFE0DE"),
    ("Orange", "#FFE8D1"),
    ("Yellow", "#FFF4C2"),
    ("Green", "#DDF5E3"),
    ("Blue", "#DCEBFF"),
    ("Violet", "#EBE3FF"),
];

fn is_section(doc: &Document, id: NodeId) -> bool {
    doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Section { .. }))
}

/// Whether `id` can carry a handoff status: a section, or a frame at the top of a page or section.
pub fn has_status(doc: &Document, id: NodeId) -> bool {
    is_section(doc, id)
        || doc.node(id).is_some_and(|n| {
            n.kind.is_frame_like()
                && !matches!(n.kind, NodeKind::Instance { .. })
                && n.parent.and_then(|p| doc.node(p)).is_some_and(|p| p.is_page() || matches!(p.kind, NodeKind::Section { .. }))
        })
}

/// The handoff status of `ids` as a dropdown `width` wide ("Mixed" when they differ).
pub fn status_dropdown(app: &mut FigApp, ui: &mut Ui, doc: &Document, ids: &[NodeId], width: f32) {
    let st = common(doc, ids, |n| n.dev_status);
    let resp = widgets::dropdown(ui, st.map(DevStatus::label).unwrap_or("Mixed"), width, Some(Icon::Code));
    egui::Popup::menu(&resp).show(|ui| {
        for d in DevStatus::ALL {
            if menu_item(ui, d.label(), "") {
                app.set_props(ids, json!({"devStatus": d.id()}));
                ui.close();
            }
        }
    });
}

pub fn section(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId]) {
    let sections = !sel.is_empty() && sel.iter().all(|i| is_section(doc, *i));
    if sel.is_empty() || !sel.iter().all(|i| has_status(doc, *i)) {
        return;
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    let ids = sel.to_vec();
    let col = widgets::col_width(ui.available_width());
    if sections {
        widgets::sublabels(ui, "Section color", None);
        row(ui, 32.0, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let current = common(doc, sel, |n| n.fills.first().and_then(|p| p.solid_color()).map(|c| c.to_hex()));
            for (name, hex) in SECTION_COLORS {
                let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                let c = figcraft_color::Color::from_hex(hex).unwrap_or(figcraft_color::Color::WHITE);
                ui.painter().rect_filled(r, CornerRadius::same(9), crate::theme::doc_c32(ui.ctx(), c));
                let on = current.as_ref().and_then(|c| c.as_ref()).is_some_and(|h| h.eq_ignore_ascii_case(hex.trim_start_matches('#')));
                let stroke = if on { Stroke::new(2.0, crate::theme::ACCENT) } else { Stroke::new(1.0, t.border) };
                ui.painter().rect_stroke(r, CornerRadius::same(9), stroke, egui::StrokeKind::Inside);
                if resp.on_hover_text(name).clicked() {
                    app.set_props(&ids, json!({"fills": [hex]}));
                }
            }
        });
    }
    widgets::sublabels(ui, "Status", None);
    row(ui, 32.0, |ui| status_dropdown(app, ui, doc, &ids, col * 2.0 + 8.0));
    row(ui, 32.0, |ui| {
        let fit = ui.add_sized(vec2(col * 2.0 + 8.0, widgets::FIELD_H), egui::Button::new("Resize to fit")).on_hover_text("Fit to contents  ⌥⇧⌘R");
        if fit.clicked() {
            app.run("object.resizeToFit", json!({"ids": ids}));
        }
    });
    ui.add_space(4.0);
    widgets::separator(ui);
}
