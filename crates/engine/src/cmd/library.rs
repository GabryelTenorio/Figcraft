//! Library commands: publish this file as a library; enable, use and update other library files
//! (their components, styles and variables).

use std::collections::BTreeSet;
use std::sync::Arc;

use figcraft_doc::*;
use serde_json::{Value, json};

use super::CommandSpec;
use crate::library::*;
use crate::params::*;
use crate::{EngineError, Result, Session, bad, spec};

/// Longest library description kept (chars).
const MAX_DESCRIPTION: usize = 2000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("libraries.publish", "Publish library…", "{}  (opens the publish form in the Assets panel)", false, |s, _| {
            s.request_ui("panel.libraries", json!({"publish": true}));
            Ok(json!({"ok": true}))
        }),
        spec!(
            "library.publish",
            "Publish library",
            "{description?, include? [component/set ids, style ids, variable collection ids or names: exactly these], exclude? [same]}  publishes this file's components, component sets, styles and variable collections (by default all but names starting with . or _, or what the last publish offered; variables hidden from publishing stay out unless something published uses them) as a new library version; save the file so other files see it",
            true,
            publish_cmd
        ),
        spec!(
            "library.list",
            "List libraries",
            "{assets? (also list each library's components, styles, variable collections and variables, and what this file can publish)}  → enabled libraries (version, updates, errors) and this file's published library",
            false,
            list
        ),
        spec!("library.add", "Add library", "{path (.figcraft file published as a library)}", true, add),
        spec!(
            "library.remove",
            "Remove library",
            "{id (library id, name or path)}  (copies still in use stay; unused ones are dropped)",
            true,
            remove
        ),
        spec!(
            "library.updates",
            "Library updates",
            "{}  → the components, styles, variable collections and variables with a newer published version",
            false,
            updates
        ),
        spec!(
            "library.acceptUpdates",
            "Accept library updates",
            "{ids? [library ids; all when omitted]}  pulls updated components, styles, variable collections and variables in (instances keep their overrides; bound layers follow)",
            true,
            accept
        ),
        spec!(
            "library.place",
            "Place library component",
            "{library (id, name or path), component (id or name of a published component, set or variant), x?, y? (page point; default the view's centre)}",
            true,
            place
        ),
        spec!(
            "library.swapAt",
            "Swap instance to library component",
            "{library, component (id or name of a published component, set or variant), x, y (page point)}  copies the component in and swaps the instance there to it (as instance.swapAt)",
            true,
            |s, p| {
                let (_, lib, key) = component_arg(s, "library.swapAt", p)?;
                let map = import(s.doc_mut()?, &lib, &[key], false)?;
                let local = map.get(&key).copied().ok_or_else(|| EngineError::Other("the component didn't import".into()))?;
                super::component::swap_at(s, &with(p, &["x", "y"], json!({"component": local})))
            }
        ),
        spec!(
            "library.import",
            "Import library component",
            "{library, component}  → {component (local, read-only main to instantiate with instance.create)}",
            true,
            |s, p| {
                let (_, lib, key) = component_arg(s, "library.import", p)?;
                let map = import(s.doc_mut()?, &lib, &[key], false)?;
                let local = map.get(&key).copied().ok_or_else(|| EngineError::Other("the component didn't import".into()))?;
                Ok(json!({"component": local}))
            }
        ),
        spec!(
            "library.applyStyle",
            "Apply library style",
            "{library, style (id or name of a published style), ids?, target? (fill|stroke)}  copies the style in (linked, read-only) and applies it",
            true,
            apply_style
        ),
        spec!(
            "library.importVariable",
            "Import library variable",
            "{library, variable (id or name of a published variable)}  → {variable (local, read-only linked copy to use with variable.bind, or as an alias in variable.set)}",
            true,
            |s, p| {
                let (lib, key) = variable_arg(s, "library.importVariable", p, "variable")?;
                Ok(json!({"variable": import_variable(s, &lib, &key)?}))
            }
        ),
        spec!(
            "library.bindVariable",
            "Bind library variable",
            "{library, variable (id or name of a published variable), field (as variable.bind), ids?}  copies the variable in (linked, read-only) and binds it",
            true,
            |s, p| {
                let (lib, key) = variable_arg(s, "library.bindVariable", p, "variable")?;
                let local = import_variable(s, &lib, &key)?;
                super::styles::bind(s, &with(p, &["ids", "id", "field"], json!({"variable": local})))?;
                Ok(json!({"variable": local}))
            }
        ),
        spec!(
            "library.aliasVariable",
            "Alias library variable",
            "{library, alias (id or name of a published variable of the same type), variable (a local variable), mode? (default first)}  copies the library variable in (linked, read-only) and makes the local variable's value an alias of it",
            true,
            alias
        ),
        spec!(
            "library.setVariableMode",
            "Set library variable mode",
            "{library, collection (id or name of a published collection), mode (id or name), ids? (frames; the page when omitted)}  copies the collection in (linked, read-only) and sets its mode on the frames",
            true,
            set_mode
        ),
    ]
}

