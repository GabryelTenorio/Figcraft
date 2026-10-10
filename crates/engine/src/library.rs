//! Local libraries (the data model is in [`figcraft_doc::library`]).
//!
//! - **Publish** ([`publish`]): the chosen components, component sets and styles, and every asset
//!   and style they use, are copied into a snapshot. Each asset keeps the library version it last
//!   changed in, so files using the library update only what changed.
//! - **Read** ([`Cache`]): library files are read read-only (only their library entries, with size
//!   limits) and again whenever they change on disk. The browser has no file system: there every
//!   read is a clear error.
//! - **Use** ([`import`], [`import_styles`]): assets are copied into the document's library store
//!   as read-only main components and styles; [`check_read_only`] refuses other edits of them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use figcraft_doc::proto::Action;
use figcraft_doc::*;
use serde_json::Value;

use crate::components::inside_instance;
use crate::params::id_of;
use crate::{EngineError, Result};

/// Largest library file read.
pub const MAX_LIBRARY_FILE: u64 = 1024 * 1024 * 1024;
/// Most library files a session remembers.
const MAX_CACHED: usize = 64;
/// Most assets one import or publish walks (with what they use).
const MAX_ASSETS: usize = 100_000;
/// Why library edits are refused.
pub const READ_ONLY: &str = "library components and styles can't be edited in this file: edit them in their library file and publish it";
#[cfg(target_arch = "wasm32")]
const NO_FILES: &str = "libraries need the desktop app: the browser has no file system";

fn disabled(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::Disabled(cmd.to_string(), msg.into())
}

fn is_asset(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Component { .. } | NodeKind::ComponentSet { .. })
}

/// Names starting with `.` or `_` keep an asset out of a library by default.
pub fn hidden_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('_')
}

/// The asset a layer belongs to: its outermost component or component set (itself included).
pub fn asset_root(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    chain.into_iter().rfind(|c| doc.node(*c).is_some_and(is_asset))
}

