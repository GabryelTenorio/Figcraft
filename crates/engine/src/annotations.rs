//! Dev Mode annotations: notes designers attach to layers for developers, each with optional
//! pinned properties (width, fill, font size…) that Dev Mode shows with their current values. They
//! are stored on the layer (`Node::annotations`), saved with the file and undoable.

use figcraft_doc::*;
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::devinspect::{Prop, Section, properties};
use crate::params::*;
use crate::{bad, spec};

/// Most annotations on one layer.
pub const MAX_PER_LAYER: usize = 64;
/// Longest annotation text (chars).
pub const MAX_LABEL: usize = 5_000;
/// Most annotated layers listed or drawn for one page.
pub const MAX_LISTED: usize = 2_000;

/// A property an annotation can pin: its id, its label and where Dev Mode's property list shows it
/// (a section and a row; `None` = the section's rows other pinnable properties don't claim).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pinnable {
    pub id: &'static str,
    pub label: &'static str,
    section: &'static str,
    row: Option<&'static str>,
}

const fn pin(id: &'static str, label: &'static str, section: &'static str, row: Option<&'static str>) -> Pinnable {
    Pinnable { id, label, section, row }
}

/// Every pinnable property, in the order Dev Mode lists them.
pub const PINNABLE: &[Pinnable] = &[
    pin("width", "Width", "Layout", Some("Width")),
    pin("height", "Height", "Layout", Some("Height")),
    pin("top", "Top", "Layout", Some("Top")),
    pin("left", "Left", "Layout", Some("Left")),
    pin("rotation", "Rotation", "Layout", Some("Rotation")),
    pin("opacity", "Opacity", "Layout", Some("Opacity")),
    pin("cornerRadius", "Corner radius", "Layout", Some("Radius")),
    pin("direction", "Direction", "Auto layout", Some("Direction")),
    pin("gap", "Gap", "Auto layout", Some("Gap")),
    pin("padding", "Padding", "Auto layout", Some("Padding")),
    pin("alignment", "Alignment", "Auto layout", Some("Align")),
    pin("fontFamily", "Font", "Typography", Some("Font")),
    pin("fontStyle", "Font style", "Typography", Some("Style")),
    pin("fontSize", "Font size", "Typography", Some("Size")),
    pin("lineHeight", "Line height", "Typography", Some("Line height")),
    pin("letterSpacing", "Letter spacing", "Typography", Some("Letter spacing")),
    pin("textAlign", "Text align", "Typography", Some("Align")),
    pin("fill", "Fill", "Fills", None),
    pin("stroke", "Stroke", "Strokes", None),
    pin("strokeWeight", "Stroke weight", "Strokes", Some("Weight")),
    pin("effects", "Effects", "Effects", None),
];

/// The pinnable property with id (or label) `key`, ignoring case.
pub fn pinnable(key: &str) -> Option<&'static Pinnable> {
    let key = key.trim();
    PINNABLE.iter().find(|p| p.id.eq_ignore_ascii_case(key) || p.label.eq_ignore_ascii_case(key))
}

/// Whether `n` has anything to show for `p` (a layer without auto layout has no gap…).
pub fn applies(sections: &[Section], p: &Pinnable) -> bool {
    pinned_in(sections, p).is_some()
}

/// `p`'s value in a layer's inspected `sections`, labelled with `p`'s label (`None` when the
/// layer doesn't have it).
pub fn pinned_in(sections: &[Section], p: &Pinnable) -> Option<Prop> {
    let s = sections.iter().find(|s| s.title == p.section)?;
    let rows: Vec<&Prop> = match p.row {
        Some(label) => s.rows.iter().filter(|r| r.label == label).take(1).collect(),
        // The section's own rows: not those another pinnable property shows (strokes' weight).
        None => s.rows.iter().filter(|r| !PINNABLE.iter().any(|q| q.section == p.section && q.row == Some(r.label.as_str()))).collect(),
    };
    let first = rows.first()?;
    let value = match p.row {
        Some(_) => first.value.clone(),
        None => {
            rows.iter().map(|r| if r.label == "Solid" { r.value.clone() } else { format!("{} {}", r.label, r.value) }).collect::<Vec<_>>().join(", ")
        }
    };
    Some(Prop { label: p.label.to_string(), value, color: first.color.clone(), token: first.token.clone() })
}

