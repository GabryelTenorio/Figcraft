//! Binding variables to properties from the Design panel: number fields show a variable button on
//! hover and the bound variable's name once bound; colour fills bind through the style picker. The
//! picker popover lists the document's variables of the matching type (and scope), by collection;
//! the Variables editor uses it to pick alias targets too.

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use figcraft_doc::{Document, ModeId, NodeId, VariableId, VariableType};
use serde_json::json;

use crate::FigApp;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, BindEvent, FieldEdit, Lead};

/// What picking a variable does.
#[derive(Clone)]
enum Target {
    /// Bind it to `field` of `ids`.
    Bind { ids: Vec<NodeId>, field: String },
    /// Make `variable`'s value in `mode` an alias of it.
    Alias { variable: VariableId, mode: ModeId },
}

/// The open variable picker: what it does, the variable type, where it opens.
#[derive(Clone)]
struct Picker {
    target: Target,
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
    let target = Target::Bind { ids: ids.to_vec(), field: field.to_string() };
    ui.data_mut(|d| d.insert_temp(picker_id(), Picker { target, kind, at }));
}

/// Open the picker at `at` to alias `variable`'s value in `mode` to another variable.
pub fn open_alias(ui: &Ui, variable: &str, mode: &str, kind: VariableType, at: Pos2) {
    let target = Target::Alias { variable: variable.to_string(), mode: mode.to_string() };
    ui.data_mut(|d| d.insert_temp(picker_id(), Picker { target, kind, at }));
}

/// Open the picker for `field` of `ids` under the pointer (from a section header button).
pub fn open_at_pointer(ui: &Ui, ids: &[NodeId], field: &str, kind: VariableType) {
    let at = ui.input(|i| i.pointer.interact_pos()).unwrap_or_default();
    open(ui, ids, field, kind, at);
}

/// Act on a bound field's event: change the variable, or detach it.
fn handle(app: &mut FigApp, ui: &Ui, ids: &[NodeId], field: &str, kind: VariableType, ev: Option<BindEvent>) {
    match ev {
        Some(BindEvent::Open(at)) => open(ui, ids, field, kind, at),
        Some(BindEvent::Detach) => {
            app.run("variable.bind", json!({"ids": ids, "field": field, "variable": null}));
        }
        None => {}
    }
}

/// A number field whose value can be bound to a number variable. `suffix` follows the number
/// ("%" for opacity, whose variables hold percentages).
pub fn number_field(
    app: &mut FigApp,
    ui: &mut Ui,
    ids: &[NodeId],
    field: &str,
    id: Id,
    lead: Lead,
    value: Option<f64>,
    suffix: &str,
    width: f32,
) -> Option<FieldEdit> {
    let name = app.session.doc().ok().and_then(|d| bound_name(d, ids, field));
    let (edit, ev) = widgets::number_field_bindable(ui, id, lead, value, suffix, width, 1.0, Some(name.as_deref()));
    handle(app, ui, ids, field, VariableType::Number, ev);
    edit
}

/// When `field` of `ids` is bound: the variable's chip in `rect` (change or detach it) and true;
/// otherwise nothing and false.
pub fn bound_chip(app: &mut FigApp, ui: &mut Ui, ids: &[NodeId], field: &str, kind: VariableType, rect: Rect) -> bool {
    let Some(name) = app.session.doc().ok().and_then(|d| bound_name(d, ids, field)) else { return false };
    let ev = widgets::bound_field(ui, Id::new(("bound_chip", field)), Lead::None, rect, &name);
    handle(app, ui, ids, field, kind, ev);
    true
}

/// A variable button at the right end of `rect` while the pointer is over it: opens the picker
/// to bind `field` of `ids`.
pub fn bind_button(ui: &mut Ui, rect: Rect, ids: &[NodeId], field: &str, kind: VariableType) {
    if !ui.rect_contains_pointer(rect) {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let b = Rect::from_center_size(pos2(rect.max.x - 11.0, rect.center().y), vec2(18.0, 18.0));
    let resp = ui.interact(b, Id::new(("bind_button", field)), Sense::click()).on_hover_text("Apply variable");
    if resp.hovered() {
        ui.painter().rect_filled(b, CornerRadius::same(4), t.hover);
    }
    icons::paint(ui.painter(), b.shrink(3.0), Icon::Variable, t.icon_secondary);
    if resp.clicked() {
        open(ui, ids, field, kind, rect.left_bottom());
    }
}

/// A field showing the variable `field` of `ids` is bound to, for bindings without a value field
/// of their own (text content, visibility): `width` wide, at most the row. Nothing when unbound.
pub fn bound_row(app: &mut FigApp, ui: &mut Ui, ids: &[NodeId], field: &str, kind: VariableType, lead: Lead, width: f32) {
    let Some(name) = app.session.doc().ok().and_then(|d| bound_name(d, ids, field)) else { return };
    crate::right::row(ui, 32.0, |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), widgets::FIELD_H), Sense::hover());
        let ev = widgets::bound_field(ui, Id::new(("bound_row", field)), lead, rect, &name);
        handle(app, ui, ids, field, kind, ev);
    });
}

/// The picker popover (call once per frame).
pub fn picker(app: &mut FigApp, ctx: &egui::Context) {
    let Some(p) = ctx.data(|d| d.get_temp::<Picker>(picker_id())) else { return };
    let Ok(doc) = app.session.ds().map(|d| d.doc.clone()) else { return };
    // Bindings offer variables scoped to the property; aliases, any that wouldn't loop.
    let offered = |v: &figcraft_doc::Variable| match &p.target {
        Target::Bind { ids, field } => {
            ids.first().and_then(|id| figcraft_engine::cmd::variables::field_scope(&doc, *id, field)).is_none_or(|scope| v.in_scope(scope))
        }
        Target::Alias { variable, .. } => {
            doc.variables.get(variable).is_some_and(|own| figcraft_engine::cmd::variables::alias_target(&doc.variables, own, &v.id).is_ok())
        }
    };
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
                        .filter(|v| v.collection == c.id && v.kind == p.kind && (q.is_empty() || v.name.to_lowercase().contains(&q)) && offered(v))
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
                let msg = if doc.variables.variables.iter().any(|v| v.kind == p.kind) {
                    format!("No {kind} variables fit here.")
                } else {
                    format!("No {kind} variables yet. Create them in Local variables.")
                };
                ui.label(egui::RichText::new(msg).font(regular(11.0)).color(t.menu_dim));
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
        match &p.target {
            Target::Bind { ids, field } => app.run("variable.bind", json!({"ids": ids, "field": field, "variable": var})),
            Target::Alias { variable, mode } => app.run("variable.set", json!({"variable": variable, "mode": mode, "value": {"alias": var}})),
        };
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