/// `base` with `p`'s values of `keys` (those it has).
fn with(p: &Value, keys: &[&str], mut base: Value) -> Value {
    for k in keys {
        if let Some(v) = get(p, k) {
            base[*k] = v.clone();
        }
    }
    base
}

/// An enabled library of the active document (`library` param), read from its file.
fn library(s: &mut Session, cmd: &str, p: &Value) -> Result<(LibraryLink, Arc<PublishedLibrary>)> {
    let key = req_string(cmd, p, "library")?;
    let link = find_link(s.doc()?, &key)
        .cloned()
        .ok_or_else(|| bad(cmd, format!("no enabled library `{key}` (library.list shows them; library.add enables one)")))?;
    let lib = s.libraries.load(&link.path)?;
    if lib.manifest.id != link.id {
        return Err(EngineError::Other(format!(
            "{} now holds another library (\"{}\"): remove \"{}\" and add the file again",
            link.path, lib.manifest.name, link.name
        )));
    }
    Ok((link, lib))
}

/// The `library` and published `component` params: (link, library, the component's snapshot id).
fn component_arg(s: &mut Session, cmd: &str, p: &Value) -> Result<(LibraryLink, Arc<PublishedLibrary>, NodeId)> {
    let (link, lib) = library(s, cmd, p)?;
    let v = get(p, "component").ok_or_else(|| bad(cmd, "`component` (id or name) required"))?;
    let m = &lib.manifest;
    let offered = |id: NodeId| asset_root(&lib.doc, id).and_then(|r| m.component(r)).is_some_and(|a| a.published);
    let by_name = || v.as_str().and_then(|t| m.components.iter().find(|a| a.published && a.name == t)).map(|a| a.id);
    let key = id_of(v).filter(|i| offered(*i)).or_else(by_name).ok_or_else(|| bad(cmd, format!("\"{}\" publishes no component {v}", link.name)))?;
    Ok((link, lib, key))
}

/// The `library` and a published variable named by `param` (id, else name): (library, the
/// variable's id in the library).
fn variable_arg(s: &mut Session, cmd: &str, p: &Value, param: &str) -> Result<(Arc<PublishedLibrary>, VariableId)> {
    let (link, lib) = library(s, cmd, p)?;
    let key = req_string(cmd, p, param)?;
    let id = published_variables(&lib).find(|v| v.id == key).or_else(|| published_variables(&lib).find(|v| v.name == key)).map(|v| v.id.clone());
    let id = id.ok_or_else(|| bad(cmd, format!("\"{}\" publishes no variable `{key}`", link.name)))?;
    Ok((lib, id))
}

/// Copy library variable `key` in (with what it aliases); its local id.
fn import_variable(s: &mut Session, lib: &PublishedLibrary, key: &str) -> Result<VariableId> {
    let map = import_variables(s.doc_mut()?, lib, &BTreeSet::from([key.to_string()]), &BTreeSet::new(), false);
    map.variables.get(key).cloned().ok_or_else(|| EngineError::Other("the variable didn't import".into()))
}

fn alias(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.aliasVariable";
    let own = req_string(cmd, p, "variable")?;
    let doc = s.doc()?;
    let kind = doc.variables.get(&own).filter(|v| !is_remote_variable(doc, &v.id)).map(|v| v.kind);
    let kind = kind.ok_or_else(|| bad(cmd, format!("no local variable `{own}`")))?;
    let (lib, key) = variable_arg(s, cmd, p, "alias")?;
    if lib.doc.variables.get(&key).is_some_and(|v| v.kind != kind) {
        return Err(bad(cmd, "the library variable has another type"));
    }
    let local = import_variable(s, &lib, &key)?;
    super::styles::var_set(s, &with(p, &["variable", "mode"], json!({"value": {"alias": local}})))?;
    Ok(json!({"variable": local}))
}

