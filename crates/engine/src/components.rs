//! Components and instances.
//!
//! An instance's layers are real nodes copied from its main component; each copy records the
//! main-component node it mirrors (`source`) and which property groups were changed locally
//! (`overrides`). When a main component changes, every instance is rebuilt from it, keeping layer
//! ids and re-applying overrides (matched by source, then by layer name path, so switching
//! variants keeps compatible overrides). Variants are components inside a component set, named
//! `Property=Value, …`; component properties (boolean, text, instance swap) are applied through
//! `prop_refs` on the layers they control.
//!
//! Override groups: `fills`, `strokes`, `effects`, `opacity`, `visible`, `name`, `corners`,
//! `characters`, `textStyle`, `size`, `position`, `component` (a swapped nested instance) and
//! `props` (property values of a nested instance).
//!
//! Instances never gain or lose layers of their own: commands that would add, remove or reorder
//! layers inside an instance are refused ([`check_structure_edit`]).

use std::collections::{BTreeMap, BTreeSet};

use figcraft_doc::*;
use figcraft_geom::Affine;
use serde_json::Value;

/// Nesting limit for instances inside instances.
const MAX_DEPTH: usize = 16;
/// Every override group a layer can carry.
pub const OVERRIDE_GROUPS: &[&str] =
    &["fills", "strokes", "effects", "opacity", "visible", "name", "corners", "characters", "textStyle", "size", "position", "component", "props"];

fn is_instance(doc: &Document, id: NodeId) -> bool {
    doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Instance { .. }))
}

/// The component a node belongs to (itself, or the nearest component ancestor).
pub fn owning_component(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    chain.into_iter().find(|c| doc.node(*c).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. })))
}

/// The innermost instance containing `id` (`id` itself when it is an instance).
pub fn owning_instance(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    chain.into_iter().find(|c| is_instance(doc, *c))
}

/// The outermost instance containing `id` (`id` itself when it is a top-level instance).
pub fn root_instance(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    chain.into_iter().rfind(|c| is_instance(doc, *c))
}

/// Whether `id` is a layer inside an instance (not the outermost instance itself).
pub fn inside_instance(doc: &Document, id: NodeId) -> bool {
    doc.node(id).and_then(|n| n.parent).is_some_and(|p| owning_instance(doc, p).is_some())
}

/// All instances of `component` in the document.
pub fn instances_of(doc: &Document, component: NodeId) -> Vec<NodeId> {
    doc.nodes().filter(|n| matches!(n.kind, NodeKind::Instance { component: c, .. } if c == component)).map(|n| n.id).collect()
}

/// The component set a variant belongs to.
pub fn variant_set(doc: &Document, component: NodeId) -> Option<NodeId> {
    doc.node(component).and_then(|c| c.parent).filter(|p| doc.node(*p).is_some_and(|n| matches!(n.kind, NodeKind::ComponentSet { .. })))
}

/// The layer name a fresh instance of `component` gets: the set's name for variants.
pub fn default_instance_name(doc: &Document, component: NodeId) -> String {
    let set = variant_set(doc, component).and_then(|s| doc.node(s));
    set.or_else(|| doc.node(component)).map(|n| n.name.clone()).unwrap_or_else(|| "Instance".into())
}

/// Default property values of a component.
pub fn default_props(comp: &Node) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    if let NodeKind::Component { props, .. } = &comp.kind {
        for (k, d) in props {
            out.insert(k.clone(), prop_default_value(d));
        }
    }
    out
}

/// A property definition's default as a JSON value.
pub fn prop_default_value(d: &PropDef) -> Value {
    match d {
        PropDef::Boolean { default } => Value::Bool(*default),
        PropDef::Text { default } => Value::String(default.clone()),
        PropDef::InstanceSwap { default } => Value::from(default.0),
        PropDef::Variant { default, .. } => Value::String(default.clone()),
    }
}

/// Create an instance of `component` under `parent` at `(x, y)` (parent space).
pub fn create_instance(doc: &mut Document, component: NodeId, parent: NodeId, x: f64, y: f64) -> Result<NodeId, DocError> {
    create_instance_at(doc, component, parent, x, y, None)
}

