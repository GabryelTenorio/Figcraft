//! Editing variables, modes and collections (the Variables editor): rename, describe, scope,
//! duplicate and delete; alias checks; and which scope a bindable property belongs to.

use std::collections::BTreeSet;

use figcraft_doc::*;
use serde_json::{Value, json};

use super::{CommandSpec, ok};
use crate::params::*;
use crate::{Result, Session, bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "variable.update",
            "Edit variable",
            "{variable, name?, description?, scopes? ([scope ids]; null or [\"ALL_SCOPES\"] = every supported property, [] = hidden from pickers)}",
            true,
            update
        ),
        spec!("variable.duplicate", "Duplicate variable", "{variable} → {id}", true, duplicate),
        spec!("variable.renameMode", "Rename mode", "{collection, mode, name}", true, rename_mode),
        spec!("variable.removeMode", "Delete mode", "{collection, mode} (a collection keeps at least one mode)", true, remove_mode),
        spec!("variable.renameCollection", "Rename collection", "{collection, name}", true, rename_collection),
        spec!(
            "variable.deleteCollection",
            "Delete collection",
            "{collection} (with its variables; bound layers keep their values)",
            true,
            delete_collection
        ),
    ]
}

/// A variable name with each `/`-separated group part trimmed; None when nothing is left.
fn clean_name(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('/').map(str::trim).filter(|s| !s.is_empty()).collect();
    let out = parts.join("/");
    (!out.is_empty()).then(|| out.chars().take(500).collect())
}

fn update(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "variable.update";
    let v = req_string(cmd, p, "variable")?;
    let doc = s.doc_mut()?;
    let var = doc.variables.get(&v).cloned().ok_or_else(|| bad(cmd, "no such variable"))?;
    let name = match string(p, "name") {
        Some(n) => {
            let n = clean_name(&n).ok_or_else(|| bad(cmd, "name can't be empty"))?;
            if doc.variables.variables.iter().any(|x| x.id != var.id && x.collection == var.collection && x.name == n) {
                return Err(bad(cmd, format!("a variable named `{n}` already exists in this collection")));
            }
            Some(n)
        }
        None => None,
    };
    let scopes = match get(p, "scopes") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::Array(a)) => {
            let mut ids = Vec::with_capacity(a.len());
            for x in a {
                let id = x.as_str().ok_or_else(|| bad(cmd, "scopes: [scope ids]"))?;
                if !scope_known(var.kind, id) {
                    return Err(bad(cmd, format!("`{id}` isn't a scope for {} variables", var.kind.id())));
                }
                ids.push(id);
            }
            // Every scope listed is the same as no limit.
            Some(normalize_scopes(var.kind, ids))
        }
        Some(_) => return Err(bad(cmd, "scopes: [scope ids] or null")),
    };
    let slot = doc.variables.variables.iter_mut().find(|x| x.id == var.id).ok_or_else(|| bad(cmd, "no such variable"))?;
    if let Some(n) = name {
        slot.name = n;
    }
    if let Some(d) = string(p, "description") {
        slot.description = d.chars().take(10_000).collect();
    }
    if let Some(sc) = scopes {
        slot.scopes = sc;
    }
    ok()
}

fn duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "variable.duplicate";
    let v = req_string(cmd, p, "variable")?;
    let doc = s.doc_mut()?;
    let pos = doc.variables.variables.iter().position(|x| x.id == v || x.name == v).ok_or_else(|| bad(cmd, "no such variable"))?;
    let mut copy = doc.variables.variables.get(pos).cloned().ok_or_else(|| bad(cmd, "no such variable"))?;
    let taken = |n: &str| doc.variables.variables.iter().any(|x| x.collection == copy.collection && x.name == n);
    let name = (1..1000)
        .map(|k| if k == 1 { format!("{} copy", copy.name) } else { format!("{} copy {k}", copy.name) })
        .find(|n| !taken(n))
        .ok_or_else(|| bad(cmd, "too many copies"))?;
    copy.id = super::styles::unique_id("VariableID", doc.variables.variables.iter().map(|x| x.id.as_str()));
    copy.name = name;
    let id = copy.id.clone();
    doc.variables.variables.insert(pos + 1, copy);
    Ok(json!({"id": id}))
}

/// The index of collection `c` (id or name).
fn collection_index(doc: &Document, cmd: &str, c: &str) -> Result<usize> {
    doc.variables.collections.iter().position(|x| x.id == c || x.name == c).ok_or_else(|| bad(cmd, "no such collection"))
}

/// Collection `c`'s index and the id of its mode `m` (id or name).
fn mode_of(doc: &Document, cmd: &str, p: &Value) -> Result<(usize, ModeId)> {
    let ci = collection_index(doc, cmd, &req_string(cmd, p, "collection")?)?;
    let m = req_string(cmd, p, "mode")?;
    let mode = doc
        .variables
        .collections
        .get(ci)
        .and_then(|c| c.modes.iter().find(|x| x.id == m || x.name == m))
        .map(|x| x.id.clone())
        .ok_or_else(|| bad(cmd, "no such mode"))?;
    Ok((ci, mode))
}

fn rename_mode(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "variable.renameMode";
    let name = req_string(cmd, p, "name")?.trim().chars().take(200).collect::<String>();
    if name.is_empty() {
        return Err(bad(cmd, "name can't be empty"));
    }
    let doc = s.doc_mut()?;
    let (ci, mode) = mode_of(doc, cmd, p)?;
    if let Some(m) = doc.variables.collections.get_mut(ci).and_then(|c| c.modes.iter_mut().find(|x| x.id == mode)) {
        m.name = name;
    }
    ok()
}