fn set_mode(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.setVariableMode";
    let (link, lib) = library(s, cmd, p)?;
    let key = req_string(cmd, p, "collection")?;
    let coll = published_collections(&lib)
        .find(|c| c.id == key)
        .or_else(|| published_collections(&lib).find(|c| c.name == key))
        .ok_or_else(|| bad(cmd, format!("\"{}\" publishes no variable collection `{key}`", link.name)))?;
    let m = req_string(cmd, p, "mode")?;
    let mode = coll.modes.iter().find(|x| x.id == m || x.name == m).map(|x| x.id.clone()).ok_or_else(|| bad(cmd, "no such mode"))?;
    let key = coll.id.clone();
    let map = import_variables(s.doc_mut()?, &lib, &BTreeSet::new(), &BTreeSet::from([key.clone()]), false);
    let local = map.collections.get(&key).cloned().ok_or_else(|| EngineError::Other("the collection didn't import".into()))?;
    super::styles::set_mode(s, &with(p, &["ids", "id"], json!({"collection": local, "mode": mode})))?;
    Ok(json!({"collection": local}))
}

fn publish_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.publish";
    let doc = s.doc()?;
    let all = publishable(doc);
    let coll_named = |c: &String, t: &str| doc.variables.collections.iter().any(|x| x.id == *c && x.name == t);
    let count = |s: &Selection| s.components.len() + s.styles.len() + s.collections.len();
    let pick = |key: &str| -> Result<Selection> {
        let mut sel = Selection::default();
        let Some(v) = get(p, key) else { return Ok(sel) };
        let Value::Array(items) = v else {
            return Err(bad(cmd, format!("`{key}` must be an array of component ids and style or variable collection ids or names")));
        };
        for item in items {
            let text = item.as_str().map(str::trim);
            let before = count(&sel);
            if let Some(t) = text.filter(|t| all.styles.iter().any(|x| x == t)) {
                sel.styles.push(t.to_string());
            } else if let Some(t) = text.filter(|t| all.collections.iter().any(|x| x == t)) {
                sel.collections.push(t.to_string());
            } else if let Some(i) = id_of(item).filter(|i| all.components.contains(i)) {
                sel.components.push(i);
            } else if let Some(t) = text {
                sel.components.extend(all.components.iter().filter(|i| doc.node(**i).is_some_and(|n| n.name == t)));
                sel.styles.extend(all.styles.iter().filter(|x| doc.style(x).is_some_and(|y| y.name == t)).cloned());
                sel.collections.extend(all.collections.iter().filter(|c| coll_named(c, t)).cloned());
            }
            if count(&sel) == before {
                return Err(bad(cmd, format!("{item} is not a local component, component set, style or variable collection of this file")));
            }
        }
        Ok(sel)
    };
    let mut sel = if get(p, "include").is_some() { pick("include")? } else { default_selection(doc) };
    let x = pick("exclude")?;
    sel.components.retain(|c| !x.components.contains(c));
    sel.styles.retain(|s| !x.styles.contains(s));
    sel.collections.retain(|c| !x.collections.contains(c));
    let description = string(p, "description").map(|d| d.trim().chars().take(MAX_DESCRIPTION).collect());
    let lib = publish(doc, &sel, description, s.ds()?.uid)?;
    let m = &lib.manifest;
    let out = json!({
        "id": m.id,
        "version": m.version,
        "components": sel.components.len(),
        "styles": sel.styles.len(),
        "collections": sel.collections.len(),
        "variables": m.variables.iter().filter(|v| v.published).count(),
    });
    s.doc_mut()?.published = Some(Arc::new(lib));
    Ok(out)
}

