//! Sections and handoff: hide a section's contents, resolve a change to a ready-for-dev layer.
//! (Wrapping layers in a new section is `object.wrapInSection`, with the other wrappers.)

use figcraft_doc::{DevStatus, NodeId, NodeKind};
use serde_json::{Value, json};

use super::CommandSpec;
use crate::params::*;
use crate::{Result, Session, bad, spec};

/// Longest change reason kept.
const MAX_NOTE: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "section.toggleContents",
            "Hide/Show section contents",
            "{ids?, hidden? (default: hide unless all are hidden)} (a hidden section shows only its name and outline; its layers aren't drawn or hit)",
            true,
            toggle_contents
        ),
        spec!(
            "dev.resolveChange",
            "Resolve change",
            "{ids?, reason?} (ready-for-dev frames and sections that changed since they were marked go back to Ready for dev; the reason is kept)",
            true,
            resolve_change
        ),
    ]
}

fn toggle_contents(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "section.toggleContents";
    let ids = req_targets(cmd, s, p)?;
    let doc = s.doc()?;
    let sections: Vec<NodeId> = ids.iter().copied().filter(|i| doc.node(*i).is_some_and(|n| matches!(n.kind, NodeKind::Section { .. }))).collect();
    if sections.is_empty() {
        return Err(bad(cmd, "select a section"));
    }
    let hide = boolean(p, "hidden").unwrap_or_else(|| !sections.iter().all(|i| doc.node(*i).is_some_and(|n| n.hides_contents())));
    // Selected layers inside a section being hidden give way to the section.
    let sel: Vec<NodeId> = s.selection();
    let mut new_sel: Vec<NodeId> = Vec::new();
    for i in sel {
        let inside = if hide { sections.iter().copied().find(|sec| doc.is_ancestor(*sec, i)) } else { None };
        let keep = inside.unwrap_or(i);
        if !new_sel.contains(&keep) {
            new_sel.push(keep);
        }
    }
    let doc = s.doc_mut()?;
    for sec in &sections {
        if let NodeKind::Section { contents_hidden } = &mut doc.get_mut(*sec)?.kind {
            *contents_hidden = hide;
        }
    }
    s.set_selection(new_sel);
    Ok(json!({"hidden": hide, "ids": sections}))
}

fn resolve_change(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "dev.resolveChange";
    let ids = req_targets(cmd, s, p)?;
    let reason: String = string(p, "reason").unwrap_or_default().trim().chars().take(MAX_NOTE).collect();
    let doc = s.doc_mut()?;
    let mut out = Vec::new();
    for i in ids {
        let n = doc.get_mut(i)?;
        if n.dev_status == DevStatus::ReadyForDev && n.dev_changed {
            n.dev_changed = false;
            n.dev_note = reason.clone();
            out.push(i);
        }
    }
    if out.is_empty() {
        return Err(bad(cmd, "nothing selected has changed since it was marked ready for dev"));
    }
    Ok(json!({"ids": out}))
}
