//! Three-way merge, for branches of a file: what a branch and its main file each changed since
//! the branch was made (two [`diff`]s from their common base), where those changes conflict, and
//! one side's changes merged into the other side.
//!
//! A conflict is the same property (or document-level entry) changed on both sides to different
//! values, or a layer deleted on one side while the other side edited it or something inside it
//! (including adding layers to it or moving layers into or out of it). Nothing that conflicts is
//! merged until it is resolved: each conflict is resolved for one side, and the other side's
//! changes to it are left out. Everything else from both sides is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use figcraft_doc::{Document, NodeId};
use serde::{Deserialize, Serialize};

use crate::change::{Change, DocProp, Entry, Prop};
use crate::replica::{Mode, Positions, Replica, create_change};
use crate::{Error, diff};

/// One side of a merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Branch,
    Main,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Branch => Side::Main,
            Side::Main => Side::Branch,
        }
    }

    /// `"branch"` or `"main"`.
    pub fn name(self) -> &'static str {
        match self {
            Side::Branch => "branch",
            Side::Main => "main",
        }
    }

    pub fn parse(s: &str) -> Option<Side> {
        match s {
            "branch" => Some(Side::Branch),
            "main" => Some(Side::Main),
            _ => None,
        }
    }
}

/// What a conflict is about.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// A layer property. A layer's parent and position are one property (where it is).
    Node(NodeId, Prop),
    /// A layer deleted on one side and edited (it or what's inside it) on the other.
    Layer(NodeId),
    /// A document-level property.
    Doc(DocProp),
    /// A document-level entry (a style, variable, comment thread…).
    Entry(Entry),
}

/// The serde name of `v` (`fills`, `styleOrder`…).
fn camel<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

/// `12/fills`, `12/layer`, `doc/styleOrder`, `style/S:3`, `comment/7`… (the keys resolutions use).
impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Node(id, prop) => write!(f, "{id}/{}", camel(prop)),
            Key::Layer(id) => write!(f, "{id}/layer"),
            Key::Doc(prop) => write!(f, "doc/{}", camel(prop)),
            Key::Entry(e) => match e {
                Entry::Style(k) => write!(f, "style/{k}"),
                Entry::Comment(k) => write!(f, "comment/{k}"),
                Entry::Collection(k) => write!(f, "collection/{k}"),
                Entry::Variable(k) => write!(f, "variable/{k}"),
                Entry::DevSnapshot(k) => write!(f, "devSnapshot/{k}"),
                Entry::Image(k) => write!(f, "image/{k}"),
            },
        }
    }
}

/// Changes that conflict, and which side's to keep.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub key: Key,
    /// Layer conflicts: the side that deleted the layer.
    pub deleted_by: Option<Side>,
    /// The branch's changes to it (indexes into [`Plan::branch`]).
    pub branch: Vec<usize>,
    /// The main file's changes to it (indexes into [`Plan::main`]).
    pub main: Vec<usize>,
}

impl Conflict {
    /// `side`'s changes to it.
    pub fn changes(&self, side: Side) -> &[usize] {
        match side {
            Side::Branch => &self.branch,
            Side::Main => &self.main,
        }
    }
}

/// What each side changed since the base, and where they conflict.
#[derive(Clone, Debug)]
pub struct Plan {
    base: Replica,
    /// The branch's changes since the base.
    pub branch: Vec<Change>,
    /// The main file's changes since the base.
    pub main: Vec<Change>,
    pub conflicts: Vec<Conflict>,
    branch_positions: Positions,
    main_positions: Positions,
}

/// Document-level properties that belong to each copy, not to the work: the file name (a branch
/// is named after itself) and the page it opens on.
fn merges(c: &Change) -> bool {
    !matches!(c, Change::SetDoc { prop: DocProp::Name | DocProp::CurrentPage, .. })
}

/// What a change writes, for finding the same write on both sides (creations: nothing; ids are
/// never made twice, see [`Document::set_client_id`]).
fn key_of(c: &Change) -> Option<Key> {
    Some(match c {
        Change::Set { id, prop: Prop::Position, .. } => Key::Node(*id, Prop::Parent),
        Change::Set { id, prop, .. } => Key::Node(*id, *prop),
        Change::Delete { id } => Key::Layer(*id),
        Change::SetDoc { prop, .. } => Key::Doc(*prop),
        Change::Put { entry, .. } | Change::Remove { entry } => Key::Entry(entry.clone()),
        Change::Create { .. } => return None,
    })
}

fn by_key(changes: &[Change]) -> BTreeMap<Key, Vec<usize>> {
    let mut out: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (i, c) in changes.iter().enumerate() {
        if let Some(k) = key_of(c) {
            out.entry(k).or_default().push(i);
        }
    }
    out
}

/// The changes (indexes) that edit what's in `gone`: write one of its layers, create a layer in it
/// (or in a layer created in it) or move a layer into it. Changes come in [`diff`] order, so
/// creations come parents first and before moves.
fn touching(gone: &BTreeSet<NodeId>, changes: &[Change]) -> Vec<usize> {
    let mut doomed = gone.clone();
    let mut out = Vec::new();
    for (i, c) in changes.iter().enumerate() {
        let hit = match c {
            Change::Set { id, prop: Prop::Parent, value } => {
                doomed.contains(id) || value.read::<Option<NodeId>>("parent").ok().flatten().is_some_and(|p| doomed.contains(&p))
            }
            Change::Set { id, .. } => doomed.contains(id),
            Change::Create { id, parent, .. } => {
                let hit = parent.is_some_and(|p| doomed.contains(&p));
                if hit {
                    doomed.insert(*id);
                }
                hit
            }
            _ => false,
        };
        if hit {
            out.push(i);
        }
    }
    out
}