fn remove_mode(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "variable.removeMode";
    let doc = s.doc_mut()?;
    let (ci, mode) = mode_of(doc, cmd, p)?;
    let coll = doc.variables.collections.get_mut(ci).ok_or_else(|| bad(cmd, "no such collection"))?;
    if coll.modes.len() < 2 {
        return Err(bad(cmd, "a collection keeps at least one mode"));
    }
    coll.modes.retain(|m| m.id != mode);
    let cid = coll.id.clone();
    for v in doc.variables.variables.iter_mut().filter(|v| v.collection == cid) {
        v.values.remove(&mode);
    }
    // Layers set to the deleted mode go back to the default.
    let users: Vec<NodeId> = doc.nodes().filter(|n| n.var_modes.get(&cid) == Some(&mode)).map(|n| n.id).collect();
    for id in users {
        doc.get_mut(id)?.var_modes.remove(&cid);
    }
    ok()
}

fn rename_collection(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "variable.renameCollection";
    let name = req_string(cmd, p, "name")?.trim().chars().take(200).collect::<String>();
    if name.is_empty() {
        return Err(bad(cmd, "name can't be empty"));
    }
    let doc = s.doc_mut()?;
    let ci = collection_index(doc, cmd, &req_string(cmd, p, "collection")?)?;
    if doc.variables.collections.iter().enumerate().any(|(i, c)| i != ci && c.name == name) {
        return Err(bad(cmd, format!("a collection named `{name}` already exists")));
    }
    if let Some(c) = doc.variables.collections.get_mut(ci) {
        c.name = name;
    }
    ok()
}

fn delete_collection(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "variable.deleteCollection";
    let doc = s.doc_mut()?;
    let ci = collection_index(doc, cmd, &req_string(cmd, p, "collection")?)?;
    let cid = doc.variables.collections.remove(ci).id;
    let gone: BTreeSet<VariableId> = doc.variables.variables.iter().filter(|v| v.collection == cid).map(|v| v.id.clone()).collect();
    doc.variables.variables.retain(|v| v.collection != cid);
    // Layers keep their current values but no longer follow the deleted variables or modes.
    let users: Vec<NodeId> =
        doc.nodes().filter(|n| n.var_modes.contains_key(&cid) || n.bound.values().any(|v| gone.contains(v))).map(|n| n.id).collect();
    for id in users {
        let n = doc.get_mut(id)?;
        n.var_modes.remove(&cid);
        n.bound.retain(|_, v| !gone.contains(v));
    }
    Ok(json!({"deletedVariables": gone.len()}))
}

/// Check that `var` may alias `target` (id or name): it exists, has the same type and doesn't
/// lead back to `var`. Returns the target's id.
pub fn alias_target(vars: &Variables, var: &Variable, target: &str) -> std::result::Result<VariableId, String> {
    let t = vars.get(target).ok_or_else(|| format!("no variable `{target}` to alias"))?;
    if t.kind != var.kind {
        return Err(format!("can't alias a {} variable to a {} one", var.kind.id(), t.kind.id()));
    }
    let mut stack = vec![t.id.clone()];
    let mut seen = BTreeSet::new();
    while let Some(cur) = stack.pop() {
        if cur == var.id {
            return Err("the alias would refer back to itself".into());
        }
        if !seen.insert(cur.clone()) || seen.len() > 10_000 {
            continue;
        }
        for v in vars.get(&cur).map(|x| x.values.values()).into_iter().flatten() {
            if let VariableValue::Alias(a) = v
                && let Some(next) = vars.get(a)
            {
                stack.push(next.id.clone());
            }
        }
    }
    Ok(t.id.clone())
}

/// The variable scope of bindable property `field` on `node` (None: any variable of the type fits).
pub fn field_scope(doc: &Document, node: NodeId, field: &str) -> Option<&'static str> {
    Some(match field {
        "width" | "height" | "minWidth" | "maxWidth" | "minHeight" | "maxHeight" => "WIDTH_HEIGHT",
        "gap" | "columnGap" | "rowGap" | "crossGap" | "padding" | "paddingHorizontal" | "paddingVertical" | "paddingTop" | "paddingRight"
        | "paddingBottom" | "paddingLeft" => "GAP",
        "cornerRadius" | "topLeftRadius" | "topRightRadius" | "bottomRightRadius" | "bottomLeftRadius" => "CORNER_RADIUS",
        "opacity" => "OPACITY",
        "strokeWeight" => "STROKE_FLOAT",
        "fontSize" => "FONT_SIZE",
        "lineHeight" => "LINE_HEIGHT",
        "letterSpacing" => "LETTER_SPACING",
        "characters" => "TEXT_CONTENT",
        "fontFamily" => "FONT_FAMILY",
        "fontStyle" => "FONT_STYLE",
        "stroke" | "strokes/0/color" => "STROKE_COLOR",
        "fill" | "fills" | "fills/0" | "fills/0/color" => match doc.node(node).map(|n| &n.kind) {
            Some(NodeKind::Text(_)) => "TEXT_FILL",
            Some(k) if k.is_frame_like() => "FRAME_FILL",
            _ => "SHAPE_FILL",
        },
        _ => return None,
    })
}