/// [`create_instance`] at child `index` of `parent` (None: on top).
pub fn create_instance_at(doc: &mut Document, component: NodeId, parent: NodeId, x: f64, y: f64, index: Option<usize>) -> Result<NodeId, DocError> {
    let comp = doc.get(component)?.clone();
    if !matches!(comp.kind, NodeKind::Component { .. }) {
        return Err(DocError::Invalid(format!("{component} is not a component")));
    }
    if doc.is_ancestor(component, parent) {
        return Err(DocError::Invalid("an instance can't be placed inside its own main component".into()));
    }
    let id = doc.alloc_id();
    let mut root = comp.clone();
    root.id = id;
    root.name = default_instance_name(doc, component);
    root.kind = NodeKind::Instance { component, props: default_props(&comp) };
    root.children.clear();
    root.parent = None;
    root.source = Some(component);
    root.overrides.clear();
    root.transform = Affine::translate((x, y));
    root.prop_refs.clear();
    doc.insert(root, parent, index)?;
    sync_instance(doc, id);
    Ok(id)
}

/// Which part of the carried overrides to apply.
#[derive(Clone, Copy, PartialEq)]
enum Carry {
    /// Paint, text, visibility, … (everything but geometry).
    Values,
    /// Values plus size (instance roots: their size decides how children follow constraints).
    Root,
    /// Size and position (children, after constraints ran).
    Geometry,
}

/// Copy the overridden property groups of `from` onto `to`.
fn carry_overrides(from: &Node, to: &mut Node, what: Carry) {
    for g in &from.overrides {
        let geo = matches!(g.as_str(), "size" | "position");
        match what {
            Carry::Values if geo => continue,
            Carry::Root if g == "position" => continue,
            Carry::Geometry if !geo => continue,
            _ => {}
        }
        match g.as_str() {
            "fills" => {
                to.fills = from.fills.clone();
                to.styles.fill = from.styles.fill.clone();
            }
            "strokes" => {
                to.strokes = from.strokes.clone();
                to.stroke = from.stroke.clone();
                to.styles.stroke = from.styles.stroke.clone();
            }
            "effects" => {
                to.effects = from.effects.clone();
                to.styles.effect = from.styles.effect.clone();
            }
            "opacity" => {
                to.opacity = from.opacity;
                to.blend = from.blend;
            }
            "visible" => to.visible = from.visible,
            "name" => to.name = from.name.clone(),
            "corners" => to.corners = from.corners,
            "characters" => {
                if let (NodeKind::Text(a), NodeKind::Text(b)) = (&from.kind, &mut to.kind) {
                    b.characters = a.characters.clone();
                    b.runs = a.runs.clone();
                }
            }
            "textStyle" => {
                if let (NodeKind::Text(a), NodeKind::Text(b)) = (&from.kind, &mut to.kind) {
                    b.style = a.style.clone();
                    b.runs = a.runs.clone();
                    b.align_h = a.align_h;
                    b.align_v = a.align_v;
                }
                to.styles.text = from.styles.text.clone();
            }
            "size" => {
                to.width = from.width;
                to.height = from.height;
                to.child_layout = from.child_layout.clone();
                if let (NodeKind::Text(a), NodeKind::Text(b)) = (&from.kind, &mut to.kind) {
                    b.auto_resize = a.auto_resize;
                }
            }
            "position" => to.transform = from.transform,
            "component" => {
                if let (NodeKind::Instance { component: a, .. }, NodeKind::Instance { component: b, .. }) = (&from.kind, &mut to.kind) {
                    *b = *a;
                }
            }
            "props" => {
                if let (NodeKind::Instance { props: a, .. }, NodeKind::Instance { props: b, .. }) = (&from.kind, &mut to.kind) {
                    *b = a.clone();
                }
            }
            _ => {}
        }
    }
    if what != Carry::Geometry {
        to.overrides = from.overrides.clone();
    }
}

