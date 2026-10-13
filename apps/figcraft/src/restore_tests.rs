//! Starting the app with the last session: the windows and tabs kept in the preferences file
//! open again, with the restoring mark up; a mark left by a run that never got through restoring
//! starts this one without them, on offer; the preference off and `--fresh` start without them
//! too; a file given on the command line opens in front of them; a preferences file that is
//! junk, cut off or huge never stops the app starting; and after a crash the unsaved documents
//! come back from their recovery copies beside the restored tabs, while a normal quit keeps none.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use figcraft_engine::Session;
use figcraft_ui_egui::FigApp;
use serde_json::json;

use crate::recovery::{Recovery, recover};
use crate::services::{Desktop, Start, mark_restoring, start_session};

fn root(name: &str) -> PathBuf {
    let r = std::env::temp_dir().join(format!("figcraft-session-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    r
}

/// A run of the app with its preferences in `dir`: files `names` saved there and open, drawn
/// until the session is kept, then gone without a word (a crash or a kill; a quit writes the
/// same preferences). Returns the files' paths.
fn left_open(dir: &Path, names: &[&str]) -> Vec<String> {
    let mut s = Session::empty();
    let mut paths = Vec::new();
    for n in names {
        let path = dir.join(format!("{n}.figcraft")).to_string_lossy().into_owned();
        s.execute("file.new", &json!({"name": n})).unwrap();
        s.execute("file.saveAs", &json!({"path": path})).unwrap();
        paths.push(path);
    }
    s.execute("doc.switch", &json!({"index": 0})).unwrap();
    let mut app = FigApp::new(s, Box::new(Desktop::load_from(Some(dir.to_path_buf()), Arc::new(Mutex::new(None)))));
    let ctx = egui::Context::default();
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui)).textures_delta.clear();
    }
    paths
}

/// The next run's session, with the preferences in `dir` loaded.
fn next(dir: &Path) -> Session {
    let svc = Desktop::load_from(Some(dir.to_path_buf()), Arc::new(Mutex::new(None)));
    let mut s = Session::empty();
    s.prefs = svc.prefs.clone().expect("the preferences were kept");
    s
}

fn names(s: &Session) -> Vec<String> {
    s.docs.iter().map(|d| d.doc.name.clone()).collect()
}

fn marked(dir: &Path) -> bool {
    dir.join("restoring").exists()
}

