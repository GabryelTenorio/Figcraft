//! A branch's history says where it came from: it starts with an entry for its creation and gets
//! one each time it is updated from main, on its tab or closed; the entries are versions like any
//! other (viewed, restored, saved with the file) and stay out of the main file's history.

use figcraft_doc::{NodeId, NodeKind};
use serde_json::{Value, json};

use crate::Session;
use crate::branches::{CREATED, UPDATED};
use crate::collab_tests::base;

fn ok(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn names(s: &mut Session) -> Vec<String> {
    ok(s, "version.list", json!({})).as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap().to_string()).collect()
}

fn layer_name(s: &Session, id: NodeId) -> String {
    s.doc().unwrap().node(id).unwrap().name.clone()
}

/// The base file with a branch "Explore" shown: (session, branch id, a rectangle).
fn branched() -> (Session, String, NodeId) {
    let mut s = Session::with_document(base());
    let r = s.doc().unwrap().nodes().find(|n| matches!(n.kind, NodeKind::Rectangle)).unwrap().id;
    let id = ok(&mut s, "branch.create", json!({"name": "Explore"}))["id"].as_str().unwrap().to_string();
    (s, id, r)
}

#[test]
fn a_branch_history_starts_with_its_creation_and_notes_each_update_from_main() {
    let (mut s, id, r) = branched();
    let original = layer_name(&s, r);
    assert_eq!(names(&mut s), [CREATED]);
    let created = ok(&mut s, "version.list", json!({}))[0].clone();
    assert!(
        created.get("auto").is_none() && created["time"].as_u64().is_some_and(|t| t > 0),
        "kept like a named version, never pruned as an autosave"
    );
    // Main moves on; the branch takes it: an entry of its own, the branch as it was right after.
    ok(&mut s, "node.set", json!({"id": r, "props": {"name": "On the branch"}}));
    ok(&mut s, "doc.switch", json!({"index": 0}));
    assert!(names(&mut s).is_empty(), "the main file's history has none of it");
    ok(&mut s, "shape.rectangle", json!({"x": 900, "y": 900, "width": 10, "height": 10, "props": {"name": "From main"}}));
    ok(&mut s, "doc.switch", json!({"index": 1}));
    assert_eq!(ok(&mut s, "branch.updateFromMain", json!({}))["updated"], true);
    assert_eq!(names(&mut s), [CREATED, UPDATED]);
    // Again, after a version saved by hand: in order.
    ok(&mut s, "file.saveVersion", json!({"name": "Mine"}));
    ok(&mut s, "doc.switch", json!({"index": 0}));
    ok(&mut s, "shape.ellipse", json!({"x": 950, "y": 900, "width": 10, "height": 10}));
    ok(&mut s, "doc.switch", json!({"index": 1}));
    ok(&mut s, "branch.updateFromMain", json!({}));
    assert_eq!(names(&mut s), [CREATED, UPDATED, "Mine", UPDATED]);
    // They are versions like any other: the creation shows the branch as it began, and restoring
    // it is one undo step.
    let first = created["id"].clone();
    ok(&mut s, "version.restore", json!({"id": first}));
    assert_eq!(layer_name(&s, r), original);
    assert!(!s.doc().unwrap().nodes().any(|n| n.name == "From main"));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(layer_name(&s, r), "On the branch");
    assert!(s.doc().unwrap().nodes().any(|n| n.name == "From main"));
    // The update's entry is the branch with main's change in it.
    let update = ok(&mut s, "version.list", json!({}))[1]["id"].clone();
    ok(&mut s, "version.preview", json!({"id": update}));
    assert!(s.doc().unwrap().nodes().any(|n| n.name == "From main") && layer_name(&s, r) == "On the branch");
    ok(&mut s, "version.exitPreview", json!({}));
    let _ = id;
}

#[test]
fn a_closed_branch_gets_its_entry_too_and_the_history_saves_with_the_file() {
    let dir = std::env::temp_dir().join(format!("figcraft-branch-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("file.figcraft").to_string_lossy().to_string();
    let (mut s, id, _) = branched();
    // Its tab closes; main changes; the branch is updated from the main file's tab.
    ok(&mut s, "file.close", json!({"index": 1, "force": true}));
    ok(&mut s, "shape.rectangle", json!({"x": 900, "y": 900, "width": 10, "height": 10, "props": {"name": "From main"}}));
    assert_eq!(ok(&mut s, "branch.updateFromMain", json!({"id": id}))["updated"], true);
    assert!(names(&mut s).is_empty(), "nothing in the main file's history");
    ok(&mut s, "file.saveAs", json!({"path": path}));
    // Opened again from the file: the branch's history has both entries.
    let mut t = Session::empty();
    ok(&mut t, "file.open", json!({"path": path}));
    ok(&mut t, "branch.open", json!({"id": id}));
    assert_eq!(names(&mut t), [CREATED, UPDATED]);
    assert!(t.doc().unwrap().nodes().any(|n| n.name == "From main"));
    // Merging leaves them with the branch; main gets its own entry for the merge.
    ok(&mut t, "branch.merge", json!({}));
    assert_eq!(names(&mut t).last().map(String::as_str), Some("Merged “Explore”"));
    ok(&mut t, "branch.open", json!({"id": id}));
    assert_eq!(names(&mut t), [CREATED, UPDATED]);
    std::fs::remove_dir_all(&dir).ok();
}