/// Layer name paths below `root` ("Frame/Label", with `#n` for same-named siblings). With
/// `src_names`, layers are named after the main layer they mirror (renames don't break matching).
fn path_map(doc: &Document, root: NodeId, src_names: bool) -> BTreeMap<NodeId, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![(root, String::new())];
    while let Some((id, prefix)) = stack.pop() {
        let Some(n) = doc.node(id) else { continue };
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for c in &n.children {
            let Some(cn) = doc.node(*c) else { continue };
            let name =
                if src_names { cn.source.and_then(|s| doc.node(s)).map(|s| s.name.as_str()).unwrap_or(cn.name.as_str()) } else { cn.name.as_str() };
            let k = seen.entry(name).or_insert(0);
            let seg = if *k == 0 { name.to_string() } else { format!("{name}#{k}") };
            *k += 1;
            let path = if prefix.is_empty() { seg } else { format!("{prefix}/{seg}") };
            out.insert(*c, path.clone());
            if out.len() > 1_000_000 {
                return out;
            }
            stack.push((*c, path));
        }
    }
    out
}

/// The override group a property-bound field belongs to.
fn field_group(field: &str) -> &'static str {
    match field {
        "visible" => "visible",
        "characters" => "characters",
        "mainComponent" => "component",
        _ => "",
    }
}

/// Apply one component-property value to a layer.
fn apply_prop(doc: &Document, n: &mut Node, field: &str, val: &Value) {
    match field {
        "visible" => {
            if let Some(b) = val.as_bool() {
                n.visible = b;
            }
        }
        "characters" => {
            if let (NodeKind::Text(t), Some(sv)) = (&mut n.kind, val.as_str())
                && t.characters != sv
            {
                let len = t.char_count();
                t.replace_range(0, len, sv);
            }
        }
        "mainComponent" => {
            if let (NodeKind::Instance { component: c, .. }, Some(v)) = (&mut n.kind, crate::params::id_of(val))
                && doc.node(v).is_some_and(|m| matches!(m.kind, NodeKind::Component { .. }))
            {
                *c = v;
            }
        }
        _ => {}
    }
}

/// Rebuild instance `inst` from its main component, keeping ids and overrides. An instance whose
/// main component was deleted keeps its layers as they are.
pub fn sync_instance(doc: &mut Document, inst: NodeId) {
    sync_instance_depth(doc, inst, 0);
}