#[test]
fn the_tabs_left_open_come_back_and_a_command_line_file_opens_in_front() {
    let dir = root("restart");
    let paths = left_open(&dir, &["A", "B", "C"]);
    let extra = left_open(&root("restart-extra"), &["Extra"]).remove(0);
    let mut s = next(&dir);
    assert_eq!(s.prefs.session.tabs(), 3, "kept while the app ran, not only when it quits");
    assert_eq!(start_session(Some(&dir), &mut s, false), Start::Restore);
    assert!(marked(&dir), "the mark is up until the interface says the restore settled");
    assert_eq!((names(&s), s.ds().unwrap().doc.name.as_str()), (vec!["A".to_string(), "B".into(), "C".into()], "A"));
    assert_eq!(s.docs.iter().map(|d| d.path.clone().unwrap()).collect::<Vec<_>>(), paths);
    // What the app was started with opens in addition, in front.
    s.execute("file.open", &json!({"path": extra})).unwrap();
    let mut app = FigApp::new(s, Box::new(Desktop::load_from(Some(dir.clone()), Arc::new(Mutex::new(None)))));
    app.handle_ui_requests();
    let ctx = egui::Context::default();
    for i in 0..4 {
        let input = egui::RawInput { time: Some(f64::from(i)), ..Default::default() };
        ctx.run_ui(input, |ui| app.ui(ui)).textures_delta.clear();
    }
    assert_eq!(
        (names(&app.session), app.session.ds().unwrap().doc.name.as_str()),
        (vec!["A".to_string(), "B".into(), "C".into(), "Extra".into()], "Extra")
    );
    assert!(!marked(&dir), "settled: the mark is down");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_run_that_never_got_through_restoring_is_not_tried_again() {
    let dir = root("guard");
    left_open(&dir, &["A", "B"]);
    // The last run put the mark up and never took it down.
    mark_restoring(&dir, true);
    let mut s = next(&dir);
    assert_eq!(start_session(Some(&dir), &mut s, false), Start::Guarded);
    assert!(s.docs.is_empty() && !marked(&dir));
    assert_eq!(s.restore.last.as_ref().map(figcraft_engine::restore::SavedSession::tabs), Some(2), "on offer instead");
    // The home screen's button restores it, with the mark up meanwhile; a crash then is
    // guarded the same way.
    let mut app = FigApp::new(s, Box::new(Desktop::load_from(Some(dir.clone()), Arc::new(Mutex::new(None)))));
    assert!(app.restore_offered());
    app.restore_last();
    assert!(marked(&dir));
    assert_eq!(names(&app.session), ["A", "B"]);
    let mut s = next(&dir);
    assert_eq!(start_session(Some(&dir), &mut s, false), Start::Guarded);
    // The run after that one starts as any other.
    let mut s = next(&dir);
    assert_eq!(start_session(Some(&dir), &mut s, false), Start::Restore);
    assert_eq!(names(&s), ["A", "B"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_preference_off_and_fresh_start_without_restoring() {
    let given: Vec<String> = ["--fresh", "a.figcraft"].iter().map(|a| a.to_string()).collect();
    let args = crate::args::parse(&given);
    assert!(args.fresh && args.files == ["a.figcraft"] && !crate::args::parse(&given[1..]).fresh);
    let dir = root("off");
    left_open(&dir, &["A"]);
    let mut s = next(&dir);
    assert_eq!(start_session(Some(&dir), &mut s, true), Start::Fresh, "--fresh");
    assert!(s.docs.is_empty() && !marked(&dir));
    assert!(s.restore.last.is_some(), "`session.restore` still brings it back when asked");
    let mut s = next(&dir);
    s.execute("pref.restoreSession", &json!({"on": false})).unwrap();
    assert_eq!(start_session(Some(&dir), &mut s, false), Start::Fresh, "the preference");
    assert!(s.docs.is_empty() && !marked(&dir));
    s.execute("session.restore", &json!({})).unwrap();
    assert_eq!(names(&s), ["A"]);
    // Nothing kept (a first run): nothing to restore, nothing offered.
    let mut s = Session::empty();
    assert_eq!(start_session(None, &mut s, false), Start::Fresh);
    assert!(s.restore.last.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_preferences_file_that_is_junk_never_stops_the_app_starting() {
    let dir = root("junk");
    let paths = left_open(&dir, &["A"]);
    let state = dir.join("state.json");
    let whole = std::fs::read_to_string(&state).unwrap();
    let load = || Desktop::load_from(Some(dir.clone()), Arc::new(Mutex::new(None))).prefs;
    // Cut off anywhere: the preferences are the defaults, or whole.
    for cut in (0..whole.len()).step_by((whole.len() / 60).max(1)).filter(|c| whole.is_char_boundary(*c)) {
        std::fs::write(&state, &whole[..cut]).unwrap();
        assert!(load().is_none_or(|p| p.session.tabs() <= 1));
    }
    // The session as anything at all: the other preferences are still read.
    let nan_rect = "[1e999, 0, 800, 600]";
    for session in
        ["null", "7", "\"x\"", "[]", "{\"windows\": 4}", "{\"windows\": [[], 3, {\"tabs\": [{\"path\": 9}, 4], \"rect\": \"wide\"}]}", nan_rect]
    {
        std::fs::write(&state, format!("{{\"prefs\": {{\"theme\": \"dark\", \"session\": {session}}}}}")).unwrap();
        let mut s = Session::empty();
        if let Some(p) = load() {
            assert_eq!((p.theme.as_str(), p.session.tabs()), ("dark", 0), "{session}");
            s.prefs = p;
        }
        // (A window without tabs still comes back where it was.)
        assert_ne!(start_session(Some(&dir), &mut s, false), Start::Guarded, "{session}");
        assert!(s.docs.is_empty(), "{session}");
        mark_restoring(&dir, false);
    }
    // Wrong types beside usable tabs, absurd rectangles: what is usable comes back.
    let session = json!({"windows": [{"rect": [0, 0, 0, 0], "maximized": "no", "tabs": [{"path": paths[0], "page": "first", "view": [1, 2]}, {"path": 5}], "front": 99}]});
    std::fs::write(&state, json!({"prefs": {"session": session}}).to_string()).unwrap();
    let mut s = next(&dir);
    assert_eq!(s.prefs.session.windows[0].rect, None);
    assert_eq!(start_session(Some(&dir), &mut s, false), Start::Restore);
    assert_eq!(names(&s), ["A"]);
    mark_restoring(&dir, false);
    // Huge: thousands of windows and tabs are cut to the limits; a file too large to be ours
    // isn't read at all.
    let tabs: Vec<_> = (0..2000).map(|i| json!({"path": format!("{i}.figcraft")})).collect();
    let windows: Vec<_> = (0..100).map(|_| json!({"tabs": tabs})).collect();
    std::fs::write(&state, json!({"prefs": {"session": {"windows": windows}}}).to_string()).unwrap();
    let p = load().unwrap();
    assert_eq!(
        (p.session.windows.len(), p.session.windows[0].tabs.len()),
        (figcraft_engine::restore::MAX_WINDOWS, figcraft_engine::restore::MAX_TABS)
    );
    let file = std::fs::File::create(&state).unwrap();
    file.set_len(80 << 20).unwrap();
    drop(file);
    assert!(load().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn after_a_crash_unsaved_documents_come_back_beside_the_restored_tabs_and_a_quit_keeps_none() {
    let dir = root("crash");
    let recovery = dir.join("Recovery");
    let paths = left_open(&dir, &["A"]);
    // The run had A open with a change that wasn't saved, and a document never saved.
    let mut s = next(&dir);
    s.execute("file.open", &json!({"path": paths[0]})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    s.execute("file.new", &json!({"name": "Scratch"})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    let mut r = Recovery::start(&recovery).unwrap();
    r.write(&s, true);
    // Killed: the lock goes with the process. The next start gets both back, and the tab.
    drop(r);
    let mut killed = next(&dir);
    assert_eq!(recover(&recovery, &mut killed), 2);
    assert_eq!(start_session(Some(&dir), &mut killed, false), Start::Restore);
    assert_eq!(killed.docs.iter().filter(|d| d.is_dirty() && d.path.is_none()).count(), 2, "the recovered copies, unsaved");
    assert_eq!(killed.docs.iter().filter(|d| d.path.as_deref() == Some(paths[0].as_str()) && !d.is_dirty()).count(), 1, "the tab, as saved");
    mark_restoring(&dir, false);
    // A normal quit instead (Don't save was the answer, or there was nothing to ask): the
    // copies go, and the saved file is all that comes back.
    let mut r = Recovery::start(&recovery).unwrap();
    r.write(&s, true);
    r.finish();
    let mut quit = next(&dir);
    assert_eq!(recover(&recovery, &mut quit), 0);
    assert_eq!(start_session(Some(&dir), &mut quit, false), Start::Restore);
    assert_eq!((names(&quit), quit.ds().unwrap().is_dirty()), (vec!["A".to_string()], false));
    let _ = std::fs::remove_dir_all(dir);
}