/// The pinned properties of annotation `a` on layer `id` with their current values (`None` for
/// those the layer doesn't have); unknown ids are skipped.
pub fn pinned(doc: &Document, id: NodeId, a: &Annotation) -> Vec<(&'static Pinnable, Option<Prop>)> {
    if a.properties.is_empty() {
        return Vec::new();
    }
    let sections = properties(doc, id);
    a.properties.iter().filter_map(|k| pinnable(k)).map(|p| (p, pinned_in(&sections, p))).collect()
}

/// The annotated layers on `page`, in layer order (bounded).
pub fn on_page(doc: &Document, page: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![page];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited += 1;
        if out.len() >= MAX_LISTED || visited > 200_000 {
            break;
        }
        let Some(n) = doc.node(id) else { continue };
        if !n.annotations.is_empty() {
            out.push(id);
        }
        stack.extend(n.children.iter().rev().copied());
    }
    out
}

/// An annotation as JSON: its layer, index, text and pinned properties with their values.
pub fn annotation_json(doc: &Document, id: NodeId, index: usize, a: &Annotation) -> Value {
    let name = doc.node(id).map(|n| n.name.clone()).unwrap_or_default();
    let props: Vec<Value> = pinned(doc, id, a)
        .into_iter()
        .map(|(p, v)| {
            let mut o = json!({"property": p.id, "label": p.label, "value": v.as_ref().map(|v| v.value.clone())});
            if let Some(t) = v.as_ref().and_then(|v| v.token.clone()) {
                o["token"] = json!(t);
            }
            if let Some(c) = v.and_then(|v| v.color) {
                o["color"] = json!(c);
            }
            o
        })
        .collect();
    json!({"id": id, "name": name, "index": index, "label": a.label, "properties": props})
}

/// `properties` from params: known ids, each once, in the given order.
fn parse_properties(cmd: &str, p: &Value) -> crate::Result<Option<Vec<String>>> {
    let Some(v) = get(p, "properties") else { return Ok(None) };
    let Value::Array(list) = v else { return Err(bad(cmd, "`properties` must be a list of property ids")) };
    let mut out: Vec<String> = Vec::new();
    for item in list.iter().take(PINNABLE.len() * 4) {
        let k = item.as_str().unwrap_or_default();
        let found = pinnable(k).ok_or_else(|| {
            let ids: Vec<&str> = PINNABLE.iter().map(|p| p.id).collect();
            bad(cmd, format!("unknown property {item}; one of: {}", ids.join(", ")))
        })?;
        if !out.iter().any(|o| o == found.id) {
            out.push(found.id.to_string());
        }
    }
    Ok(Some(out))
}

/// `label` from params, trimmed of trailing whitespace and capped.
fn parse_label(cmd: &str, p: &Value) -> crate::Result<Option<String>> {
    match get(p, "label").or_else(|| get(p, "text")) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim_end().chars().take(MAX_LABEL).collect())),
        Some(_) => Err(bad(cmd, "`label` must be a string")),
    }
}

fn check(cmd: &str, a: &Annotation) -> crate::Result<()> {
    if a.label.trim().is_empty() && a.properties.is_empty() {
        return Err(bad(cmd, "an annotation needs a note or a pinned property"));
    }
    Ok(())
}

/// The layer an annotation command acts on: `id`, else the first selected layer.
fn layer(cmd: &str, s: &crate::Session, p: &Value) -> crate::Result<NodeId> {
    let id = targets(s, p).first().copied().ok_or_else(|| bad(cmd, "select a layer or pass `id`"))?;
    let n = s.doc()?.node(id).ok_or_else(|| bad(cmd, "no such layer"))?;
    if n.is_page() {
        return Err(bad(cmd, "pages can't be annotated"));
    }
    Ok(id)
}