fn sync_instance_depth(doc: &mut Document, inst: NodeId, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(root) = doc.node(inst).cloned() else { return };
    let NodeKind::Instance { component, props: root_props } = &root.kind else { return };
    let component = *component;
    let Some(comp) = doc.node(component).cloned() else { return };
    if !matches!(comp.kind, NodeKind::Component { .. }) || doc.is_ancestor(inst, component) || doc.is_ancestor(component, inst) {
        // A missing main, or an instance inside its own main (would recurse forever).
        return;
    }
    let mut eff_props = default_props(&comp);
    for (k, v) in root_props {
        if eff_props.contains_key(k) {
            eff_props.insert(k.clone(), v.clone());
        }
    }
    // Old layers, by (parent, source) and by name path.
    let old_paths = path_map(doc, inst, true);
    let mut by_source: BTreeMap<(NodeId, NodeId), Node> = BTreeMap::new();
    let mut by_path: BTreeMap<String, Node> = BTreeMap::new();
    for (i, path) in &old_paths {
        let Some(n) = doc.node(*i) else { continue };
        if let (Some(s), Some(p)) = (n.source, n.parent) {
            by_source.entry((p, s)).or_insert_with(|| n.clone());
        }
        by_path.insert(path.clone(), n.clone());
    }
    for c in root.children.clone() {
        let _ = doc.remove(c);
    }
    // Ids for the copies: reuse the matching old layer's id.
    let src: Vec<NodeId> = doc.subtree(component).into_iter().filter(|i| *i != component).collect();
    let main_paths = path_map(doc, component, false);
    let mut map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    map.insert(component, inst);
    let mut olds: BTreeMap<NodeId, Node> = BTreeMap::new();
    let mut used: BTreeSet<NodeId> = BTreeSet::new();
    for s in &src {
        let Some(sn) = doc.node(*s) else { continue };
        let parent_new = sn.parent.and_then(|p| map.get(&p).copied());
        let old = parent_new.and_then(|pn| by_source.get(&(pn, *s))).or_else(|| main_paths.get(s).and_then(|p| by_path.get(p)));
        let nid = match old {
            Some(o) if !doc.contains(o.id) && used.insert(o.id) => {
                olds.insert(o.id, o.clone());
                o.id
            }
            _ => doc.alloc_id(),
        };
        map.insert(*s, nid);
    }
    // Build the copies (pre-order: parents first).
    let mut enclosing: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    enclosing.insert(inst, vec![inst]);
    let mut inst_props: BTreeMap<NodeId, BTreeMap<String, Value>> = BTreeMap::new();
    inst_props.insert(inst, eff_props);
    let mut nested = Vec::new();
    let mut new_nodes = Vec::new();
    for s in &src {
        let Some(orig) = doc.node(*s).cloned() else { continue };
        let Some(&nid) = map.get(s) else { continue };
        let Some(parent) = orig.parent.and_then(|p| map.get(&p).copied()) else { continue };
        let mut n = orig.clone();
        n.id = nid;
        n.parent = Some(parent);
        n.children = orig.children.iter().filter_map(|c| map.get(c).copied()).collect();
        n.source = Some(*s);
        n.overrides.clear();
        n.prop_refs.clear();
        if let Some(old) = olds.get(&nid) {
            carry_overrides(old, &mut n, Carry::Values);
        }
        // Component properties: the main layer's bindings use this instance's values; layers of
        // nested instances use the nested instance's values (one level per source hop).
        let enc = enclosing.get(&parent).cloned().unwrap_or_default();
        let mut chain = vec![orig.clone()];
        while chain.len() < enc.len() {
            let Some(next) = chain.last().and_then(|c| c.source).and_then(|x| doc.node(x)).cloned() else { break };
            chain.push(next);
        }
        for (j, level) in chain.iter().enumerate().rev() {
            let Some(props) = enc.get(j).and_then(|e| inst_props.get(e)) else { continue };
            for (field, prop) in &level.prop_refs {
                let g = field_group(field);
                if n.overrides.contains(g) || (j > 0 && orig.overrides.contains(g)) {
                    continue;
                }
                if let Some(val) = props.get(prop) {
                    apply_prop(doc, &mut n, field, val);
                }
            }
        }
        if let NodeKind::Instance { component: c, props } = &n.kind {
            inst_props.insert(nid, props.clone());
            let mut e = enc.clone();
            e.push(nid);
            enclosing.insert(nid, e);
            if !matches!(orig.kind, NodeKind::Instance { component: oc, .. } if oc == *c) {
                nested.push(nid);
            }
        } else {
            enclosing.insert(nid, enc);
        }
        new_nodes.push(n);
    }
    for n in new_nodes {
        let (id, parent) = (n.id, n.parent);
        if insert_linked(doc, n).is_err()
            && let Some(p) = parent.and_then(|p| doc.node_mut(p))
        {
            p.children.retain(|c| *c != id);
        }
    }
    // Root: follows the main except its own placement, name, size and overrides.
    let order: Vec<NodeId> = comp.children.iter().filter_map(|c| map.get(c).copied()).filter(|c| doc.contains(*c)).collect();
    let mut fresh = comp.clone();
    fresh.id = inst;
    fresh.parent = root.parent;
    fresh.children = order;
    fresh.transform = root.transform;
    fresh.kind = root.kind.clone();
    // A nested instance keeps mirroring its layer of the outer main.
    fresh.source = if inside_instance(doc, inst) { root.source } else { Some(component) };
    fresh.constraints = root.constraints;
    fresh.child_layout = root.child_layout.clone();
    fresh.interactions = root.interactions.clone();
    fresh.exports = root.exports.clone();
    fresh.prop_refs = root.prop_refs.clone();
    fresh.plugin_data = root.plugin_data.clone();
    fresh.locked = root.locked;
    fresh.visible = root.visible;
    fresh.var_modes = root.var_modes.clone();
    fresh.is_mask = root.is_mask;
    fresh.name = default_instance_name(doc, component);
    carry_overrides(&root, &mut fresh, Carry::Root);
    if let Some(r) = doc.node_mut(inst) {
        *r = fresh;
    }
    // A resized instance: children follow their constraints (auto layout re-flows later).
    let (iw, ih) = doc.node(inst).map(|n| (n.width, n.height)).unwrap_or((comp.width, comp.height));
    if (iw - comp.width).abs() > 1e-9 || (ih - comp.height).abs() > 1e-9 {
        figcraft_layout::apply_constraints(doc, inst, comp.width, comp.height);
    }
    // Geometry overrides of children win over constraints.
    for s in &src {
        let Some(nid) = map.get(s).copied() else { continue };
        let Some(old) = olds.get(&nid) else { continue };
        if !old.overrides.contains("size") && !old.overrides.contains("position") {
            continue;
        }
        let Some((ow, oh)) = doc.node(nid).map(|n| (n.width, n.height)) else { continue };
        if old.overrides.contains("size") {
            // Through set_size so vector networks scale with the box.
            figcraft_layout::set_size(doc, nid, old.width, old.height);
            if doc.node(nid).is_some_and(|n| n.kind.is_frame_like() && n.layout.is_none()) {
                figcraft_layout::apply_constraints(doc, nid, ow, oh);
            }
        }
        if let Some(n) = doc.node_mut(nid) {
            carry_overrides(old, n, Carry::Geometry);
        }
    }
    // Text whose characters or style differ from the main re-measures.
    for nid in map.values() {
        if let Some(n) = doc.node(*nid)
            && let Some((w, h)) = figcraft_text::auto_size(n)
            && ((w - n.width).abs() > 1e-6 || (h - n.height).abs() > 1e-6)
        {
            figcraft_layout::set_size(doc, *nid, w, h);
        }
    }
    // Nested instances showing another main than the main's copy (swapped): rebuild from it.
    for n in nested {
        sync_instance_depth(doc, n, depth + 1);
    }
}

