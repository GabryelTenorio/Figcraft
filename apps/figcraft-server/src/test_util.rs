//! What the tests share: temporary data directories (removed when dropped), settings, and engine
//! sessions linked over loopback.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use figcraft_collab_net::NetLink;
use figcraft_collab_net::guest::GuestConfig;
use figcraft_engine::Session;
use serde_json::json;

use crate::config::Config;

/// A directory of its own under the system's temporary directory, removed with everything in it
/// when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join("figcraft-server-tests").join(format!("{tag}-{}-{nanos}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Settings for a server on `dir`, listening on a free loopback port.
pub fn config(dir: &Path) -> Config {
    Config { listen: "127.0.0.1:0".into(), data: dir.to_path_buf(), ..Config::default() }
}

/// Joins quickly, and keeps trying long enough to outlast a server restart.
pub fn quick(mut c: GuestConfig) -> GuestConfig {
    c.backoff = Duration::from_millis(20);
    c.max_backoff = Duration::from_millis(200);
    c.attempts = 100;
    c.limits.hello_timeout = Duration::from_secs(5);
    c
}

/// An engine session with the network link, named `name`.
pub fn session(name: &str) -> Session {
    let mut s = Session::new();
    s.live.install(Box::new(NetLink::new(Arc::new(|| {})).with_guest_config(quick)));
    s.execute("pref.userName", &json!({"name": name})).unwrap();
    s
}

/// Pump every session (as their windows would each frame) until `f` holds.
pub fn pump(sessions: &mut [&mut Session], what: &str, f: impl Fn(&[&mut Session]) -> bool) {
    let start = Instant::now();
    loop {
        for s in sessions.iter_mut() {
            s.live_poll();
            s.live_sync();
        }
        if f(sessions) {
            return;
        }
        let states: Vec<String> = sessions.iter().map(|s| s.live_summary().to_string()).collect();
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}: {states:#?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The synced properties of two documents agree.
pub fn same(a: &figcraft_doc::Document, b: &figcraft_doc::Document) -> bool {
    figcraft_collab::diff(&figcraft_collab::Replica::new(a.clone()), b).is_ok_and(|d| d.changes.is_empty())
}