fn assets_json(lib: &PublishedLibrary) -> Value {
    let m = &lib.manifest;
    let modes = |c: &str| lib.doc.variables.collection(c).map(|c| c.modes.iter().map(|x| json!({"id": x.id, "name": x.name})).collect::<Vec<_>>());
    json!({
        "components": m.components.iter().filter(|a| a.published).map(|a| json!({"id": a.id, "name": a.name, "set": a.set, "version": a.version})).collect::<Vec<_>>(),
        "styles": m.styles.iter().filter(|x| x.published).map(|x| json!({"id": x.id, "name": x.name, "type": x.kind, "version": x.version})).collect::<Vec<_>>(),
        "collections": m.collections.iter().filter(|x| x.published).map(|x| json!({"id": x.id, "name": x.name, "modes": modes(&x.id), "version": x.version})).collect::<Vec<_>>(),
        "variables": m.variables.iter().filter(|x| x.published).map(|x| json!({"id": x.id, "name": x.name, "type": x.kind, "collection": x.collection, "version": x.version})).collect::<Vec<_>>(),
    })
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let with_assets = boolean(p, "assets").unwrap_or(false);
    let links = s.doc()?.libraries.enabled.clone();
    let mut out = Vec::new();
    for l in links {
        let mut e = json!({"id": l.id, "name": l.name, "path": l.path, "seenVersion": l.version});
        match s.libraries.load(&l.path) {
            Ok(lib) if lib.manifest.id == l.id => {
                e["version"] = json!(lib.manifest.version);
                e["description"] = json!(lib.manifest.description);
                e["updates"] = json!(pending(s.doc()?, &lib).count());
                if with_assets && let (Value::Object(e), Value::Object(a)) = (&mut e, assets_json(&lib)) {
                    e.extend(a);
                }
            }
            Ok(lib) => e["error"] = json!(format!("the file now holds another library (\"{}\")", lib.manifest.name)),
            Err(err) => e["error"] = json!(err.to_string()),
        }
        out.push(e);
    }
    let doc = s.doc()?;
    let published = doc.published.as_ref().map(|lib| {
        let m = &lib.manifest;
        let mut v = json!({"id": m.id, "name": m.name, "version": m.version, "description": m.description, "time": m.time,
            "components": m.components.iter().filter(|a| a.published).count(), "styles": m.styles.iter().filter(|x| x.published).count(),
            "collections": m.collections.iter().filter(|x| x.published).count(), "variables": m.variables.iter().filter(|x| x.published).count()});
        if with_assets {
            v["assets"] = assets_json(lib);
        }
        v
    });
    let mut res = json!({"libraries": out, "published": published});
    if with_assets {
        let (def, all) = (default_selection(doc), publishable(doc));
        res["publishable"] = json!({
            "components": all.components.iter().filter_map(|c| doc.node(*c).map(|n| json!({"id": c, "name": n.name, "selected": def.components.contains(c)}))).collect::<Vec<_>>(),
            "styles": all.styles.iter().filter_map(|x| doc.style(x).map(|st| json!({"id": st.id, "name": st.name, "selected": def.styles.contains(x)}))).collect::<Vec<_>>(),
            "collections": all.collections.iter().filter_map(|x| doc.variables.collection(x).map(|c| json!({"id": c.id, "name": c.name, "selected": def.collections.contains(x)}))).collect::<Vec<_>>(),
        });
    }
    Ok(res)
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.add";
    let path = absolute_path(req_string(cmd, p, "path")?.trim());
    let lib = s.libraries.load(&path)?;
    let m = &lib.manifest;
    let doc = s.doc()?;
    if doc.published.as_ref().is_some_and(|me| me.manifest.id == m.id) {
        return Err(EngineError::Disabled(cmd.into(), format!("this file is the library \"{}\"", m.name)));
    }
    let unchanged = doc.libraries.link(&m.id).is_some_and(|l| l.path == path && l.name == m.name);
    if !unchanged {
        let libs = s.doc_mut()?.libraries_mut();
        match libs.enabled.iter_mut().find(|l| l.id == m.id) {
            // The same library from another place (the file moved): follow it.
            Some(l) => {
                l.path = path.clone();
                l.name = m.name.clone();
            }
            None => libs.enabled.push(LibraryLink { id: m.id.clone(), name: m.name.clone(), path: path.clone(), version: m.version }),
        }
    }
    let mut out = assets_json(&lib);
    out["id"] = json!(m.id);
    out["name"] = json!(m.name);
    out["version"] = json!(m.version);
    out["path"] = json!(path);
    Ok(out)
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.remove";
    let key = req_string(cmd, p, "id")?;
    let id = find_link(s.doc()?, &key).map(|l| l.id.clone()).ok_or_else(|| bad(cmd, format!("no enabled library `{key}`")))?;
    let doc = s.doc_mut()?;
    doc.libraries_mut().enabled.retain(|l| l.id != id);
    prune(doc, &id)?;
    Ok(json!({"removed": id}))
}

