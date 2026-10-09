//! Binding variables to properties from the Design panel: number fields show a variable button on
//! hover and the bound variable's name once bound; colour fills bind through the style picker. The
//! picker popover lists the document's variables of the matching type, by collection.

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use figcraft_doc::{Document, NodeId, VariableType};
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, BindEvent, FieldEdit, Lead};

/// The open variable picker: which layers and field it binds, the variable type, where it opens.
#[derive(Clone)]
struct Picker {
    ids: Vec<NodeId>,
    field: String,
    kind: VariableType,
    at: Pos2,
}

fn picker_id() -> Id {
    Id::new("variable_picker")
}

/// The variable `field` is bound to on every one of `ids` (its name), if they agree.
pub fn bound_name(doc: &Document, ids: &[NodeId], field: &str) -> Option<String> {
    let mut it = ids.iter().map(|i| doc.node(*i).and_then(|n| n.bound.get(field)).cloned());
    let first = it.next().flatten()?;
    if !it.all(|v| v.as_ref() == Some(&first)) {
        return None;
    }
    doc.variables.get(&first).map(|v| v.name.clone())
}

/// Open the picker for `field` of `ids` at `at`.
pub fn open(ui: &Ui, ids: &[NodeId], field: &str, kind: VariableType, at: Pos2) {
    ui.data_mut(|d| d.insert_temp(picker_id(), Picker { ids: ids.to_vec(), field: field.to_string(), kind, at }));
}

/// A number field whose value can be bound to a number variable.
pub fn number_field(
    app: &mut FigApp,
    ui: &mut Ui,
    ids: &[NodeId],
    field: &str,
    id: Id,
    lead: Lead,
    value: Option<f64>,
    width: f32,
) -> Option<FieldEdit> {
    let name = app.session.doc().ok().and_then(|d| bound_name(d, ids, field));
    let (edit, ev) = widgets::number_field_bindable(ui, id, lead, value, "", width, 1.0, Some(name.as_deref()));
    match ev {
        Some(BindEvent::Open(at)) => open(ui, ids, field, VariableType::Number, at),
        Some(BindEvent::Detach) => {
            app.run("variable.bind", json!({"ids": ids, "field": field, "variable": null}));
        }
        None => {}
    }
    edit
}

/// The picker popover (call once per frame).
pub fn picker(app: &mut FigApp, ctx: &egui::Context) {
    let Some(p) = ctx.data(|d| d.get_temp::<Picker>(picker_id())) else { return };
    let Ok(doc) = app.session.doc().cloned() else { return };
    let t = Tokens::get(ctx);
    let query_id = Id::new("variable_picker_query");
    let mut query: String = ctx.data(|d| d.get_temp(query_id)).unwrap_or_default();
    let q = query.trim().to_lowercase();
    let mut chosen: Option<String> = None;
    let mut close = false;
    let area = egui::Area::new(Id::new("variable_picker_area")).order(egui::Order::Foreground).fixed_pos(p.at + vec2(0.0, 4.0)).show(ctx, |ui| {
        egui::Frame::NONE.fill(t.menu).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_width(240.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Variables").font(semibold(11.0)).color(t.menu_text));
            });
            ui.add_space(4.0);
            let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search").font(regular(11.0)).desired_width(224.0));
            if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
                r.request_focus();
            }
            ui.add_space(4.0);
            let mut any = false;
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                for c in &doc.variables.collections {
                    let vars: Vec<_> = doc
                        .variables
                        .variables
                        .iter()
                        .filter(|v| v.collection == c.id && v.kind == p.kind && (q.is_empty() || v.name.to_lowercase().contains(&q)))
                        .collect();
                    if vars.is_empty() {
                        continue;
                    }
                    any = true;
                    ui.label(egui::RichText::new(&c.name).font(regular(10.0)).color(t.menu_dim));
                    for v in vars {
                        let (r, resp) = ui.allocate_exact_size(vec2(224.0, 26.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(5), t.menu_hover);
                        }
                        let modes = std::collections::BTreeMap::new();
                        let value = doc.variables.resolve(&v.id, &modes);
                        match &value {
                            Some(figcraft_doc::VariableValue::Color(c)) => {
                                let sw = Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(14.0, 14.0));
                                widgets::paint_swatch(ui.painter(), sw, &figcraft_color::Paint::solid(*c));
                            }
                            _ => icons::paint(
                                ui.painter(),
                                Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(12.0, 12.0)),
                                Icon::Variable,
                                t.menu_text,
                            ),
                        }
                        ui.painter().text(pos2(r.min.x + 26.0, r.center().y), Align2::LEFT_CENTER, &v.name, regular(11.0), t.menu_text);
                        if let Some(val) = value.filter(|v| !matches!(v, figcraft_doc::VariableValue::Color(_))) {
                            ui.painter().text(pos2(r.max.x - 6.0, r.center().y), Align2::RIGHT_CENTER, val.display(), regular(11.0), t.menu_dim);
                        }
                        if resp.clicked() {
                            chosen = Some(v.id.clone());
                        }
                    }
                }
            });
            if !any {
                let kind = match p.kind {
                    VariableType::Color => "colour",
                    VariableType::Number => "number",
                    VariableType::String => "string",
                    VariableType::Boolean => "boolean",
                };
                ui.label(
                    egui::RichText::new(format!("No {kind} variables yet. Create them in Local variables.")).font(regular(11.0)).color(t.menu_dim),
                );
            }
        });
    });
    let rect = area.response.rect;
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("variable_picker_border"))).rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, Color32::from_black_alpha(40)),
        egui::StrokeKind::Inside,
    );
    let clicked_outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|pos| !rect.contains(pos)));
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || clicked_outside {
        close = true;
    }
    if let Some(var) = chosen {
        app.run("variable.bind", json!({"ids": p.ids, "field": p.field, "variable": var}));
        close = true;
    }
    if close {
        ctx.data_mut(|d| {
            d.remove::<Picker>(picker_id());
            d.remove::<String>(query_id);
        });
    } else {
        ctx.data_mut(|d| d.insert_temp(query_id, query));
    }
}