/// The changes on `branch` and on `main` since `base` (the main file when the branch was made or
/// last updated), and their conflicts.
pub fn plan(base: &Document, branch: &Document, main: &Document) -> Result<Plan, Error> {
    let base_r = Replica::new(base.clone());
    let b = diff(&base_r, branch)?;
    let m = diff(&base_r, main)?;
    let bc: Vec<Change> = b.changes.into_iter().filter(merges).collect();
    let mc: Vec<Change> = m.changes.into_iter().filter(merges).collect();
    let mut conflicts = Vec::new();
    let km = by_key(&mc);
    for (k, bi) in by_key(&bc) {
        // Deletions conflict with edits (below); images are content-addressed, so never differ.
        if matches!(k, Key::Layer(_) | Key::Entry(Entry::Image(_))) {
            continue;
        }
        // shortcut: a layer inside an instance that follows its main conflicts along with the main's
        // layer (either side is fine: the engine re-derives it after merging); leave such layers
        // out once the plan can tell followed values from overrides.
        let Some(mi) = km.get(&k) else { continue };
        let same = bi.len() == mi.len() && bi.iter().zip(mi).all(|(x, y)| bc.get(*x) == mc.get(*y));
        if !same {
            conflicts.push(Conflict { key: k, deleted_by: None, branch: bi, main: mi.clone() });
        }
    }
    for (side, changes, doc, other) in [(Side::Branch, &bc, branch, &mc), (Side::Main, &mc, main, &bc)] {
        for (i, c) in changes.iter().enumerate() {
            let Change::Delete { id } = c else { continue };
            // Layers moved out before the deletion survive it.
            let gone: BTreeSet<NodeId> = base.subtree(*id).into_iter().filter(|n| !doc.contains(*n)).collect();
            let edits = touching(&gone, other);
            if edits.is_empty() {
                continue;
            }
            let (branch, main) = if side == Side::Branch { (vec![i], edits) } else { (edits, vec![i]) };
            conflicts.push(Conflict { key: Key::Layer(*id), deleted_by: Some(side), branch, main });
        }
    }
    Ok(Plan {
        base: base_r,
        branch: bc,
        main: mc,
        conflicts,
        branch_positions: b.replica.positions().clone(),
        main_positions: m.replica.positions().clone(),
    })
}

impl Plan {
    /// The base, as a replica.
    pub fn base(&self) -> &Replica {
        &self.base
    }

    /// `side`'s changes.
    pub fn changes(&self, side: Side) -> &[Change] {
        match side {
            Side::Branch => &self.branch,
            Side::Main => &self.main,
        }
    }

    fn positions(&self, side: Side) -> &Positions {
        match side {
            Side::Branch => &self.branch_positions,
            Side::Main => &self.main_positions,
        }
    }

    /// The conflicts `pick` has no side for.
    pub fn unresolved(&self, pick: &dyn Fn(&Key) -> Option<Side>) -> Vec<&Conflict> {
        self.conflicts.iter().filter(|c| pick(&c.key).is_none()).collect()
    }

    /// Merge `from`'s changes into `into` (the other side's document, as the plan was made from),
    /// each conflict resolved by `pick`. Fails when a conflict has no side, or when the result is
    /// not a valid document. Returns the merged document and the layers it touched.
    pub fn merge(&self, into: &Document, from: Side, pick: &dyn Fn(&Key) -> Option<Side>) -> Result<(Document, BTreeSet<NodeId>), Error> {
        let unresolved: Vec<String> = self.unresolved(pick).iter().map(|c| c.key.to_string()).collect();
        if !unresolved.is_empty() {
            return Err(Error::Invalid(format!("unresolved conflicts: {}", unresolved.join(", "))));
        }
        let mut skip: BTreeSet<usize> = BTreeSet::new();
        let mut batch = Vec::new();
        for c in &self.conflicts {
            if pick(&c.key) != Some(from) {
                skip.extend(c.changes(from));
            } else if let (Key::Layer(id), Some(by)) = (&c.key, c.deleted_by)
                && by != from
            {
                // `into` deleted the layer and `from`'s edits win: bring back what it deleted (as
                // it was in the base), then `from`'s edits apply to it.
                for n in self.base.document().subtree(*id) {
                    if let Some(node) = self.base.document().node(n).filter(|_| !into.contains(n)) {
                        batch.push(create_change(node, self.base.key(n))?);
                    }
                }
            }
        }
        // shortcut: images a side stopped using are kept (unused ones cost file size only); drop
        // them if files grow.
        let kept = |c: &Change| !matches!(c, Change::Remove { entry: Entry::Image(_) });
        batch.extend(self.changes(from).iter().enumerate().filter(|(i, c)| !skip.contains(i) && kept(c)).map(|(_, c)| c.clone()));
        let mut r = Replica::from_parts(into.clone(), self.positions(from.other()).clone());
        let applied = r.apply(&batch, Mode::Strict)?;
        Ok((r.document().clone(), applied.touched))
    }

    /// `into` (the other side's document) with only `from`'s changes at `indexes` applied, those
    /// that apply (for showing what one side's changes to a conflict look like).
    pub fn with_changes(&self, into: &Document, from: Side, indexes: &[usize]) -> Document {
        let changes: Vec<Change> = indexes.iter().filter_map(|i| self.changes(from).get(*i).cloned()).collect();
        let mut r = Replica::from_parts(into.clone(), self.positions(from.other()).clone());
        let _ = r.apply(&changes, Mode::Lenient);
        r.document().clone()
    }
}