fn updates(s: &mut Session, _p: &Value) -> Result<Value> {
    let links = s.doc()?.libraries.enabled.clone();
    let (mut out, mut errors, mut count) = (Vec::new(), Vec::new(), 0);
    for l in links {
        let lib = match s.libraries.load(&l.path) {
            Ok(lib) if lib.manifest.id == l.id => lib,
            Ok(_) => {
                errors.push(json!({"id": l.id, "name": l.name, "error": "the file now holds another library"}));
                continue;
            }
            Err(e) => {
                errors.push(json!({"id": l.id, "name": l.name, "error": e.to_string()}));
                continue;
            }
        };
        let p = pending(s.doc()?, &lib);
        if p.is_empty() {
            continue;
        }
        count += p.count();
        let m = &lib.manifest;
        let name = |r: &ImportedComponent| m.component(r.key).map(|a| a.name.clone()).unwrap_or_default();
        let rows = |list: &[ImportedRecord], name: &dyn Fn(&str) -> Option<String>| {
            list.iter().map(|r| json!({"id": r.local, "key": r.key, "name": name(&r.key)})).collect::<Vec<_>>()
        };
        out.push(json!({
            "id": l.id,
            "name": m.name,
            "version": m.version,
            "components": p.components.iter().map(|r| json!({"id": r.root, "key": r.key, "name": name(r)})).collect::<Vec<_>>(),
            "styles": rows(&p.styles, &|k| m.style(k).map(|x| x.name.clone())),
            "collections": rows(&p.collections, &|k| m.collection(k).map(|x| x.name.clone())),
            "variables": rows(&p.variables, &|k| m.variable(k).map(|x| x.name.clone())),
        }));
    }
    Ok(json!({"count": count, "libraries": out, "errors": errors}))
}

fn accept(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.acceptUpdates";
    let named: Option<Vec<String>> = match get(p, "ids") {
        Some(Value::Array(a)) => Some(a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()),
        Some(Value::String(one)) => Some(vec![one.clone()]),
        Some(_) => return Err(bad(cmd, "`ids` must be an array of library ids")),
        None => None,
    };
    let links: Vec<LibraryLink> = match &named {
        Some(keys) => keys
            .iter()
            .map(|k| find_link(s.doc()?, k).cloned().ok_or_else(|| bad(cmd, format!("no enabled library `{k}`"))))
            .collect::<Result<_>>()?,
        None => s.doc()?.libraries.enabled.clone(),
    };
    let (mut done, mut skipped) = (Pending::default(), Vec::new());
    for l in links {
        let lib = match library(s, cmd, &json!({"library": l.id})) {
            Ok((_, lib)) => lib,
            Err(e) if named.is_none() => {
                skipped.push(json!({"id": l.id, "name": l.name, "error": e.to_string()}));
                continue;
            }
            Err(e) => return Err(e),
        };
        let p = accept_updates(s.doc_mut()?, &lib)?;
        done.components.extend(p.components);
        done.styles.extend(p.styles);
        done.collections.extend(p.collections);
        done.variables.extend(p.variables);
    }
    Ok(json!({
        "components": done.components.len(),
        "styles": done.styles.len(),
        "collections": done.collections.len(),
        "variables": done.variables.len(),
        "skipped": skipped,
    }))
}

fn place(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.place";
    let (_, lib, key) = component_arg(s, cmd, p)?;
    let (cx, cy) = s.ds().map(|d| (d.view.cx, d.view.cy))?;
    let (x, y) = (num(p, "x").unwrap_or(cx), num(p, "y").unwrap_or(cy));
    let map = import(s.doc_mut()?, &lib, &[key], false)?;
    let local = map.get(&key).copied().ok_or_else(|| EngineError::Other("the component didn't import".into()))?;
    let mut out = super::component::place(s, &json!({"component": local, "x": x, "y": y}))?;
    out["component"] = json!(local);
    Ok(out)
}

fn apply_style(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.applyStyle";
    let (link, lib) = library(s, cmd, p)?;
    let key = req_string(cmd, p, "style")?;
    let m = &lib.manifest;
    let style = m
        .styles
        .iter()
        .filter(|x| x.published)
        .find(|x| x.id == key)
        .or_else(|| m.styles.iter().filter(|x| x.published).find(|x| x.name == key))
        .map(|x| x.id.clone())
        .ok_or_else(|| bad(cmd, format!("\"{}\" publishes no style `{key}`", link.name)))?;
    let local = import_styles(s.doc_mut()?, &lib, &[style].into_iter().collect(), false)
        .into_values()
        .next()
        .ok_or_else(|| EngineError::Other("the style didn't import".into()))?;
    super::styles::apply(s, &with(p, &["ids", "id", "target"], json!({"style": local})))?;
    Ok(json!({"style": local}))
}