/// Insert a node whose parent/children links are already set (used when rebuilding instances).
fn insert_linked(doc: &mut Document, n: Node) -> Result<(), DocError> {
    let parent = n.parent.ok_or_else(|| DocError::Invalid("no parent".into()))?;
    let id = n.id;
    let mut n = n;
    let kids = std::mem::take(&mut n.children);
    doc.insert(n, parent, None)?;
    if let Some(m) = doc.node_mut(id) {
        m.children = kids;
    }
    // The parent may already list us (its children were copied with ids): keep the first.
    if let Some(p) = doc.node_mut(parent) {
        let mut seen = BTreeSet::new();
        p.children.retain(|c| seen.insert(*c));
    }
    Ok(())
}

/// Property groups that differ between two versions of a node.
pub fn diff_groups(a: &Node, b: &Node) -> Vec<&'static str> {
    let mut out = Vec::new();
    if a.fills != b.fills {
        out.push("fills");
    }
    if a.strokes != b.strokes || a.stroke != b.stroke {
        out.push("strokes");
    }
    if a.effects != b.effects {
        out.push("effects");
    }
    if a.opacity != b.opacity || a.blend != b.blend {
        out.push("opacity");
    }
    if a.visible != b.visible {
        out.push("visible");
    }
    if a.name != b.name {
        out.push("name");
    }
    if a.corners != b.corners {
        out.push("corners");
    }
    if (a.width - b.width).abs() > 1e-6 || (a.height - b.height).abs() > 1e-6 {
        out.push("size");
    }
    if a.transform != b.transform {
        out.push("position");
    }
    if let (NodeKind::Text(x), NodeKind::Text(y)) = (&a.kind, &b.kind) {
        if x.characters != y.characters {
            out.push("characters");
        }
        if x.style != y.style || x.runs != y.runs || x.align_h != y.align_h || x.align_v != y.align_v {
            out.push("textStyle");
        }
    }
    if let (NodeKind::Instance { component: x, props: px }, NodeKind::Instance { component: y, props: py }) = (&a.kind, &b.kind) {
        if x != y {
            out.push("component");
        }
        if px != py {
            out.push("props");
        }
    }
    out
}

