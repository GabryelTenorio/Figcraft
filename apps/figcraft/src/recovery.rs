//! Recovery copies, so a crash, a forced quit or a power cut loses at most a few seconds of work.
//!
//! While the app runs, each document with unsaved changes is copied to `Recovery/<pid>/<uid>.figcraft`
//! in the config folder, in the background, at most every [`INTERVAL`] seconds, and the copy goes
//! once the document is saved or closed. The app holds a lock on its folder. At startup, the folders
//! whose lock is free belong to runs that ended without closing: their copies open again as
//! unsaved documents (`file.recover`) and the folder goes. Closing the app normally removes its own.
//!
//! A document that is one of your online files, in step with its server, has a note beside its
//! copy saying which (`<uid>.online`: the server and the file's id, never a token): the next start
//! opens that file again as its owner (`collab.openFile`) in place of the stale copy. Without
//! the note, or when the file is no longer recorded, the copy opens as any other.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use figcraft_engine::Session;
use serde_json::json;

/// Seconds between copies of a document that keeps changing.
pub const INTERVAL: f64 = 10.0;

const LOCK: &str = "lock";

/// The app's recovery, shared by its frame loop and its services (the panic path).
pub type Shared = Arc<Mutex<Option<Recovery>>>;

pub fn lock(r: &Shared) -> MutexGuard<'_, Option<Recovery>> {
    r.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Recovery {
    dir: PathBuf,
    /// Held while the app runs: a free lock means the run that wrote the folder is gone.
    _lock: File,
    /// The revision each document was last copied at, and the write in flight.
    written: HashMap<u64, (u64, Option<JoinHandle<()>>)>,
    /// When (UI clock) the next copies are due.
    next: f64,
}

impl Recovery {
    /// Take this run's folder under `root` (`None` when it can't be created or locked: no copies).
    pub fn start(root: &Path) -> Option<Self> {
        let dir = root.join(std::process::id().to_string());
        std::fs::create_dir_all(&dir).ok()?;
        let lock = File::create(dir.join(LOCK)).ok()?;
        lock.try_lock().ok()?;
        Some(Self { dir, _lock: lock, written: HashMap::new(), next: 0.0 })
    }

    /// Copy the documents that changed since their last copy and drop the copies of documents that
    /// were saved or closed. Returns how long until it needs to run again (`None`: nothing pending,
    /// an idle window can sleep).
    pub fn tick(&mut self, s: &Session, now: f64) -> Option<f64> {
        let pending = s.docs.iter().any(|d| needs_copy(d) && self.written.get(&d.uid).is_none_or(|w| w.0 != d.revision));
        if now < self.next {
            return pending.then_some(self.next - now);
        }
        self.write(s, false);
        self.next = now + INTERVAL;
        None
    }

    /// Copy every document with unsaved changes; `wait`: before returning (after a panic).
    pub fn write(&mut self, s: &Session, wait: bool) {
        for d in &s.docs {
            let path = self.dir.join(format!("{}.figcraft", d.uid));
            let note = path.with_extension(ONLINE);
            if !needs_copy(d) {
                if self.written.remove(&d.uid).is_some() {
                    let _ = std::fs::remove_file(&path);
                    let _ = std::fs::remove_file(&note);
                }
                continue;
            }
            // Which online file it is (a line each: the server, the file's id), while it is one.
            let _ = match s.online_of(d.uid) {
                Some((server, file)) => std::fs::write(&note, format!("{server}\n{file}")),
                None => std::fs::remove_file(&note),
            };
            let entry = self.written.entry(d.uid).or_insert((0, None));
            let busy = entry.1.as_ref().is_some_and(|h| !h.is_finished());
            if entry.0 == d.revision || busy {
                continue;
            }
            let doc = d.doc.clone();
            let job = move || write_atomic(&path, &doc);
            if wait {
                job();
                entry.1 = None;
            } else {
                entry.1 = std::thread::Builder::new().name("recovery".into()).spawn(job).ok();
            }
            entry.0 = d.revision;
        }
        // Closed documents.
        let open: Vec<u64> = s.docs.iter().map(|d| d.uid).collect();
        let dir = &self.dir;
        self.written.retain(|uid, _| {
            let keep = open.contains(uid);
            if !keep {
                let _ = std::fs::remove_file(dir.join(format!("{uid}.figcraft")));
                let _ = std::fs::remove_file(dir.join(format!("{uid}.{ONLINE}")));
            }
            keep
        });
    }

    /// A normal quit: the documents were saved or their changes discarded on purpose.
    pub fn finish(self) {
        let Self { dir, _lock, written, .. } = self;
        // Windows removes a folder only once nothing in it is open: copies being written first.
        for h in written.into_values().filter_map(|w| w.1) {
            let _ = h.join();
        }
        drop(_lock);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The extension of the note beside the copy of a document that is one of your online files.
const ONLINE: &str = "online";

/// The copy at `file` is of one of your online files that was in step with its server: open that
/// file again as its owner. False when it wasn't one, or can't be opened that way now.
fn reopen_online(file: &Path, s: &mut Session) -> bool {
    let Ok(note) = std::fs::read_to_string(file.with_extension(ONLINE)) else { return false };
    let mut lines = note.lines().map(str::trim);
    let (Some(server), Some(id)) = (lines.next(), lines.next()) else { return false };
    s.execute("collab.openFile", &json!({"server": server, "file": id})).is_ok()
}

/// A document whose changes would be lost (a branch tab's edits are in its main file).
fn needs_copy(d: &figcraft_engine::DocState) -> bool {
    d.is_dirty() && d.branch_of.is_none()
}

fn write_atomic(path: &Path, doc: &figcraft_engine::doc::Document) {
    let Ok(bytes) = figcraft_format::save_bytes(doc, None) else { return };
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Open the copies left by runs that ended without closing (folders under `root` whose lock is
/// free, and copies directly in `root` from older versions) as unsaved documents, then remove them.
/// A copy that can't be read stays where it is. Returns how many documents came back as unsaved
/// copies (your online files open again from their servers instead and aren't counted).
pub fn recover(root: &Path, s: &mut Session) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else { return 0 };
    let mut n = 0;
    let mut open = |file: &Path, s: &mut Session| {
        if file.extension().is_none_or(|e| e != "figcraft") {
            return;
        }
        // One of your online files comes back from its server; anything else from its copy.
        let online = reopen_online(file, s);
        if online || s.execute("file.recover", &json!({"path": file.to_string_lossy()})).is_ok() {
            n += usize::from(!online);
            let _ = std::fs::remove_file(file);
        }
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() {
            open(&path, s);
            continue;
        }
        // Still running (another window of the app), or not ours to take.
        let Ok(lock) = File::options().write(true).create(true).truncate(false).open(path.join(LOCK)) else { continue };
        if lock.try_lock().is_err() {
            continue;
        }
        let files: Vec<PathBuf> = std::fs::read_dir(&path).into_iter().flatten().flatten().map(|f| f.path()).collect();
        for f in &files {
            if f.extension().is_some_and(|e| e == "tmp") {
                // A copy cut off mid-write; the one before it is still there.
                let _ = std::fs::remove_file(f);
            } else {
                open(f, s);
            }
        }
        // The notes go with their copies (one whose copy couldn't be read stays with it).
        for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == ONLINE) && !f.with_extension("figcraft").exists()) {
            let _ = std::fs::remove_file(f);
        }
        drop(lock);
        // Empty now unless a copy couldn't be read (kept for the next start).
        let _ = std::fs::remove_file(path.join(LOCK));
        let _ = std::fs::remove_dir(&path);
    }
    n
}
