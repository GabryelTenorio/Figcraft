//! Autosave checkpoints: only for unsaved changes since the last version, capped (oldest
//! autosaves go first, user versions stay), kept in the file with their flag.

use serde_json::{Value, json};

use crate::Session;

fn rect(s: &mut Session, x: f64) {
    s.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 10, "height": 10})).unwrap();
}

fn autosave(s: &mut Session) -> bool {
    s.execute("version.autosave", &json!({})).unwrap()["saved"].as_bool().unwrap()
}

fn list(s: &mut Session) -> Vec<Value> {
    s.execute("version.list", &json!({})).unwrap().as_array().unwrap().clone()
}

#[test]
fn autosaves_follow_unsaved_changes_only() {
    let mut s = Session::new();
    assert!(!autosave(&mut s), "nothing changed");
    rect(&mut s, 0.0);
    assert!(autosave(&mut s));
    let l = list(&mut s);
    assert_eq!((l[0]["name"].as_str(), l[0]["auto"].as_bool()), (Some("Autosave"), Some(true)));
    assert!(!autosave(&mut s), "no change since the checkpoint");
    // A version the user saves is a checkpoint too.
    rect(&mut s, 20.0);
    s.execute("file.saveVersion", &json!({"name": "Mine"})).unwrap();
    assert!(!autosave(&mut s));
    assert!(list(&mut s)[1].get("auto").is_none());
    // Not while a version is shown.
    rect(&mut s, 40.0);
    s.execute("version.preview", &json!({"index": 0})).unwrap();
    assert!(!autosave(&mut s));
    s.execute("version.exitPreview", &json!({})).unwrap();
    assert!(autosave(&mut s));
}

#[test]
fn oldest_autosaves_make_room_and_user_versions_stay() {
    let mut s = Session::new();
    rect(&mut s, 0.0);
    s.execute("file.saveVersion", &json!({"name": "Keep me"})).unwrap();
    for i in 0..(crate::cmd::file::MAX_AUTOSAVES + 5) {
        rect(&mut s, i as f64 * 20.0);
        assert!(autosave(&mut s));
    }
    let l = list(&mut s);
    assert_eq!(l.iter().filter(|v| v["auto"] == true).count(), crate::cmd::file::MAX_AUTOSAVES);
    assert_eq!(l[0]["name"], "Keep me");
}

#[test]
fn autosaves_keep_their_flag_in_the_file() {
    let dir = std::env::temp_dir().join(format!("figcraft-autosave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("auto.figcraft").to_string_lossy().to_string();
    let mut s = Session::new();
    rect(&mut s, 0.0);
    assert!(autosave(&mut s));
    s.execute("file.saveVersion", &json!({"name": "Named"})).unwrap();
    s.execute("file.save", &json!({"path": path})).unwrap();
    let mut t = Session::empty();
    t.execute("file.open", &json!({"path": path})).unwrap();
    let l = list(&mut t);
    assert_eq!(l.len(), 2);
    assert_eq!(l[0]["auto"], true);
    assert!(l[1].get("auto").is_none());
    // Just opened: nothing to autosave.
    assert!(!autosave(&mut t));
    let _ = std::fs::remove_dir_all(&dir);
}