/// Whether a geometry change of `id` comes from layout rather than the user: an ancestor was
/// resized in the same edit (constraints), the parent is an auto layout frame, or auto-sized text.
fn geometry_derived(doc: &Document, before: &Document, id: NodeId, group: &str) -> bool {
    for a in doc.ancestors(id) {
        let (Some(x), Some(y)) = (doc.node(a), before.node(a)) else { continue };
        if (x.width - y.width).abs() > 1e-6 || (x.height - y.height).abs() > 1e-6 {
            return true;
        }
    }
    let Some(n) = doc.node(id) else { return true };
    if group == "position" && n.parent.and_then(|p| doc.node(p)).is_some_and(|p| p.layout.is_some()) && !n.child_layout.absolute {
        return true;
    }
    if group == "size"
        && let NodeKind::Text(t) = &n.kind
    {
        let wchanged = before.node(id).is_some_and(|b| (b.width - n.width).abs() > 1e-6);
        return match t.auto_resize {
            TextAutoResize::WidthAndHeight => true,
            TextAutoResize::Height => !wchanged,
            _ => false,
        };
    }
    false
}

/// After an edit: record overrides inside instances, then rebuild instances whose main changed,
/// whose properties or main were switched, or whose overrides were reset.
pub fn sync_after_edit(doc: &mut Document, before: &Document, changed: &[NodeId], touched: &BTreeSet<NodeId>) {
    let mut resync: BTreeSet<NodeId> = BTreeSet::new();
    // 1. Overrides for layers edited inside instances.
    for &id in changed {
        let (Some(n), Some(b)) = (doc.node(id), before.node(id)) else { continue };
        let Some(inst) = owning_instance(doc, id) else { continue };
        let top_root = inst == id && !inside_instance(doc, id);
        if top_root
            && let (NodeKind::Instance { component: c1, props: p1 }, NodeKind::Instance { component: c2, props: p2 }) = (&n.kind, &b.kind)
            && (c1 != c2 || p1 != p2)
        {
            // Switching the main or a property value of an instance isn't an override.
            resync.insert(id);
        }
        let raw = diff_groups(b, n);
        let mut groups = raw.clone();
        if top_root {
            groups.retain(|g| !matches!(*g, "component" | "props" | "position"));
        }
        if groups.iter().any(|g| matches!(*g, "size" | "position")) {
            groups.retain(|g| !matches!(*g, "size" | "position") || !geometry_derived(doc, before, id, g));
        }
        // A value equal to the main's isn't an override (setting it back clears the override).
        let mut cleared = Vec::new();
        if let Some(src) = n.source.and_then(|s| doc.node(s)) {
            let mut differs = diff_groups(src, n);
            if inst == id {
                differs.retain(|g| *g != "name");
                if n.name != default_instance_name(doc, src.id) {
                    differs.push("name");
                }
            }
            groups.retain(|g| differs.contains(g));
            cleared =
                raw.iter().filter(|g| !matches!(**g, "size" | "position") && !differs.contains(g) && n.overrides.contains(**g)).copied().collect();
            if top_root {
                cleared.retain(|g| !matches!(*g, "component" | "props"));
            }
        }
        let lost = b.overrides.iter().any(|g| !n.overrides.contains(g));
        let nested_switch = !top_root && raw.iter().any(|g| matches!(*g, "component" | "props"));
        if (!groups.is_empty() || !cleared.is_empty())
            && let Some(m) = doc.node_mut(id)
        {
            for g in groups {
                m.overrides.insert(g.to_string());
            }
            for g in cleared {
                m.overrides.remove(g);
            }
        }
        if (lost || nested_switch)
            && let Some(r) = root_instance(doc, id)
        {
            resync.insert(r);
        }
    }
    // 2. Components whose layers changed, were added or removed.
    let mut comps: BTreeSet<NodeId> = BTreeSet::new();
    for &id in changed {
        if let Some(c) = owning_component(doc, id) {
            comps.insert(c);
        }
    }
    for &id in touched {
        if !doc.contains(id)
            && let Some(c) = owning_component(before, id)
            && doc.contains(c)
        {
            comps.insert(c);
        }
    }
    for r in resync {
        if doc.contains(r) {
            sync_instance(doc, r);
            if let Some(c) = owning_component(doc, r) {
                comps.insert(c);
            }
        }
    }
    // 3. Instances of changed components follow; an instance inside another component makes that
    // one change too. Instances nested inside instances are rebuilt with their outer instance.
    let mut done: BTreeSet<NodeId> = BTreeSet::new();
    let mut frontier: Vec<NodeId> = comps.into_iter().collect();
    let mut rounds = 0;
    while !frontier.is_empty() && rounds < 64 {
        rounds += 1;
        let mut next = Vec::new();
        let mut synced: BTreeSet<NodeId> = BTreeSet::new();
        for c in frontier {
            if !done.insert(c) {
                continue;
            }
            for inst in instances_of(doc, c) {
                // A copy inside another instance is rebuilt with its outermost instance.
                let r = root_instance(doc, inst).unwrap_or(inst);
                if !synced.insert(r) {
                    continue;
                }
                sync_instance(doc, r);
                if let Some(outer) = owning_component(doc, r)
                    && !done.contains(&outer)
                {
                    next.push(outer);
                }
            }
        }
        frontier = next;
    }
}

