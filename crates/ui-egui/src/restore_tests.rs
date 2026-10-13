//! Session restore in the interface, driven through egui headless: a window comes back where it
//! was only where a screen shows it; the windows and tabs open now are kept with the preferences
//! as they change (at once, then at most once a second, never while nothing changes) and come
//! back in their windows with their panels, place and state; a window off every screen moves
//! onto one; a build with one window keeps every tab; files that are gone get one message; a
//! file opened since stays in front; the home screen offers the last session after a run that
//! started without it, with the app's crash guard up while it is restored.

use std::sync::{Arc, Mutex};

use egui::{Pos2, RawInput, Rect, ViewportId, ViewportInfo, vec2};
use figcraft_engine::Session;
use figcraft_engine::restore::{SavedSession, SavedTab, SavedWindow};
use serde_json::{Value, json};

use crate::restore::{monitor_of, place};
use crate::{FigApp, RightTab, Services};

const A: [f32; 4] = [0.0, 0.0, 1920.0, 1080.0];
const B: [f32; 4] = [1920.0, 0.0, 1280.0, 1024.0];

type Screen = [f32; 4];
/// What the case is, the saved rectangle, the screen it was on, the screens now, where it opens.
type Case = (&'static str, [f32; 4], Option<Screen>, &'static [Screen], [f32; 4]);

#[test]
fn a_window_comes_back_where_a_screen_shows_it() {
    let nan = f32::NAN;
    let table: &[Case] = &[
        ("on its screen", [100.0, 80.0, 1200.0, 800.0], Some(A), &[A, B], [100.0, 80.0, 1200.0, 800.0]),
        ("across two screens", [1500.0, 50.0, 900.0, 700.0], Some(A), &[A, B], [1500.0, 50.0, 900.0, 700.0]),
        ("on the second screen", [2000.0, 100.0, 1000.0, 700.0], Some(B), &[A, B], [2000.0, 100.0, 1000.0, 700.0]),
        ("its screen is gone", [2000.0, 100.0, 1000.0, 700.0], Some(B), &[A], [920.0, 100.0, 1000.0, 700.0]),
        ("far off every screen", [-5000.0, -5000.0, 800.0, 600.0], None, &[A, B], [0.0, 0.0, 800.0, 600.0]),
        ("only a sliver shows", [1900.0, 100.0, 800.0, 600.0], Some(A), &[A], [1120.0, 100.0, 800.0, 600.0]),
        ("its top strip is above the screen", [300.0, -200.0, 800.0, 600.0], Some(A), &[A], [300.0, 0.0, 800.0, 600.0]),
        ("its top strip is below the screen", [300.0, 1075.0, 800.0, 600.0], Some(A), &[A], [300.0, 480.0, 800.0, 600.0]),
        ("the bottom hangs off: it can still be grabbed", [300.0, 900.0, 800.0, 600.0], Some(A), &[A], [300.0, 900.0, 800.0, 600.0]),
        ("larger than the screen", [5000.0, 5000.0, 4000.0, 3000.0], None, &[A], [0.0, 0.0, 1920.0, 1080.0]),
        ("off screen: back to its own screen, not the nearest", [-900.0, 100.0, 800.0, 600.0], Some(B), &[A, B], [1920.0, 100.0, 800.0, 600.0]),
        (
            "off screen: the nearest when its own is gone",
            [3300.0, 100.0, 800.0, 600.0],
            Some([3200.0, 0.0, 1000.0, 1000.0]),
            &[A, B],
            [2400.0, 100.0, 800.0, 600.0],
        ),
        ("narrower than the strip to grab", [1850.0, 100.0, 100.0, 100.0], Some(A), &[A], [1820.0, 100.0, 100.0, 100.0]),
        ("no screens known", [-5000.0, 0.0, 800.0, 600.0], None, &[], [-5000.0, 0.0, 800.0, 600.0]),
        (
            "screens that aren't",
            [-5000.0, 0.0, 800.0, 600.0],
            None,
            &[[0.0, 0.0, 0.0, 0.0], [f32::NAN, 0.0, 100.0, 100.0]],
            [-5000.0, 0.0, 800.0, 600.0],
        ),
    ];
    for (what, rect, monitor, monitors, want) in table {
        assert_eq!(place(*rect, *monitor, monitors), *want, "{what}");
        // Where it opens, a screen shows its top strip.
        if let Some(m) = monitor_of(*want, monitors) {
            assert!(want[1] >= m[1] - 0.5 && want[1] < m[1] + m[3], "{what}");
        }
    }
    assert!(place([nan, 0.0, 800.0, 600.0], None, &[A])[0].is_nan(), "never a panic on a rectangle that isn't one");
    assert_eq!(monitor_of([1500.0, 50.0, 900.0, 700.0], &[A, B]), Some(B), "the screen showing most of it");
    assert_eq!(monitor_of([-5000.0, 0.0, 800.0, 600.0], &[A, B]), None);
}