/// The annotation `index` on `id` must exist.
fn existing(cmd: &str, s: &crate::Session, id: NodeId, p: &Value) -> crate::Result<usize> {
    let index = req_index(cmd, p, "index")?;
    let count = s.doc()?.node(id).map_or(0, |n| n.annotations.len());
    if index >= count {
        return Err(bad(cmd, format!("no annotation {index} (the layer has {count})")));
    }
    Ok(index)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "annotation.add",
            "Add annotation",
            "{id? (default the first selected layer), label? (the note), properties?: [width|height|top|left|rotation|opacity|cornerRadius|direction|gap|padding|alignment|fontFamily|fontStyle|fontSize|lineHeight|letterSpacing|textAlign|fill|stroke|strokeWeight|effects]} → {id, index}  (a note, pinned properties or both)",
            true,
            |s, p| {
                let cmd = "annotation.add";
                let id = layer(cmd, s, p)?;
                let a = Annotation { label: parse_label(cmd, p)?.unwrap_or_default(), properties: parse_properties(cmd, p)?.unwrap_or_default() };
                check(cmd, &a)?;
                let n = s.doc_mut()?.get_mut(id)?;
                if n.annotations.len() >= MAX_PER_LAYER {
                    return Err(bad(cmd, format!("a layer has at most {MAX_PER_LAYER} annotations")));
                }
                n.annotations.push(a);
                Ok(json!({"id": id, "index": n.annotations.len() - 1}))
            }
        ),
        spec!("annotation.edit", "Edit annotation", "{id?, index, label?, properties?: [..] (replaces the pinned properties)}", true, |s, p| {
            let cmd = "annotation.edit";
            let id = layer(cmd, s, p)?;
            let index = existing(cmd, s, id, p)?;
            let (label, props) = (parse_label(cmd, p)?, parse_properties(cmd, p)?);
            if label.is_none() && props.is_none() {
                return Err(bad(cmd, "pass `label` and/or `properties`"));
            }
            let n = s.doc_mut()?.get_mut(id)?;
            let a = n.annotations.get_mut(index).ok_or_else(|| bad(cmd, "no such annotation"))?;
            if let Some(l) = label {
                a.label = l;
            }
            if let Some(pr) = props {
                a.properties = pr;
            }
            check(cmd, a)?;
            Ok(json!({"id": id, "index": index}))
        }),
        spec!("annotation.remove", "Remove annotation", "{id?, index? (omit: every annotation on the layer)}", true, |s, p| {
            let cmd = "annotation.remove";
            let id = layer(cmd, s, p)?;
            let index = match get(p, "index") {
                Some(_) => Some(existing(cmd, s, id, p)?),
                None => None,
            };
            let n = s.doc_mut()?.get_mut(id)?;
            if n.annotations.is_empty() {
                return Err(bad(cmd, "the layer has no annotations"));
            }
            match index {
                Some(i) if i < n.annotations.len() => {
                    n.annotations.remove(i);
                }
                Some(_) => return Err(bad(cmd, "no such annotation")),
                None => n.annotations.clear(),
            }
            Ok(json!({"id": id, "remaining": n.annotations.len()}))
        }),
        spec!(
            "annotation.list",
            "List annotations",
            "{ids? | id? (those layers), page? (page id, or \"all\"; default the current page)} → {annotations: [{id, name, index, label, properties: [{property, label, value (null when the layer has none), token?, color?}]}]}",
            false,
            |s, p| {
                let doc = s.doc()?;
                let layers: Vec<NodeId> = if get(p, "ids").is_some() || get(p, "id").is_some() {
                    targets(s, p).into_iter().filter(|i| doc.contains(*i)).collect()
                } else if string(p, "page").is_some_and(|v| v.eq_ignore_ascii_case("all")) {
                    doc.pages.iter().flat_map(|pg| on_page(doc, *pg)).take(MAX_LISTED).collect()
                } else {
                    let page = id(p, "page").unwrap_or(s.page()?);
                    if !doc.pages.contains(&page) {
                        return Err(bad("annotation.list", format!("{page} is not a page")));
                    }
                    on_page(doc, page)
                };
                let list: Vec<Value> = layers
                    .iter()
                    .filter_map(|i| doc.node(*i).map(|n| (i, n)))
                    .flat_map(|(i, n)| n.annotations.iter().enumerate().map(move |(k, a)| annotation_json(doc, *i, k, a)))
                    .collect();
                Ok(json!({"annotations": list}))
            }
        ),
    ]
}