/// Refuse edits that add, remove or reorder layers inside an instance (only its main can).
pub fn check_structure_edit(before: &Document, after: &Document) -> Result<(), String> {
    for &id in after.dirty() {
        let (Some(n), Some(b)) = (after.node(id), before.node(id)) else { continue };
        if n.children == b.children {
            continue;
        }
        if owning_instance(before, id).is_some() && owning_instance(after, id).is_some() {
            let name = owning_instance(after, id).and_then(|i| after.node(i)).map(|i| i.name.clone()).unwrap_or_default();
            return Err(format!(
                "layers can't be added, removed or reordered inside the instance \"{name}\": edit its main component, or detach the instance first"
            ));
        }
    }
    Ok(())
}

/// Re-derive `source` links and overrides of instance `inst` against its own main by comparing
/// values (after detaching an outer instance, or restoring a main): the look is kept exactly.
pub fn rebase_instance(doc: &mut Document, inst: NodeId) {
    let Some(NodeKind::Instance { component, .. }) = doc.node(inst).map(|n| n.kind.clone()) else { return };
    let mine = path_map(doc, inst, true);
    let theirs: BTreeMap<String, NodeId> =
        if doc.contains(component) { path_map(doc, component, false).into_iter().map(|(k, v)| (v, k)).collect() } else { BTreeMap::new() };
    let value_groups = |a: &Node, b: &Node| -> Vec<String> {
        diff_groups(a, b).into_iter().filter(|g| !matches!(*g, "size" | "position")).map(str::to_string).collect()
    };
    let mut updates = Vec::new();
    for (id, path) in &mine {
        let Some(n) = doc.node(*id) else { continue };
        // Follow the source chain into the main first (exact), then fall back to the name path.
        let mut main = None;
        let mut cur = n.source;
        for _ in 0..MAX_DEPTH {
            let Some(c) = cur else { break };
            if c != component && doc.contains(c) && doc.is_ancestor(component, c) {
                main = Some(c);
                break;
            }
            cur = doc.node(c).and_then(|x| x.source);
        }
        let main = main.or_else(|| theirs.get(path).copied());
        let mut overrides: BTreeSet<String> = n.overrides.iter().filter(|g| !matches!(g.as_str(), "component" | "props")).cloned().collect();
        if let Some(m) = main.and_then(|m| doc.node(m)) {
            overrides.extend(value_groups(m, n));
            if matches!((&m.kind, &n.kind), (NodeKind::Instance { component: a, .. }, NodeKind::Instance { component: b, .. }) if a != b) {
                overrides.insert("component".into());
            }
        }
        updates.push((*id, main, overrides));
    }
    for (id, main, overrides) in updates {
        if let Some(n) = doc.node_mut(id) {
            n.source = main;
            n.overrides = if main.is_some() { overrides } else { BTreeSet::new() };
        }
    }
    let root_groups = match (doc.node(component), doc.node(inst)) {
        (Some(m), Some(n)) => value_groups(m, n).into_iter().filter(|g| g != "name").collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if let Some(n) = doc.node_mut(inst) {
        n.source = Some(component);
        n.overrides.retain(|g| !matches!(g.as_str(), "component" | "props" | "position"));
        n.overrides.extend(root_groups);
    }
}

/// Parse a variant name "Size=Large, State=Hover" into pairs.
pub fn variant_props(name: &str) -> BTreeMap<String, String> {
    name.split(',')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Format variant pairs as a layer name, in the given property order (others after, sorted).
pub fn variant_name(pairs: &BTreeMap<String, String>, order: &[String]) -> String {
    let mut keys: Vec<&String> = order.iter().filter(|k| pairs.contains_key(*k)).collect();
    keys.extend(pairs.keys().filter(|k| !order.contains(k)));
    keys.iter().filter_map(|k| pairs.get(*k).map(|v| format!("{k}={v}"))).collect::<Vec<_>>().join(", ")
}

/// Variant properties of a set: property → values, in first-seen order.
pub fn variant_options(doc: &Document, set: NodeId) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for c in doc.node(set).map(|s| s.children.clone()).unwrap_or_default() {
        let Some(cn) = doc.node(c) else { continue };
        for kv in cn.name.split(',') {
            let Some((k, v)) = kv.split_once('=') else { continue };
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            match out.iter_mut().find(|(p, _)| *p == k) {
                Some((_, vals)) => {
                    if !vals.contains(&v) {
                        vals.push(v);
                    }
                }
                None => out.push((k, vec![v])),
            }
        }
    }
    out
}

/// The variant in `set` matching `wanted` best: all given properties equal, or else the most.
pub fn find_variant(doc: &Document, set: NodeId, wanted: &BTreeMap<String, String>) -> Option<NodeId> {
    let s = doc.node(set)?;
    let score = |c: &NodeId| doc.node(*c).map(|n| variant_props(&n.name)).map(|p| wanted.iter().filter(|(k, v)| p.get(*k) == Some(*v)).count());
    if let Some(exact) = s.children.iter().copied().find(|c| score(c) == Some(wanted.len())) {
        return Some(exact);
    }
    s.children.iter().copied().filter(|c| score(c).is_some()).max_by_key(|c| score(c).unwrap_or(0))
}

/// Check that every instance mirrors its main's layer structure (tests and diagnostics): each
/// layer's `source` is the main layer at the same place, swapped nested instances mirror their
/// own main. Returns the problems found.
pub fn mirror_issues(doc: &Document) -> Vec<String> {
    fn walk(doc: &Document, inst: &Node, main: &Node, depth: usize, out: &mut Vec<String>) {
        if depth > MAX_DEPTH {
            return;
        }
        if inst.children.len() != main.children.len() {
            out.push(format!("{} has {} layers, its main {} has {}", inst.id, inst.children.len(), main.id, main.children.len()));
            return;
        }
        for (a, b) in inst.children.iter().zip(&main.children) {
            let (Some(an), Some(bn)) = (doc.node(*a), doc.node(*b)) else { continue };
            if an.source != Some(bn.id) {
                out.push(format!("{} mirrors {:?}, expected {}", an.id, an.source, bn.id));
                continue;
            }
            match (&an.kind, &bn.kind) {
                (NodeKind::Instance { component: x, .. }, NodeKind::Instance { component: y, .. }) if x != y => {
                    if let Some(m) = doc.node(*x) {
                        walk(doc, an, m, depth + 1, out);
                    }
                }
                _ => walk(doc, an, bn, depth + 1, out),
            }
        }
    }
    let mut out = Vec::new();
    for n in doc.nodes() {
        let NodeKind::Instance { component, .. } = &n.kind else { continue };
        if inside_instance(doc, n.id) {
            continue;
        }
        if let Some(m) = doc.node(*component) {
            walk(doc, n, m, 0, &mut out);
        }
    }
    out
}

#[cfg(test)]
#[path = "components_tests.rs"]
mod tests;
