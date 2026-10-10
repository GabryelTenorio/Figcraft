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
//! `characters`, `textStyle`, `size`, `position`, `component` (a swapped nested instance),
//! `props` (property values of a nested instance) and `slot` (a slot holding the instance's own
//! content).
//!
//! Instances never gain or lose layers of their own: commands that would add, remove or reorder
//! layers inside an instance are refused ([`check_structure_edit`]), except inside **slots**. A
//! slot is a frame of a main component bound to a slot property (`prop_refs["slot"]`). An
//! instance's copy of it shows the main's default content until its layers are edited; from then
//! on (override group `slot`) its children are the instance's own content: free to edit, kept
//! across syncs and variant switches, and outside the instance's structure (instances placed in it
//! stand alone).

use std::collections::{BTreeMap, BTreeSet};

use figcraft_doc::*;
use figcraft_geom::Affine;
use serde_json::Value;

/// Nesting limit for instances inside instances.
const MAX_DEPTH: usize = 16;
/// The override group (and `prop_refs` field) of slots.
pub const SLOT: &str = "slot";
/// Every override group a layer can carry.
pub const OVERRIDE_GROUPS: &[&str] = &[
    "fills",
    "strokes",
    "effects",
    "opacity",
    "visible",
    "name",
    "corners",
    "characters",
    "textStyle",
    "size",
    "position",
    "component",
    "props",
    SLOT,
];

fn is_instance(doc: &Document, id: NodeId) -> bool {
    doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Instance { .. }))
}

/// A slot (inside an instance) whose children are the instance's own content.
fn holds_content(n: &Node) -> bool {
    n.overrides.contains(SLOT)
}

/// `id` and its ancestors, up to the nearest slot above it that holds an instance's own content:
/// content belongs to no instance (it edits freely, and instances placed in it stand alone).
fn scope(doc: &Document, id: NodeId) -> Vec<NodeId> {
    let mut out = vec![id];
    for a in doc.ancestors(id) {
        if doc.node(a).is_some_and(holds_content) {
            break;
        }
        out.push(a);
    }
    out
}

/// The component a node belongs to (itself, or the nearest component ancestor).
pub fn owning_component(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    chain.into_iter().find(|c| doc.node(*c).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. })))
}

/// The innermost instance containing `id` (`id` itself when it is an instance).
pub fn owning_instance(doc: &Document, id: NodeId) -> Option<NodeId> {
    scope(doc, id).into_iter().find(|c| is_instance(doc, *c))
}

/// The outermost instance containing `id` (`id` itself when it is a top-level instance).
pub fn root_instance(doc: &Document, id: NodeId) -> Option<NodeId> {
    scope(doc, id).into_iter().rfind(|c| is_instance(doc, *c))
}

/// The innermost instance `id` is a layer of (never `id` itself).
fn enclosing_instance(doc: &Document, id: NodeId) -> Option<NodeId> {
    scope(doc, id).into_iter().skip(1).find(|c| is_instance(doc, *c))
}

/// Whether `id` is a layer inside an instance (not the outermost instance itself).
pub fn inside_instance(doc: &Document, id: NodeId) -> bool {
    enclosing_instance(doc, id).is_some()
}

/// The slot property frame `id` stands for (its name and preferred components): a frame bound to
/// a slot property of its component, or an instance's copy of one (followed through its sources).
pub fn slot_def(doc: &Document, id: NodeId) -> Option<(&str, &[NodeId])> {
    let mut cur = id;
    for _ in 0..=MAX_DEPTH {
        let n = doc.node(cur)?;
        if !matches!(n.kind, NodeKind::Frame) {
            return None;
        }
        if let Some(name) = n.prop_refs.get(SLOT) {
            let NodeKind::Component { props, .. } = &doc.node(owning_component(doc, cur)?)?.kind else { return None };
            return match props.get(name) {
                Some(PropDef::Slot { preferred }) => Some((name.as_str(), preferred.as_slice())),
                _ => None,
            };
        }
        cur = n.source?;
    }
    None
}