/// What the app's services were asked: the sessions kept, and the restoring mark put up or down.
#[derive(Default)]
struct Asked {
    kept: Vec<SavedSession>,
    marks: Vec<bool>,
}

struct Keeps(Arc<Mutex<Asked>>);
impl Services for Keeps {
    fn save_prefs(&mut self, prefs: &figcraft_engine::Prefs) {
        self.0.lock().unwrap().kept.push(prefs.session.clone());
    }
    fn restoring(&mut self, on: bool) {
        self.0.lock().unwrap().marks.push(on);
    }
}

/// The app with egui's context and clock, as the desktop app runs it (windows are native).
struct Run {
    ctx: egui::Context,
    app: FigApp,
    asked: Arc<Mutex<Asked>>,
    time: f64,
    /// Where the system says the main window is: `[x, y, width, height]`, maximized.
    main: Option<([f32; 4], bool)>,
    /// Where the main window was told to go and what state to take (the commands' names), in order.
    told: Vec<String>,
}

impl Run {
    fn new(s: Session) -> Self {
        let asked = Arc::new(Mutex::new(Asked::default()));
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        Self { ctx, app: FigApp::new(s, Box::new(Keeps(asked.clone()))), asked, time: 0.0, main: None, told: Vec::new() }
    }

    /// One main-window frame `secs` later.
    fn frame(&mut self, secs: f64) {
        self.time += secs;
        let mut input = RawInput { time: Some(self.time), ..Default::default() };
        if let Some(([x, y, w, h], maximized)) = self.main {
            let rect = Rect::from_min_size(Pos2::new(x, y), vec2(w, h));
            let info = ViewportInfo { outer_rect: Some(rect), inner_rect: Some(rect), maximized: Some(maximized), ..Default::default() };
            input.viewports.insert(ViewportId::ROOT, info);
        }
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
        let commands = out.viewport_output.get(&ViewportId::ROOT).map(|v| v.commands.as_slice()).unwrap_or_default();
        let name = |c: &egui::ViewportCommand| format!("{c:?}").split('(').next().unwrap_or_default().to_string();
        self.told.extend(commands.iter().map(name).filter(|c| ["OuterPosition", "InnerSize", "Maximized", "Fullscreen"].contains(&c.as_str())));
    }

    /// Frames until the interface is up (the first one only installs the fonts).
    fn up(mut self) -> Self {
        for _ in 0..3 {
            self.frame(0.01);
        }
        self
    }

    fn kept(&self) -> usize {
        self.asked.lock().unwrap().kept.len()
    }

    fn windows(&self) -> Value {
        crate::windows::list(&self.app)
    }
}

