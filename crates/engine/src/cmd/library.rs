//! Library commands: publish this file as a library; enable, use and update other library files.

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
            "{description?, include? [component/set ids, style ids or names: exactly these], exclude? [same]}  publishes this file's components, component sets and styles (by default all but names starting with . or _, or what the last publish offered) as a new library version; save the file so other files see it",
            true,
            publish_cmd
        ),
        spec!(
            "library.list",
            "List libraries",
            "{assets? (also list each library's components and styles, and what this file can publish)}  → enabled libraries (version, updates, errors) and this file's published library",
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
        spec!("library.updates", "Library updates", "{}  → the components and styles with a newer published version", false, updates),
        spec!(
            "library.acceptUpdates",
            "Accept library updates",
            "{ids? [library ids; all when omitted]}  pulls updated components and styles in (instances keep their overrides)",
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
    ]
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

fn publish_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "library.publish";
    let doc = s.doc()?;
    let (comps_all, styles_all) = publishable(doc);
    let pick = |key: &str| -> Result<(Vec<NodeId>, Vec<String>)> {
        let Some(v) = get(p, key) else { return Ok((Vec::new(), Vec::new())) };
        let Value::Array(items) = v else { return Err(bad(cmd, format!("`{key}` must be an array of component ids and style ids or names"))) };
        let (mut c, mut st) = (Vec::new(), Vec::new());
        for item in items {
            let text = item.as_str().map(str::trim);
            let before = (c.len(), st.len());
            if let Some(t) = text.filter(|t| styles_all.iter().any(|x| x == t)) {
                st.push(t.to_string());
            } else if let Some(i) = id_of(item).filter(|i| comps_all.contains(i)) {
                c.push(i);
            } else if let Some(t) = text {
                c.extend(comps_all.iter().filter(|i| doc.node(**i).is_some_and(|n| n.name == t)));
                st.extend(styles_all.iter().filter(|x| doc.style(x).is_some_and(|y| y.name == t)).cloned());
            }
            if (c.len(), st.len()) == before {
                return Err(bad(cmd, format!("{item} is not a local component, component set or style of this file")));
            }
        }
        Ok((c, st))
    };
    let (mut comps, mut styles) = if get(p, "include").is_some() { pick("include")? } else { default_selection(doc) };
    let (xc, xs) = pick("exclude")?;
    comps.retain(|c| !xc.contains(c));
    styles.retain(|x| !xs.contains(x));
    let description = string(p, "description").map(|d| d.trim().chars().take(MAX_DESCRIPTION).collect());
    let lib = publish(doc, &comps, &styles, description, s.ds()?.uid)?;
    let out = json!({
        "id": lib.manifest.id,
        "version": lib.manifest.version,
        "components": comps.len(),
        "styles": styles.len(),
    });
    s.doc_mut()?.published = Some(Arc::new(lib));
    Ok(out)
}

fn assets_json(lib: &PublishedLibrary) -> Value {
    let m = &lib.manifest;
    json!({
        "components": m.components.iter().filter(|a| a.published).map(|a| json!({"id": a.id, "name": a.name, "set": a.set, "version": a.version})).collect::<Vec<_>>(),
        "styles": m.styles.iter().filter(|x| x.published).map(|x| json!({"id": x.id, "name": x.name, "type": x.kind, "version": x.version})).collect::<Vec<_>>(),
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
                let (c, st) = pending(s.doc()?, &lib);
                e["version"] = json!(lib.manifest.version);
                e["description"] = json!(lib.manifest.description);
                e["updates"] = json!(c.len() + st.len());
                if with_assets {
                    let a = assets_json(&lib);
                    e["components"] = a["components"].clone();
                    e["styles"] = a["styles"].clone();
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
            "components": m.components.iter().filter(|a| a.published).count(), "styles": m.styles.iter().filter(|x| x.published).count()});
        if with_assets {
            v["assets"] = assets_json(lib);
        }
        v
    });
    let mut res = json!({"libraries": out, "published": published});
    if with_assets {
        let (comps, styles) = default_selection(doc);
        let (all_c, all_s) = publishable(doc);
        res["publishable"] = json!({
            "components": all_c.iter().filter_map(|c| doc.node(*c).map(|n| json!({"id": c, "name": n.name, "selected": comps.contains(c)}))).collect::<Vec<_>>(),
            "styles": all_s.iter().filter_map(|x| doc.style(x).map(|st| json!({"id": st.id, "name": st.name, "selected": styles.contains(x)}))).collect::<Vec<_>>(),
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
        let doc = s.doc()?;
        let (comps, styles) = pending(doc, &lib);
        if comps.is_empty() && styles.is_empty() {
            continue;
        }
        count += comps.len() + styles.len();
        let name = |r: &ImportedComponent| lib.manifest.component(r.key).map(|a| a.name.clone()).unwrap_or_default();
        out.push(json!({
            "id": l.id,
            "name": lib.manifest.name,
            "version": lib.manifest.version,
            "components": comps.iter().map(|r| json!({"id": r.root, "key": r.key, "name": name(r)})).collect::<Vec<_>>(),
            "styles": styles.iter().map(|r| json!({"id": r.local, "key": r.key, "name": lib.manifest.style(&r.key).map(|x| x.name.clone())})).collect::<Vec<_>>(),
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
    let (mut comps, mut styles, mut skipped) = (0, 0, Vec::new());
    for l in links {
        let lib = match library(s, cmd, &json!({"library": l.id})) {
            Ok((_, lib)) => lib,
            Err(e) if named.is_none() => {
                skipped.push(json!({"id": l.id, "name": l.name, "error": e.to_string()}));
                continue;
            }
            Err(e) => return Err(e),
        };
        let (c, st) = accept_updates(s.doc_mut()?, &lib)?;
        comps += c;
        styles += st;
    }
    Ok(json!({"components": comps, "styles": styles, "skipped": skipped}))
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
    let mut q = json!({"style": local});
    for k in ["ids", "id", "target"] {
        if let Some(v) = get(p, k) {
            q[k] = v.clone();
        }
    }
    super::styles::apply(s, &q)?;
    Ok(json!({"style": local}))
}