/// The name of the slot property frame `id` stands for ([`slot_def`]).
pub fn slot_name(doc: &Document, id: NodeId) -> Option<String> {
    slot_def(doc, id).map(|(name, _)| name.to_string())
}

/// Whether `id` is a slot (in a main component, or an instance's copy of one).
pub fn is_slot(doc: &Document, id: NodeId) -> bool {
    slot_def(doc, id).is_some()
}

/// Whether slot `id` (inside an instance) holds the instance's own content rather than its main's
/// default content.
pub fn slot_edited(doc: &Document, id: NodeId) -> bool {
    doc.node(id).is_some_and(holds_content)
}

/// The slot `id` is, or lies in, within its instance: where layers may be added, removed and
/// reordered freely.
pub fn slot_scope(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    for c in chain {
        if is_instance(doc, c) {
            return None;
        }
        if is_slot(doc, c) {
            return Some(c);
        }
    }
    None
}

/// Whether layers can't be added to, removed from or reordered in `id`: it is (inside) an
/// instance, outside its slots.
pub fn structure_locked(doc: &Document, id: NodeId) -> bool {
    owning_instance(doc, id).is_some() && slot_scope(doc, id).is_none()
}

/// For tree walks that look for where layers can go: whether the children of `id` are locked (in
/// an instance, outside its slots), given whether `id` itself is (`locked`).
pub fn locks_children(doc: &Document, id: NodeId, locked: bool) -> bool {
    is_instance(doc, id) || (locked && !is_slot(doc, id))
}

/// Whether layer `id`'s place (parent and position among its siblings) changed between two
/// versions only where instance structure allows it.
pub fn move_allowed(before: &Document, after: &Document, id: NodeId) -> bool {
    let place = |d: &Document| d.node(id).and_then(|n| n.parent).map(|p| (p, d.index_in_parent(id)));
    let (was, is) = (place(before), place(after));
    was == is || (!was.is_some_and(|(p, _)| structure_locked(before, p)) && !is.is_some_and(|(p, _)| structure_locked(after, p)))
}

/// The slots of a component or instance (its own: not those of instances nested in it, nor of the
/// content put into its slots), with their property names, top-most first.
pub fn slots_of(doc: &Document, root: NodeId) -> Vec<(NodeId, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = doc.node(root).map(|n| n.children.iter().rev().copied().collect()).unwrap_or_default();
    let mut seen = 0usize;
    while let Some(c) = stack.pop() {
        seen += 1;
        if seen > 1_000_000 {
            break;
        }
        let Some(n) = doc.node(c) else { continue };
        if let Some(name) = slot_name(doc, c) {
            out.push((c, name));
        }
        if !is_instance(doc, c) && !holds_content(n) {
            stack.extend(n.children.iter().rev().copied());
        }
    }
    out
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

/// Default property values of a component (slots hold content, not values: they have none).
pub fn default_props(comp: &Node) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    if let NodeKind::Component { props, .. } = &comp.kind {
        for (k, d) in props {
            if takes_value(d) {
                out.insert(k.clone(), prop_default_value(d));
            }
        }
    }
    out
}

/// Whether instances store a value for the property (every kind but slots).
pub fn takes_value(d: &PropDef) -> bool {
    !matches!(d, PropDef::Slot { .. })
}