/// Components the layers of `root` use: mains of instances, instance swap defaults and values.
fn used_components(doc: &Document, root: NodeId) -> BTreeSet<NodeId> {
    let mut out = BTreeSet::new();
    for id in doc.subtree(root) {
        let Some(n) = doc.node(id) else { continue };
        match &n.kind {
            NodeKind::Instance { component, props } => {
                out.insert(*component);
                for k in swap_props(doc, *component) {
                    out.extend(props.get(&k).and_then(id_of));
                }
            }
            NodeKind::Component { props, .. } => {
                for d in props.values() {
                    match d {
                        PropDef::InstanceSwap { default } => {
                            out.insert(*default);
                        }
                        PropDef::Slot { preferred } => out.extend(preferred.iter().copied()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The instance swap properties of component `c`.
fn swap_props(doc: &Document, c: NodeId) -> Vec<String> {
    match doc.node(c).map(|n| &n.kind) {
        Some(NodeKind::Component { props, .. }) => {
            props.iter().filter(|(_, d)| matches!(d, PropDef::InstanceSwap { .. })).map(|(k, _)| k.clone()).collect()
        }
        _ => Vec::new(),
    }
}

/// `roots` (asset roots) and every asset they use, transitively, in discovery order.
pub fn closure(doc: &Document, roots: &[NodeId]) -> Vec<NodeId> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = roots.iter().rev().copied().collect();
    while let Some(r) = stack.pop() {
        if out.len() >= MAX_ASSETS || !seen.insert(r) {
            continue;
        }
        out.push(r);
        stack.extend(used_components(doc, r).into_iter().filter_map(|c| asset_root(doc, c)).filter(|a| !seen.contains(a)));
    }
    out
}

fn style_links(n: &Node) -> impl Iterator<Item = &String> {
    [&n.styles.fill, &n.styles.stroke, &n.styles.effect, &n.styles.grid, &n.styles.text].into_iter().flatten()
}

/// Style ids the layers under `roots` link to.
fn linked_styles(doc: &Document, roots: &[NodeId]) -> BTreeSet<String> {
    roots.iter().flat_map(|r| doc.subtree(*r)).filter_map(|i| doc.node(i)).flat_map(|n| style_links(n).cloned()).collect()
}

/// Local components and component sets, sets as one entry (never library copies or layers of
/// instances), in id order.
pub fn local_components(doc: &Document) -> Vec<NodeId> {
    let in_set = |n: &Node| n.parent.and_then(|p| doc.node(p)).is_some_and(|p| matches!(p.kind, NodeKind::ComponentSet { .. }));
    doc.nodes()
        .filter(|n| matches!(n.kind, NodeKind::Component { .. }) && !in_set(n) || matches!(n.kind, NodeKind::ComponentSet { .. }))
        .filter(|n| !doc.is_remote(n.id) && !inside_instance(doc, n.id))
        .map(|n| n.id)
        .collect()
}

/// Whether `style` was copied from a library.
pub fn is_remote_style(doc: &Document, style: &str) -> bool {
    doc.libraries.style_import(style).is_some()
}

/// Local styles (not library copies).
pub fn local_styles(doc: &Document) -> impl Iterator<Item = &Style> {
    doc.styles.iter().filter(|s| !is_remote_style(doc, &s.id))
}

/// What a file can publish: its top-level local components and sets, and its local styles.
pub fn publishable(doc: &Document) -> (Vec<NodeId>, Vec<String>) {
    let comps = local_components(doc).into_iter().filter(|c| asset_root(doc, *c) == Some(*c)).collect();
    (comps, local_styles(doc).map(|s| s.id.clone()).collect())
}

/// What a publish offers when nothing is chosen: the assets offered last time plus new ones, never
/// assets named with a leading `.` or `_`.
pub fn default_selection(doc: &Document) -> (Vec<NodeId>, Vec<String>) {
    let (comps, styles) = publishable(doc);
    let prev = doc.published.as_ref().map(|p| &p.manifest);
    let comps = comps
        .into_iter()
        .filter(|c| {
            let offered = prev.and_then(|m| m.component(*c)).map(|a| a.published);
            offered.unwrap_or_else(|| doc.node(*c).is_some_and(|n| !hidden_name(&n.name)))
        })
        .collect();
    let styles = styles
        .into_iter()
        .filter(|s| {
            let offered = prev.and_then(|m| m.style(s)).map(|x| x.published);
            offered.unwrap_or_else(|| doc.style(s).is_some_and(|x| !hidden_name(&x.name)))
        })
        .collect();
    (comps, styles)
}

/// Whether asset `root` is the same in two snapshots (its position on the page aside).
fn same_asset(a: &Document, b: &Document, root: NodeId) -> bool {
    let (sa, sb) = (a.subtree(root), b.subtree(root));
    sa == sb
        && sa.iter().all(|i| match (a.node(*i), b.node(*i)) {
            (Some(x), Some(y)) if *i == root => Node { transform: y.transform, parent: y.parent, ..x.clone() } == *y,
            (Some(x), Some(y)) => x == y,
            _ => false,
        })
}

/// A new, unique library id.
fn new_library_id(doc: &Document, seed: u64) -> String {
    let now = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = figcraft_format::image_hash(format!("{}|{now}|{seed}|{}", doc.name, doc.node_count()).as_bytes());
    h.get(..16).map(str::to_string).unwrap_or(h)
}

/// Publish `components` (asset roots) and `styles` of `doc` as a new library version. `seed` keeps
/// a first publish's new id unique. Refused when there is nothing to publish or nothing changed.
pub fn publish(doc: &Document, components: &[NodeId], styles: &[String], description: Option<String>, seed: u64) -> Result<PublishedLibrary> {
    let cmd = "library.publish";
    if components.is_empty() && styles.is_empty() {
        return Err(disabled(cmd, "nothing to publish: the file has no components or styles to offer"));
    }
    let units = closure(doc, components);
    // The snapshot keeps the file's ids, on a page whose id none of them uses.
    let mut snap = Document::new(doc.name.clone());
    let subtrees: Vec<(NodeId, Vec<NodeId>)> = units.iter().map(|u| (*u, doc.subtree(*u))).collect();
    if let Some(max) = subtrees.iter().flat_map(|(_, s)| s.iter()).max() {
        snap.reserve_ids(*max);
    }
    let page = snap.add_page("Library");
    for p in snap.pages.clone() {
        if p != page {
            snap.remove(p)?;
        }
    }
    snap.current_page = Some(page);
    for (u, sub) in &subtrees {
        let mut nodes: Vec<Node> = sub.iter().filter_map(|i| doc.node(*i).cloned()).collect();
        if let Some(r) = nodes.first_mut() {
            r.transform = doc.world_transform(*u);
        }
        let ids: BTreeMap<NodeId, NodeId> = nodes.iter().map(|n| (n.id, n.id)).collect();
        snap.paste_nodes(&nodes, &ids, *u, page, None)?;
    }
    let mut keep: BTreeSet<String> = linked_styles(doc, &units);
    keep.extend(styles.iter().cloned());
    snap.styles = doc.styles.iter().filter(|s| keep.contains(&s.id)).cloned().collect();
    for h in used_images(&snap) {
        if let Some(img) = doc.images.get(&h) {
            snap.images.insert(h, img.clone());
        }
    }
    snap.take_dirty();
    // Versions: an asset or style keeps the version it last changed in.
    let prev = doc.published.as_deref();
    let version = prev.map_or(1, |p| p.manifest.version.saturating_add(1));
    let offered: BTreeSet<NodeId> = components.iter().copied().collect();
    let components = units
        .iter()
        .filter_map(|u| {
            let n = snap.node(*u)?;
            let kept = prev.and_then(|p| p.manifest.component(*u).filter(|_| same_asset(&p.doc, &snap, *u))).map(|a| a.version);
            Some(LibraryAsset {
                id: *u,
                name: n.name.clone(),
                set: matches!(n.kind, NodeKind::ComponentSet { .. }),
                version: kept.unwrap_or(version),
                published: offered.contains(u),
            })
        })
        .collect();
    let styles = snap
        .styles
        .iter()
        .map(|s| {
            let kept = prev.and_then(|p| p.manifest.style(&s.id).filter(|_| p.doc.styles.iter().any(|x| x == s))).map(|x| x.version);
            LibraryStyle {
                id: s.id.clone(),
                name: s.name.clone(),
                kind: s.kind.type_name().into(),
                version: kept.unwrap_or(version),
                published: styles.contains(&s.id),
            }
        })
        .collect();
    let manifest = LibraryManifest {
        id: prev.map(|p| p.manifest.id.clone()).unwrap_or_else(|| new_library_id(doc, seed)),
        name: doc.name.clone(),
        version,
        description: description.or_else(|| prev.map(|p| p.manifest.description.clone())).unwrap_or_default(),
        time: crate::cmd::comment::now_secs(),
        components,
        styles,
    };
    if let Some(p) = prev
        && (LibraryManifest { version: p.manifest.version, time: p.manifest.time, ..manifest.clone() }) == p.manifest
    {
        return Err(disabled(cmd, format!("no changes to publish since version {}", p.manifest.version)));
    }
    Ok(PublishedLibrary { manifest, doc: Arc::new(snap) })
}

/// When a library file was last read.
#[derive(Clone, Debug, PartialEq)]
struct Stamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

#[derive(Clone, Debug)]
struct Entry {
    stamp: Option<Stamp>,
    result: std::result::Result<Arc<PublishedLibrary>, String>,
}

/// Library files read by a session, by path (re-read when they change on disk).
#[derive(Clone, Debug, Default)]
pub struct Cache {
    entries: HashMap<String, Entry>,
}

impl Cache {
    /// The published library in the file at `path`.
    pub fn load(&mut self, path: &str) -> Result<Arc<PublishedLibrary>> {
        self.load_limited(path, MAX_LIBRARY_FILE)
    }

    /// [`load`](Self::load), refusing files larger than `max_bytes`.
    pub fn load_limited(&mut self, path: &str, max_bytes: u64) -> Result<Arc<PublishedLibrary>> {
        let stamp = stat(path);
        // Checked before the cache: the limit is the caller's.
        if let Ok(st) = &stamp
            && st.len > max_bytes
        {
            return Err(EngineError::Other(format!("{path} is too large to use as a library (the limit is {} MB)", max_bytes / 1024 / 1024)));
        }
        if let (Some(e), Ok(st)) = (self.entries.get(path), &stamp)
            && e.stamp.as_ref() == Some(st)
        {
            return e.result.clone().map_err(EngineError::Other);
        }
        let result = match &stamp {
            Ok(_) => read_file(path).map(Arc::new),
            Err(e) => Err(e.clone()),
        };
        if self.entries.len() >= MAX_CACHED && !self.entries.contains_key(path) {
            self.entries.clear();
        }
        self.entries.insert(path.to_string(), Entry { stamp: stamp.ok(), result: result.clone() });
        result.map_err(EngineError::Other)
    }

    /// What the last read of `path` gave (no file access).
    pub fn cached(&self, path: &str) -> Option<std::result::Result<&Arc<PublishedLibrary>, &str>> {
        self.entries.get(path).map(|e| e.result.as_ref().map_err(String::as_str))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn stat(path: &str) -> std::result::Result<Stamp, String> {
    let m = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if !m.is_file() {
        return Err(format!("{path} is not a file"));
    }
    Ok(Stamp { len: m.len(), modified: m.modified().ok() })
}

#[cfg(target_arch = "wasm32")]
fn stat(_path: &str) -> std::result::Result<Stamp, String> {
    Err(NO_FILES.into())
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &str) -> std::result::Result<PublishedLibrary, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    match figcraft_format::read_library(std::io::BufReader::new(f)) {
        Ok(Some(lib)) => Ok(lib),
        Ok(None) => Err(format!("{path} has no published library: open it and publish it first (Libraries ▸ Publish library…)")),
        Err(e) => Err(format!("{path}: {e}")),
    }
}

#[cfg(target_arch = "wasm32")]
fn read_file(_path: &str) -> std::result::Result<PublishedLibrary, String> {
    Err(NO_FILES.into())
}

/// Turn `path` into an absolute path (library links outlive the working directory).
pub fn absolute_path(path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(p) = std::path::absolute(path) {
        return p.to_string_lossy().into_owned();
    }
    path.to_string()
}

/// The enabled library `key` (its id, name or path).
pub fn find_link<'a>(doc: &'a Document, key: &str) -> Option<&'a LibraryLink> {
    let l = &doc.libraries.enabled;
    l.iter().find(|l| l.id == key).or_else(|| l.iter().find(|l| l.name == key || l.path == key))
}

/// Copy images `hashes` of `src` that `doc` lacks.
fn copy_images(doc: &mut Document, src: &Document, hashes: &BTreeSet<String>) {
    for h in hashes {
        if !doc.images.contains_key(h)
            && let Some(img) = src.images.get(h)
        {
            doc.images.insert(h.clone(), img.clone());
        }
    }
}

/// Copy library styles `keys` into `doc` as linked remote styles (with `update`, copies older than
/// the library's are refreshed). Returns library style id → local id.
pub fn import_styles(doc: &mut Document, lib: &PublishedLibrary, keys: &BTreeSet<String>, update: bool) -> BTreeMap<String, String> {
    let lid = lib.manifest.id.clone();
    let mut out = BTreeMap::new();
    let mut hashes = BTreeSet::new();
    for key in keys {
        let Some(style) = lib.doc.styles.iter().find(|s| s.id == *key) else { continue };
        let version = lib.manifest.style(key).map_or(lib.manifest.version, |s| s.version);
        let record = doc.libraries.styles.iter().find(|r| r.library == lid && r.key == *key && doc.styles.iter().any(|s| s.id == r.local)).cloned();
        let local = match record {
            Some(r) => {
                if update
                    && version > r.version
                    && let Some(slot) = doc.styles.iter_mut().find(|s| s.id == r.local)
                {
                    *slot = Style { id: r.local.clone(), ..style.clone() };
                    if let Some(rec) = doc.libraries_mut().styles.iter_mut().find(|x| x.local == r.local) {
                        rec.version = version;
                    }
                }
                r.local
            }
            None => {
                let base = format!("{lid}:{key}");
                let mut local = base.clone();
                let mut k = 1;
                while doc.styles.iter().any(|s| s.id == local) && k < 10_000 {
                    k += 1;
                    local = format!("{base}#{k}");
                }
                doc.styles.push(Style { id: local.clone(), ..style.clone() });
                let libs = doc.libraries_mut();
                libs.styles.retain(|r| !(r.library == lid && r.key == *key));
                libs.styles.push(ImportedStyle { library: lid.clone(), key: key.clone(), local: local.clone(), version });
                local
            }
        };
        if let StyleKind::Paint { paints } = &style.kind {
            paint_images(paints, &mut hashes);
        }
        out.insert(key.clone(), local);
    }
    copy_images(doc, &lib.doc, &hashes);
    out
}

/// Point a layer copied from `src` at its new ids. [`Document::paste_nodes`] maps ids, children,
/// parents, instance mains and prototype destinations; this maps the rest: mirrored layers,
/// instance swap properties and (with `styles`) style links. Variable bindings (not imported) are
/// dropped with their values kept; references `map` doesn't know are dropped.
fn localize(n: &mut Node, src: &Document, map: &BTreeMap<NodeId, NodeId>, styles: Option<&BTreeMap<String, String>>) {
    n.source = n.source.and_then(|s| map.get(&s).copied());
    let swaps = match &n.kind {
        NodeKind::Instance { component, .. } if !map.contains_key(component) => {
            // Its main isn't there: keep the layers as a plain frame.
            n.kind = NodeKind::Frame;
            Vec::new()
        }
        NodeKind::Instance { component, .. } => swap_props(src, *component),
        _ => Vec::new(),
    };
    match &mut n.kind {
        NodeKind::Instance { props, .. } => {
            for k in swaps {
                match props.get(&k).and_then(id_of).and_then(|c| map.get(&c)) {
                    Some(l) => {
                        props.insert(k, Value::from(l.0));
                    }
                    None => {
                        props.remove(&k);
                    }
                }
            }
        }
        NodeKind::Component { props, .. } => {
            props.retain(|_, d| !matches!(d, PropDef::InstanceSwap { default } if !map.contains_key(default)));
            for d in props.values_mut() {
                match d {
                    PropDef::InstanceSwap { default } => {
                        if let Some(l) = map.get(default) {
                            *default = *l;
                        }
                    }
                    PropDef::Slot { preferred } => *preferred = preferred.iter().filter_map(|c| map.get(c).copied()).collect(),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    if let Some(styles) = styles {
        for r in [&mut n.styles.fill, &mut n.styles.stroke, &mut n.styles.effect, &mut n.styles.grid, &mut n.styles.text] {
            *r = r.take().and_then(|s| styles.get(&s).cloned());
        }
    }
    n.bound.clear();
    n.var_modes.clear();
    n.plugin_data.remove("figcraft.thumbnail");
    for it in &mut n.interactions {
        it.actions.retain(|a| !matches!(a, Action::SetVariable { .. }) && a.dest().is_none_or(|d| map.contains_key(&d)));
    }
    n.interactions.retain(|it| !it.actions.is_empty());
}

/// Copy library assets `keys` (any layer of an asset names it), with every asset and style they
/// use, into `doc`'s library store. Copies the document already has are kept, or with `update`
/// re-imported in place (same layer ids, so instances keep their overrides) when the library has
/// a newer version. Returns library id → local id for every layer of those assets.
pub fn import(doc: &mut Document, lib: &PublishedLibrary, keys: &[NodeId], update: bool) -> Result<BTreeMap<NodeId, NodeId>> {
    let src = lib.doc.as_ref();
    let lid = lib.manifest.id.clone();
    let roots: Vec<NodeId> = keys.iter().filter_map(|k| asset_root(src, *k)).collect();
    if roots.is_empty() {
        return Err(EngineError::Other("no such component in the library".into()));
    }
    let units = closure(src, &roots);
    let store = doc.ensure_library_store();
    let records: BTreeMap<NodeId, ImportedComponent> =
        doc.libraries.components.iter().filter(|r| r.library == lid && doc.contains(r.root)).map(|r| (r.key, r.clone())).collect();
    let version_of = |u: NodeId| lib.manifest.component(u).map_or(lib.manifest.version, |a| a.version);
    let write: Vec<(NodeId, Vec<NodeId>)> =
        units.iter().filter(|u| records.get(u).is_none_or(|r| update && version_of(**u) > r.version)).map(|u| (*u, src.subtree(*u))).collect();
    // Kept copies keep their ids; re-imported ones reuse them where the library layer remains.
    let mut map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    for u in &units {
        if let Some(r) = records.get(u)
            && !write.iter().any(|(w, _)| w == u)
        {
            map.extend(r.ids.iter().map(|(a, b)| (*a, *b)));
        }
    }
    let mut used: BTreeSet<NodeId> = map.values().copied().collect();
    for (u, sub) in &write {
        let old = records.get(u);
        let old_layers: BTreeSet<NodeId> = old.map(|r| doc.subtree(r.root).into_iter().collect()).unwrap_or_default();
        for id in sub {
            let reuse = old.and_then(|r| r.ids.get(id)).copied().filter(|l| (old_layers.contains(l) || !doc.contains(*l)) && !used.contains(l));
            let local = reuse.unwrap_or_else(|| doc.alloc_id());
            used.insert(local);
            map.insert(*id, local);
        }
    }
    let roots_written: Vec<NodeId> = write.iter().map(|(u, _)| *u).collect();
    let styles = import_styles(doc, lib, &linked_styles(src, &roots_written), update);
    for (u, _) in &write {
        if let Some(r) = records.get(u)
            && doc.contains(r.root)
        {
            doc.remove(r.root)?;
        }
    }
    let mut hashes = BTreeSet::new();
    for (u, sub) in &write {
        let mut nodes = Vec::with_capacity(sub.len());
        for id in sub {
            let Some(mut n) = src.node(*id).cloned() else { continue };
            localize(&mut n, src, &map, Some(&styles));
            node_images(&n, &mut hashes);
            nodes.push(n);
        }
        doc.paste_nodes(&nodes, &map, *u, store, None)?;
    }
    copy_images(doc, src, &hashes);
    for (u, sub) in &write {
        let Some(root) = map.get(u).copied() else { continue };
        let ids = sub.iter().filter_map(|i| map.get(i).map(|l| (*i, *l))).collect();
        let rec = ImportedComponent { library: lid.clone(), key: *u, root, version: version_of(*u), ids };
        let libs = doc.libraries_mut();
        match libs.components.iter_mut().find(|r| r.library == lid && r.key == *u) {
            Some(r) => *r = rec,
            None => libs.components.push(rec),
        }
    }
    Ok(map)
}

/// Pending updates of `doc`'s copies of library `lib`: (component records, style records).
pub fn pending(doc: &Document, lib: &PublishedLibrary) -> (Vec<ImportedComponent>, Vec<ImportedStyle>) {
    let m = &lib.manifest;
    let comps = doc
        .libraries
        .components
        .iter()
        .filter(|r| r.library == m.id && doc.contains(r.root) && m.component(r.key).is_some_and(|a| a.version > r.version))
        .cloned()
        .collect();
    let styles = doc
        .libraries
        .styles
        .iter()
        .filter(|r| r.library == m.id && doc.style(&r.local).is_some() && m.style(&r.key).is_some_and(|s| s.version > r.version))
        .cloned()
        .collect();
    (comps, styles)
}

/// Pull library `lib`'s updates of the copies `doc` has (instances keep their overrides).
/// Returns how many components and styles changed.
pub fn accept_updates(doc: &mut Document, lib: &PublishedLibrary) -> Result<(usize, usize)> {
    let (comps, styles) = pending(doc, lib);
    if !comps.is_empty() {
        let keys: Vec<NodeId> = comps.iter().map(|r| r.key).collect();
        import(doc, lib, &keys, true)?;
    }
    if !styles.is_empty() {
        import_styles(doc, lib, &styles.iter().map(|r| r.key.clone()).collect(), true);
    }
    let m = &lib.manifest;
    if let Some(l) = doc.libraries_mut().enabled.iter_mut().find(|l| l.id == m.id)
        && (l.version != m.version || l.name != m.name)
    {
        l.version = m.version;
        l.name = m.name.clone();
    }
    prune(doc, &m.id)?;
    Ok((comps.len(), styles.len()))
}

/// Drop copies of `library`'s components and styles that nothing in the document uses any more.
pub fn prune(doc: &mut Document, library: &str) -> Result<()> {
    for _ in 0..MAX_ASSETS {
        // Which copies instances outside them use.
        let mut used: BTreeSet<NodeId> = BTreeSet::new();
        for n in doc.nodes() {
            if let NodeKind::Instance { component, .. } = &n.kind
                && let Some(root) = asset_root(doc, *component)
                && !doc.is_ancestor(root, n.id)
            {
                used.insert(root);
            }
        }
        let unused: Vec<ImportedComponent> =
            doc.libraries.components.iter().filter(|r| r.library == library && !used.contains(&r.root)).cloned().collect();
        if unused.is_empty() {
            break;
        }
        for r in &unused {
            if doc.contains(r.root) && doc.is_remote(r.root) {
                doc.remove(r.root)?;
            }
        }
        doc.libraries_mut().components.retain(|r| !unused.contains(r));
    }
    let linked: BTreeSet<String> = doc.nodes().flat_map(|n| style_links(n).cloned()).collect();
    let unused: Vec<String> =
        doc.libraries.styles.iter().filter(|r| r.library == library && !linked.contains(&r.local)).map(|r| r.local.clone()).collect();
    if !unused.is_empty() {
        doc.styles.retain(|s| !unused.contains(&s.id));
        doc.libraries_mut().styles.retain(|r| !unused.contains(&r.local));
    }
    Ok(())
}

/// When pages of `other` are appended to `doc` (`map`: their ids → the pasted ids): the library
/// copies they use come along (shared with the copies `doc` has of the same assets), with their
/// records and library links, and the pasted layers point at them.
pub fn merge_store(doc: &mut Document, other: &Document, map: &BTreeMap<NodeId, NodeId>) -> Result<()> {
    if other.library_store().is_none() {
        return Ok(());
    }
    let mut remote: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    let mut fresh = Vec::new();
    for r in other.libraries.components.iter().filter(|r| other.contains(r.root)) {
        match doc.libraries.components.iter().find(|m| m.library == r.library && m.key == r.key && doc.contains(m.root)) {
            Some(mine) => remote.extend(r.ids.iter().filter_map(|(lib, theirs)| mine.ids.get(lib).map(|l| (*theirs, *l)))),
            None => fresh.push(r.clone()),
        }
    }
    for r in &fresh {
        for id in other.subtree(r.root) {
            remote.insert(id, doc.alloc_id());
        }
    }
    if !fresh.is_empty() {
        let store = doc.ensure_library_store();
        for r in &fresh {
            let mut nodes: Vec<Node> = other.subtree(r.root).into_iter().filter_map(|i| other.node(i).cloned()).collect();
            for n in &mut nodes {
                localize(n, other, &remote, None);
            }
            let Some(root) = remote.get(&r.root).copied() else { continue };
            doc.paste_nodes(&nodes, &remote, r.root, store, None)?;
            let ids = r.ids.iter().filter_map(|(lib, theirs)| remote.get(theirs).map(|l| (*lib, *l))).collect();
            doc.libraries_mut().components.push(ImportedComponent { root, ids, ..r.clone() });
        }
    }
    // The pasted layers still name `other`'s copies: point them at `doc`'s.
    for (theirs, id) in map {
        let Some(n) = other.node(*theirs) else { continue };
        let main = match &n.kind {
            NodeKind::Instance { component, .. } => remote.get(component).copied(),
            _ => None,
        };
        let source = n.source.and_then(|s| remote.get(&s).copied());
        if (main.is_none() && source.is_none()) || !doc.contains(*id) {
            continue;
        }
        let n = doc.get_mut(*id)?;
        if let (NodeKind::Instance { component, .. }, Some(m)) = (&mut n.kind, main) {
            *component = m;
        }
        if source.is_some() {
            n.source = source;
        }
    }
    let libs = doc.libraries_mut();
    for s in &other.libraries.styles {
        if !libs.styles.iter().any(|x| x.library == s.library && x.key == s.key) {
            libs.styles.push(s.clone());
        }
    }
    for l in &other.libraries.enabled {
        if libs.link(&l.id).is_none() {
            libs.enabled.push(l.clone());
        }
    }
    Ok(())
}

/// Refuse edits of library copies, except by library commands (and imports of other files,
/// which bring their copies along).
pub fn check_read_only(cmd: &str, before: &Document, after: &Document) -> std::result::Result<(), String> {
    if cmd.starts_with("library.") || cmd == "file.import" {
        return Ok(());
    }
    let Some(store) = after.library_store().or_else(|| before.library_store()) else { return Ok(()) };
    let touched = |d: &Document, id: NodeId| d.contains(id) && d.page_of(id) == Some(store);
    if after.dirty().iter().any(|id| touched(before, *id) || touched(after, *id)) {
        return Err(READ_ONLY.into());
    }
    Ok(())
}

/// The library a remote layer was copied from (its id), if any.
pub fn library_of(doc: &Document, id: NodeId) -> Option<&str> {
    if !doc.is_remote(id) {
        return None;
    }
    let root = asset_root(doc, id)?;
    doc.libraries.import_of(root).map(|r| r.library.as_str())
}

/// The display name of library `id` in `doc` (its link's name, else the id).
pub fn library_name<'a>(doc: &'a Document, id: &'a str) -> &'a str {
    doc.libraries.link(id).map_or(id, |l| l.name.as_str())
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