/// A folder of saved files named `names`, and a session with them open.
fn open_files(test: &str, names: &[&str]) -> (std::path::PathBuf, Session, Vec<String>) {
    let dir = std::env::temp_dir().join(format!("figcraft-ui-restore-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::empty();
    let mut paths = Vec::new();
    for n in names {
        let path = dir.join(format!("{n}.figcraft")).to_string_lossy().into_owned();
        s.execute("file.new", &json!({"name": n})).unwrap();
        s.execute("file.saveAs", &json!({"path": path})).unwrap();
        paths.push(path);
    }
    (dir, s, paths)
}

/// The next run: nothing open, the preferences as they were kept.
fn next_run(prefs: &figcraft_engine::Prefs) -> Run {
    let mut s = Session::empty();
    s.prefs = serde_json::from_value(serde_json::to_value(prefs).unwrap()).unwrap();
    Run::new(s)
}

fn tab(path: &str) -> SavedTab {
    SavedTab { path: Some(path.into()), ..SavedTab::default() }
}

#[test]
fn the_session_is_kept_as_it_changes_and_not_while_it_doesnt() {
    let (dir, mut s, paths) = open_files("kept", &["A", "B"]);
    s.execute("file.new", &json!({"name": "Scratch"})).unwrap();
    s.execute("doc.switch", &json!({"index": 0})).unwrap();
    let mut r = Run::new(s);
    r.main = Some(([40.0, 30.0, 1300.0, 820.0], false));
    r = r.up();
    assert_eq!(r.kept(), 1, "kept at once");
    let saved = r.app.session.prefs.session.clone();
    assert_eq!(saved.windows.len(), 1);
    let w = &saved.windows[0];
    assert_eq!(
        w.tabs.iter().map(|t| t.path.clone().unwrap()).collect::<Vec<_>>(),
        paths,
        "saved files only: unsaved work is the recovery copies' job"
    );
    assert_eq!((w.front, w.rect, w.maximized), (0, Some([40.0, 30.0, 1300.0, 820.0]), false));
    assert_eq!((w.left_tab.as_str(), w.right_tab.as_str(), w.panel_widths), ("file", "design", Some([240.0, 240.0])));
    // Nothing changes: nothing is written, and nothing asks for another frame to write.
    for _ in 0..30 {
        r.frame(0.5);
    }
    assert_eq!(r.kept(), 1);
    // The tab in front, the panel and the window's place change in quick succession: one more
    // write once a second has passed since the last, with all of it.
    r.frame(5.0);
    r.app.session.execute("doc.switch", &json!({"index": 1})).unwrap();
    r.frame(0.1);
    assert_eq!(r.kept(), 2);
    r.app.ui.right_tab = RightTab::Inspect;
    r.frame(0.1);
    r.main = Some(([300.0, 200.0, 1000.0, 700.0], false));
    r.frame(0.1);
    assert_eq!(r.kept(), 2, "not on every change");
    r.frame(1.0);
    assert_eq!(r.kept(), 3);
    let last = r.asked.lock().unwrap().kept.last().cloned().unwrap();
    assert_eq!(last, r.app.session.prefs.session);
    let w = &last.windows[0];
    assert_eq!((w.front, w.right_tab.as_str(), w.rect), (1, "inspect", Some([300.0, 200.0, 1000.0, 700.0])));
    // Maximized: the state is kept, and the place it goes back to.
    r.main = Some(([0.0, 0.0, 1920.0, 1080.0], true));
    r.frame(2.0);
    let w = r.app.session.prefs.session.windows[0].clone();
    assert_eq!((w.maximized, w.rect), (true, Some([300.0, 200.0, 1000.0, 700.0])));
    assert_eq!(crate::control::handle(&mut r.app, &r.ctx, "ui.inspect", &json!({})).unwrap()["session"]["saved"]["windows"][0]["maximized"], true);
    // Quitting: the window on its way out isn't how the app was left.
    let before = r.kept();
    r.app.ui.quit = true;
    r.app.session.execute("file.close", &json!({"index": 0, "force": true})).unwrap();
    r.frame(2.0);
    assert_eq!(r.kept(), before);
    assert_eq!(r.app.session.prefs.session.windows[0].tabs.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn windows_and_tabs_come_back_as_they_were_left() {
    let (dir, s, _) = open_files("windows", &["A", "B", "C"]);
    let mut r = Run::new(s).up();
    // B and C move to a second window, which shows C; the main window is left on Dev Mode.
    let w = crate::control::handle(&mut r.app, &r.ctx, "window.new", &json!({"index": 1})).unwrap()["window"].as_u64().unwrap();
    r.frame(0.1);
    let c = r.app.session.docs[2].uid;
    crate::windows::adopt(&mut r.app, w, &[c], Some(c));
    r.app.ui.right_tab = RightTab::Inspect;
    // A is zoomed in on a corner.
    r.frame(0.1);
    (r.app.session.docs[0].view.cx, r.app.session.docs[0].view.zoom) = (77.0, 3.0);
    r.app.set_panel_widths(300.0, 320.0);
    r.main = Some(([10.0, 20.0, 1100.0, 760.0], false));
    r.frame(2.0);
    let mut prefs = r.app.session.prefs.clone();
    assert_eq!(prefs.session.windows.iter().map(|w| w.tabs.len()).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(prefs.session.windows[1].front, 1);
    // The second window was left maximized, over this place.
    prefs.session.windows[1].rect = Some([200.0, 150.0, 1000.0, 720.0]);
    prefs.session.windows[1].maximized = true;
    prefs.session.windows[1].right_tab = "prototype".into();
    let left = prefs.session.clone();

    // The app quits and starts again.
    let mut r = next_run(&prefs);
    let out = r.app.run("session.restore", json!({})).unwrap();
    assert_eq!((&out["windows"], &out["opened"]), (&json!(2), &json!(3)));
    r = r.up();
    let list = r.windows();
    assert_eq!((&list[0]["files"], &list[0]["active"], &list[0]["rect"]), (&json!(["A"]), &json!("A"), &json!([10.0, 20.0, 1100.0, 760.0])));
    assert_eq!((&list[1]["files"], &list[1]["active"]), (&json!(["B", "C"]), &json!("C")));
    assert_eq!(
        (&list[1]["rect"], &list[1]["maximized"], &list[1]["fullscreen"]),
        (&json!([200.0, 150.0, 1000.0, 720.0]), &json!(true), &json!(false))
    );
    assert_eq!((r.app.ui.right_tab, r.app.ui.left_width, r.app.ui.right_width), (RightTab::Inspect, 300.0, 320.0), "the main window's panels");
    assert_eq!(r.app.windows.list[0].pane.ui.right_tab, RightTab::Prototype, "the other window's own");
    assert!(!r.app.ui.home && r.app.session.ds().unwrap().doc.name == "A");
    let view = &r.app.session.ds().unwrap().view;
    assert_eq!((view.cx, view.zoom), (77.0, 3.0), "drawn, and still where it was left (not fitted to the window again)");
    assert_eq!(r.app.session.prefs.session, left, "what is kept now is what was left");
    // The crash guard goes once the restore has settled, not before.
    assert_eq!(r.asked.lock().unwrap().marks, Vec::<bool>::new());
    r.frame(1.5);
    assert_eq!(r.asked.lock().unwrap().marks, [false]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_window_off_every_screen_moves_onto_one_and_one_window_keeps_every_tab() {
    let (dir, s, paths) = open_files("screens", &["A", "B"]);
    let mut prefs = s.prefs.clone();
    prefs.session = SavedSession {
        windows: vec![
            SavedWindow {
                rect: Some([-4000.0, 100.0, 1200.0, 800.0]),
                monitor: Some([-3840.0, 0.0, 1920.0, 1080.0]),
                tabs: vec![tab(&paths[0])],
                ..SavedWindow::default()
            },
            SavedWindow { rect: Some([2500.0, 300.0, 5000.0, 4000.0]), tabs: vec![tab(&paths[1])], ..SavedWindow::default() },
        ],
    };
    let mut r = next_run(&prefs);
    r.app.monitors = vec![A];
    r.app.run("session.restore", json!({}));
    r = r.up();
    let list = r.windows();
    assert_eq!((&list[0]["rect"], &list[0]["monitor"]), (&json!([0.0, 100.0, 1200.0, 800.0]), &json!(A)));
    assert_eq!((&list[1]["rect"], &list[1]["files"]), (&json!(A), &json!(["B"])));
    // A build with one window (the browser): the tabs of every window, in this one.
    let mut r = next_run(&prefs);
    r.ctx.set_embed_viewports(true);
    r.app.run("session.restore", json!({}));
    r = r.up();
    let list = r.windows();
    assert_eq!((list.as_array().map(Vec::len), &list[0]["files"]), (Some(1), &json!(["A", "B"])));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn gone_files_get_one_message_and_a_file_opened_since_stays_in_front() {
    let (dir, s, paths) = open_files("gone", &["A", "B", "C", "D"]);
    let mut prefs = s.prefs.clone();
    prefs.session =
        SavedSession { windows: vec![SavedWindow { tabs: paths[..3].iter().map(|p| tab(p)).collect(), front: 1, ..SavedWindow::default() }] };
    std::fs::remove_file(&paths[0]).unwrap();
    std::fs::remove_file(&paths[2]).unwrap();
    let mut r = next_run(&prefs);
    r.app.run("session.restore", json!({}));
    // The file the app was started with opens after, in front; documents recovered after a
    // crash are still announced.
    r.app.session.execute("file.open", &json!({"path": paths[3]})).unwrap();
    r.app.note_recovered(1);
    r = r.up();
    let toast = r.app.ui.toast.clone().unwrap().0;
    assert!(toast.starts_with("Recovered an unsaved file") && toast.ends_with("\n2 files couldn't be reopened"), "{toast}");
    assert_eq!(r.windows()[0]["files"], json!(["B", "D"]));
    assert_eq!(r.app.session.ds().unwrap().doc.name, "D");
    // The files that are gone stay in Open recent (greyed there), and leave the kept session.
    assert!(r.app.session.prefs.recent_files.contains(&paths[0]));
    assert_eq!(r.app.session.prefs.session.windows[0].tabs.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_home_screen_offers_the_last_session_with_the_crash_guard_up() {
    let (dir, s, paths) = open_files("offer", &["A", "B"]);
    let mut prefs = s.prefs.clone();
    prefs.session = SavedSession { windows: vec![SavedWindow { tabs: vec![tab(&paths[0]), tab(&paths[1])], front: 1, ..SavedWindow::default() }] };
    // A run that starts without restoring: the last session is on offer, on the home screen.
    let mut r = next_run(&prefs);
    r.app.session.restore.last = Some(r.app.session.prefs.session.clone());
    r.app.ui.home = true;
    r = r.up();
    assert!(r.app.restore_offered());
    let inspect = r.app.inspect();
    assert_eq!((&inspect["session"]["offered"], &inspect["home"]), (&json!(true), &json!(true)));
    assert!(r.app.session.prefs.session.tabs() == 0, "this run's own session is what is kept now");
    // The button: the guard is up before anything opens, and down once it settled.
    r.app.restore_last();
    assert_eq!(r.asked.lock().unwrap().marks, [true]);
    r.frame(0.1);
    assert_eq!((r.windows()[0]["files"].clone(), r.windows()[0]["active"].clone()), (json!(["A", "B"]), json!("B")));
    assert!(!r.app.ui.home && !r.app.restore_offered());
    r.frame(1.5);
    assert_eq!(r.asked.lock().unwrap().marks, [true, false]);
    assert_eq!(r.app.session.prefs.session.tabs(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_window_left_maximized_comes_back_maximized_over_its_place() {
    let (dir, s, paths) = open_files("maximized", &["A"]);
    let mut prefs = s.prefs.clone();
    let rect = [200.0, 120.0, 1300.0, 850.0];
    prefs.session =
        SavedSession { windows: vec![SavedWindow { rect: Some(rect), maximized: true, tabs: vec![tab(&paths[0])], ..SavedWindow::default() }] };
    // The app starts: its window opens maximized (the app asked for that), and placing it again
    // would take it out of that state.
    let mut r = next_run(&prefs);
    r.main = Some(([0.0, 0.0, 1920.0, 1040.0], true));
    r.app.run("session.restore", json!({}));
    r = r.up();
    assert_eq!(r.told, Vec::<String>::new());
    let w = r.app.session.prefs.session.windows[0].clone();
    assert_eq!((w.maximized, w.rect), (true, Some(rect)), "the place under it is still the one to go back to");
    // Restored while the app runs, from a window that isn't maximized: placed first, then maximized.
    let mut r = next_run(&prefs);
    r.main = Some(([50.0, 50.0, 1000.0, 700.0], false));
    r = r.up();
    r.app.run("session.restore", json!({"session": serde_json::to_value(&prefs.session).unwrap()}));
    r.frame(0.1);
    assert_eq!(r.told, ["OuterPosition", "InnerSize", "Maximized"]);
    // And back: a maximized window leaves that state before it is placed.
    let mut normal = prefs.session.clone();
    normal.windows[0].maximized = false;
    r.main = Some(([0.0, 0.0, 1920.0, 1040.0], true));
    r.frame(0.1);
    r.told.clear();
    r.app.run("session.restore", json!({"session": serde_json::to_value(&normal).unwrap()}));
    r.frame(0.1);
    assert_eq!(r.told, ["Maximized", "OuterPosition", "InnerSize"]);
    let _ = std::fs::remove_dir_all(dir);
}