/// A property definition's default as a JSON value.
pub fn prop_default_value(d: &PropDef) -> Value {
    match d {
        PropDef::Boolean { default } => Value::Bool(*default),
        PropDef::Text { default } => Value::String(default.clone()),
        PropDef::InstanceSwap { default } => Value::from(default.0),
        PropDef::Variant { default, .. } => Value::String(default.clone()),
        PropDef::Slot { .. } => Value::Null,
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
    root.annotations.clear();
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
/// `src_names` (an instance's layers), layers are named after the main layer they mirror (renames
/// don't break matching), and the content of slots holding the instance's own is left out.
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
            if !(src_names && holds_content(cn)) {
                stack.push((*c, path));
            }
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

/// Rebuild instance `inst` from its main component, keeping ids, overrides and the content of its
/// slots. An instance whose main component was deleted keeps its layers as they are.
pub fn sync_instance(doc: &mut Document, inst: NodeId) {
    let kept = slot_content(doc, inst);
    sync_instance_depth(doc, inst, 0);
    restore_slot_content(doc, kept);
}

/// The slots in instance `inst` (nested instances included) holding the instance's own content:
/// each with its content (the subtrees of its children, pre-order).
fn slot_content(doc: &Document, inst: NodeId) -> Vec<(NodeId, Vec<Node>)> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = doc.node(inst).map(|n| n.children.clone()).unwrap_or_default();
    while let Some(c) = stack.pop() {
        let Some(n) = doc.node(c) else { continue };
        if holds_content(n) {
            let nodes = n.children.iter().flat_map(|k| doc.subtree(*k)).filter_map(|d| doc.node(d).cloned()).collect();
            out.push((c, nodes));
        } else {
            stack.extend(n.children.iter().copied());
        }
    }
    out
}

/// Put slot content back after a rebuild, into the slots that still hold the instance's own
/// content (a slot the main no longer has, or no longer marks as a slot, loses it). Constraints
/// and geometry overrides ran before, so the content keeps its place.
fn restore_slot_content(doc: &mut Document, kept: Vec<(NodeId, Vec<Node>)>) {
    for (slot, nodes) in kept {
        let keep = slot_edited(doc, slot) && is_slot(doc, slot) && nodes.iter().all(|n| !doc.contains(n.id));
        if !keep {
            continue;
        }
        let ids: Vec<NodeId> = nodes.iter().map(|n| n.id).collect();
        for n in nodes {
            // Pre-order: every parent is back before its children.
            let _ = insert_linked(doc, n);
        }
        // Never leave links to a layer that couldn't come back.
        for id in ids.into_iter().chain([slot]) {
            let dangling = doc.node(id).is_some_and(|n| n.children.iter().any(|c| !doc.contains(*c)));
            if dangling {
                let live: Vec<NodeId> = doc.node(id).map(|n| n.children.iter().copied().filter(|c| doc.contains(*c)).collect()).unwrap_or_default();
                if let Some(n) = doc.node_mut(id) {
                    n.children = live;
                }
            }
        }
    }
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
    // Old layers, by (parent, source), by source alone (when only one layer mirrors it) and by
    // name path.
    let old_paths = path_map(doc, inst, true);
    let mut by_source: BTreeMap<(NodeId, NodeId), Node> = BTreeMap::new();
    let mut by_src: BTreeMap<NodeId, Option<Node>> = BTreeMap::new();
    let mut by_path: BTreeMap<String, Node> = BTreeMap::new();
    for (i, path) in &old_paths {
        let Some(n) = doc.node(*i) else { continue };
        if let Some(s) = n.source {
            if let Some(p) = n.parent {
                by_source.entry((p, s)).or_insert_with(|| n.clone());
            }
            by_src.entry(s).and_modify(|e| *e = None).or_insert_with(|| Some(n.clone()));
        }
        by_path.insert(path.clone(), n.clone());
    }
    for c in root.children.clone() {
        let _ = doc.remove(c);
    }
    // Ids for the copies: reuse the matching old layer's id (a layer the main moved to another
    // parent keeps its id and overrides).
    let src: Vec<NodeId> = doc.subtree(component).into_iter().filter(|i| *i != component).collect();
    let main_paths = path_map(doc, component, false);
    let mut map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    map.insert(component, inst);
    let mut olds: BTreeMap<NodeId, Node> = BTreeMap::new();
    let mut used: BTreeSet<NodeId> = BTreeSet::new();
    // Slots holding the instance's own content (new ids), and main layers not copied: the
    // default content of those slots.
    let mut edited: BTreeSet<NodeId> = BTreeSet::new();
    let mut skipped: BTreeSet<NodeId> = BTreeSet::new();
    for s in &src {
        let Some(sn) = doc.node(*s) else { continue };
        if sn.parent.is_some_and(|p| skipped.contains(&p) || map.get(&p).is_some_and(|pn| edited.contains(pn))) {
            skipped.insert(*s);
            continue;
        }
        let parent_new = sn.parent.and_then(|p| map.get(&p).copied());
        let old = parent_new
            .and_then(|pn| by_source.get(&(pn, *s)))
            .or_else(|| by_src.get(s).and_then(Option::as_ref))
            .or_else(|| main_paths.get(s).and_then(|p| by_path.get(p)));
        let nid = match old {
            Some(o) if !doc.contains(o.id) && used.insert(o.id) => {
                olds.insert(o.id, o.clone());
                o.id
            }
            _ => doc.alloc_id(),
        };
        if olds.get(&nid).is_some_and(holds_content) && is_slot(doc, *s) {
            edited.insert(nid);
        }
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
        // Annotations belong to the layer they were added to, not to its copies.
        n.annotations = olds.get(&nid).map(|o| o.annotations.clone()).unwrap_or_default();
        if let Some(old) = olds.get(&nid) {
            carry_overrides(old, &mut n, Carry::Values);
        }
        if !edited.contains(&nid) {
            // No longer a slot in the main: it mirrors the main again.
            n.overrides.remove(SLOT);
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
    fresh.annotations = root.annotations.clone();
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
    // 0. A slot whose layers were added, removed or reordered holds the instance's own content
    //    from now on; copies of instance layers placed elsewhere stop mirroring their main.
    for &id in changed {
        let (Some(n), Some(b)) = (doc.node(id), before.node(id)) else { continue };
        if n.children == b.children || owning_instance(doc, id).is_none() {
            continue;
        }
        if let Some(slot) = slot_scope(doc, id)
            && !slot_edited(doc, slot)
        {
            own_slot_content(doc, slot);
        }
    }
    for &id in changed {
        if !before.contains(id) {
            relink_copy(doc, id);
        }
    }
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

/// Slot `slot` (inside an instance) takes the instance's own content: the layers it shows (the
/// main's default content until now) stop mirroring the main, and instances among them stand
/// alone, linked to their own mains.
fn own_slot_content(doc: &mut Document, slot: NodeId) {
    if let Some(n) = doc.node_mut(slot) {
        n.overrides.insert(SLOT.into());
    }
    for nested in unlink_layers(doc, slot, None) {
        rebase_instance(doc, nested);
    }
}

/// Unlink the layers below `top` that are layers of `owner` (an instance, or `None` for the
/// content of a slot): plain layers forget the main layer they mirrored and their property
/// bindings. The instances among them are returned, to re-link to their own mains (with
/// [`rebase_instance`]) once the caller is done.
pub fn unlink_layers(doc: &mut Document, top: NodeId, owner: Option<NodeId>) -> Vec<NodeId> {
    let picked: Vec<(NodeId, bool)> =
        doc.subtree(top).into_iter().filter(|d| *d != top && enclosing_instance(doc, *d) == owner).map(|d| (d, is_instance(doc, d))).collect();
    let mut nested = Vec::new();
    for (d, instance) in picked {
        let Some(n) = doc.node_mut(d) else { continue };
        n.prop_refs.clear();
        if instance {
            nested.push(d);
        } else {
            n.source = None;
            n.overrides.clear();
        }
    }
    nested
}

/// A new layer copied from inside an instance (pasted, duplicated) to where no instance owns it
/// stops mirroring the main; a copied nested instance is re-linked to its own main (its look kept).
fn relink_copy(doc: &mut Document, id: NodeId) {
    let Some(n) = doc.node(id) else { return };
    match &n.kind {
        NodeKind::Instance { component, .. } => {
            let main_ok = doc.node(*component).is_some_and(|m| matches!(m.kind, NodeKind::Component { .. }));
            if main_ok && !inside_instance(doc, id) && (n.source != Some(*component) || !instance_issues(doc, id).is_empty()) {
                rebase_instance(doc, id);
            }
        }
        _ => {
            if (n.source.is_some() || !n.overrides.is_empty())
                && owning_instance(doc, id).is_none()
                && let Some(m) = doc.node_mut(id)
            {
                m.source = None;
                m.overrides.clear();
            }
        }
    }
}

/// Refuse edits that add, remove or reorder layers inside an instance (only its main can), outside
/// its slots.
pub fn check_structure_edit(before: &Document, after: &Document) -> Result<(), String> {
    for &id in after.dirty() {
        let (Some(n), Some(b)) = (after.node(id), before.node(id)) else { continue };
        // Library copies change only through library updates (and are read-only otherwise).
        if n.children == b.children || after.is_remote(id) {
            continue;
        }
        if structure_locked(before, id) && structure_locked(after, id) {
            let name = owning_instance(after, id).and_then(|i| after.node(i)).map(|i| i.name.clone()).unwrap_or_default();
            return Err(format!(
                "layers can't be added, removed or reordered inside the instance \"{name}\": edit its main component, or detach the instance first"
            ));
        }
    }
    Ok(())
}

/// Re-derive `source` links and overrides of instance `inst` against its own main by comparing
/// values (after detaching an outer instance, restoring a main, or copying a nested instance out):
/// the look is kept exactly. Values a component property sets aren't overrides.
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
            let bound: Vec<&str> = m.prop_refs.keys().map(|f| field_group(f)).collect();
            overrides.extend(value_groups(m, n).into_iter().filter(|g| !bound.contains(&g.as_str())));
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
/// own main, slots holding the instance's own content aren't compared. Returns the problems found.
pub fn mirror_issues(doc: &Document) -> Vec<String> {
    doc.nodes()
        .filter(|n| matches!(n.kind, NodeKind::Instance { .. }) && !inside_instance(doc, n.id))
        .flat_map(|n| instance_issues(doc, n.id))
        .collect()
}

/// [`mirror_issues`] for one instance.
fn instance_issues(doc: &Document, id: NodeId) -> Vec<String> {
    fn walk(doc: &Document, inst: &Node, main: &Node, depth: usize, out: &mut Vec<String>) {
        if depth > MAX_DEPTH || holds_content(inst) {
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
    if let Some(n) = doc.node(id)
        && let NodeKind::Instance { component, .. } = &n.kind
        && let Some(m) = doc.node(*component)
    {
        walk(doc, n, m, 0, &mut out);
    }
    out
}

#[cfg(test)]
#[path = "components_tests.rs"]
pub(crate) mod tests;

/// The nested instance copies inside instance `id` whose main-component counterpart is exposed
/// (top-most first; an exposed instance's own exposed nested instances follow it).
pub fn exposed_copies(doc: &Document, id: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = doc.node(id).map(|n| n.children.iter().rev().copied().collect()).unwrap_or_default();
    let mut seen = 0usize;
    while let Some(c) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            break;
        }
        let Some(n) = doc.node(c) else { continue };
        let exposed = matches!(n.kind, NodeKind::Instance { .. }) && n.source.and_then(|src| doc.node(src)).is_some_and(|src| src.exposed);
        if exposed {
            out.push(c);
        }
        // Only exposed nested instances pass their own exposed instances up.
        if !matches!(n.kind, NodeKind::Instance { .. }) || exposed {
            stack.extend(n.children.iter().rev().copied());
        }
    }
    out
}
