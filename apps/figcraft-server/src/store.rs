//! The data directory: the files a server keeps, their share links and the server key, and the
//! [`Backend`] the hub serves them through.
//!
//! ```text
//! <data>/LOCK                    held while a server (or a command changing the directory) uses it
//! <data>/server.json             where the running server listens (for the command line)
//! <data>/server.key              the server key: managing files (publish, links, removal)
//! <data>/files/<id>/meta.json    name, created, updated, sequence number
//! <data>/files/<id>/links.json   share links: id, access, label, created, SHA-256 of the token
//! <data>/files/<id>/snapshot.bin the session state at the last compaction
//! <data>/files/<id>/log.bin      what changed since (see `disk`)
//! ```
//!
//! An open file appends its server's journal to its log; the hub has the store make the appends
//! durable before anyone hears of them. After [`StoreConfig::compact_after`] entries or
//! [`StoreConfig::compact_bytes`] of log, when everyone has left, and when the server stops, the
//! state is written as the next snapshot and the log starts over. Link tokens are stored only as
//! hashes and compared in constant time. Whatever is damaged at startup is skipped and reported,
//! never deleted.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use figcraft_collab::message::{Access, Admin, AdminReply, FileInfo, LinkInfo, NewLink};
use figcraft_collab::{Change, Replica, Server, same_secret};
use figcraft_collab_net::hub::{Admission, Backend, Closing, Handled, OpenFile};
use figcraft_collab_net::token;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::disk::{self, LINKS, LOG, LOG_HEADER, META, SNAPSHOT};

pub const LOCK: &str = "LOCK";
pub const KEY: &str = "server.key";
pub const FILES: &str = "files";

/// What the store enforces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoreConfig {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_links: usize,
    pub compact_after: usize,
    pub compact_bytes: u64,
    pub core: figcraft_collab::Limits,
}

/// A file's description (`meta.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Meta {
    name: String,
    created: u64,
    updated: u64,
    #[serde(default)]
    seq: u64,
}

/// A share link (`links.json`): the token itself is never stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Link {
    id: String,
    access: Access,
    label: String,
    created: u64,
    /// SHA-256 of the token, hex.
    hash: String,
}

impl Link {
    fn info(&self, file: &str) -> LinkInfo {
        LinkInfo { id: self.id.clone(), file: file.to_string(), access: self.access, label: self.label.clone(), created: self.created }
    }
}

/// An open file's log.
struct Log {
    generation: u64,
    file: File,
    /// Records not written yet (written and synced at the next flush).
    pending: Vec<u8>,
    /// Entries and bytes since the last compaction.
    entries: usize,
    bytes: u64,
    /// Changed since the last compaction.
    dirty: bool,
}

struct Entry {
    meta: Meta,
    links: Vec<Link>,
    /// Open: its log.
    log: Option<Log>,
}

/// The data directory in use (it holds the directory's lock until dropped).
pub struct Store {
    root: PathBuf,
    cfg: StoreConfig,
    key: String,
    files: BTreeMap<String, Entry>,
    /// What was skipped at startup or found damaged since, for the operator.
    problems: Vec<String>,
    _lock: File,
}

/// Seconds since 1970.
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// SHA-256 of a token, hex.
pub fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// A file or link id as this store makes them (and nothing a path could misuse).
fn valid_id(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A name others may see: trimmed, no control characters, at most 256 characters.
fn clean_name(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(256).collect()
}

/// Take the data directory's lock: `None` when someone else holds it.
pub fn lock(root: &Path) -> Result<Option<File>, String> {
    fs::create_dir_all(root).map_err(|e| format!("can't create the data directory {}: {e}", root.display()))?;
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(LOCK))
        .map_err(|e| format!("can't open {}: {e}", root.join(LOCK).display()))?;
    match f.try_lock() {
        Ok(()) => Ok(Some(f)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(format!("can't lock the data directory: {e}")),
    }
}

/// Read the server key, or make one.
fn load_key(root: &Path) -> Result<String, String> {
    let path = root.join(KEY);
    match fs::read_to_string(&path) {
        Ok(k) if valid_id(k.trim()) && k.trim().len() >= 16 => Ok(k.trim().to_string()),
        Ok(_) => Err(format!("{} isn't a server key (delete it to make a new one)", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => write_key(root),
        Err(e) => Err(format!("can't read {}: {e}", path.display())),
    }
}

/// Make a new server key (only the operator's account can read it on Unix).
fn write_key(root: &Path) -> Result<String, String> {
    let key = token::secret()?;
    let path = root.join(KEY);
    disk::write_atomic(&path, format!("{key}\n").as_bytes()).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

impl Store {
    /// Use the data directory `root` (made if missing). Fails when another server or command holds
    /// it. Files that can't be read are skipped and listed in [`problems`](Self::problems).
    pub fn open(root: &Path, cfg: StoreConfig) -> Result<Store, String> {
        Self::try_open(root, cfg)?.ok_or_else(|| format!("another figcraft-server (or a command) is using {}", root.display()))
    }

    /// [`open`](Self::open), or `None` when someone else holds the directory.
    pub fn try_open(root: &Path, cfg: StoreConfig) -> Result<Option<Store>, String> {
        let Some(lock) = lock(root)? else { return Ok(None) };
        let key = load_key(root)?;
        let dir = root.join(FILES);
        fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
        let mut store = Store { root: root.to_path_buf(), cfg, key, files: BTreeMap::new(), problems: Vec::new(), _lock: lock };
        store.scan(&dir);
        Ok(Some(store))
    }

    fn scan(&mut self, dir: &Path) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => return self.problems.push(format!("can't list {}: {e}", dir.display())),
        };
        let mut ids: Vec<(String, PathBuf)> = entries.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect();
        ids.sort();
        for (id, path) in ids {
            if !path.is_dir() {
                continue;
            }
            if !valid_id(&id) {
                self.problems.push(format!("skipped {}: not a file folder of this server", path.display()));
                continue;
            }
            if self.files.len() >= self.cfg.max_files {
                self.problems.push(format!("skipped file {id}: over the {}-file cap", self.cfg.max_files));
                continue;
            }
            match self.check(&path) {
                Ok(entry) => {
                    self.files.insert(id, entry);
                }
                Err(why) => self.problems.push(format!("skipped file {id}: {why}")),
            }
        }
    }

    /// A file folder's description and links, after checking its snapshot and log. Leftover
    /// temporary files (writes cut short) are removed.
    fn check(&self, path: &Path) -> Result<Entry, String> {
        for name in [SNAPSHOT, LOG, META, LINKS] {
            let tmp = disk::temp_of(&path.join(name));
            if tmp.exists() {
                let _ = fs::remove_file(&tmp);
            }
        }
        let meta: Meta = serde_json::from_slice(&disk::read_capped(&path.join(META), 1 << 20)?).map_err(|e| format!("{META} is damaged ({e})"))?;
        let links: Vec<Link> =
            serde_json::from_slice(&disk::read_capped(&path.join(LINKS), 16 << 20)?).map_err(|e| format!("{LINKS} is damaged ({e})"))?;
        let generation = disk::check_snapshot(&disk::read_capped(&path.join(SNAPSHOT), self.cfg.max_file_bytes)?)?;
        // A damaged log only loses its end; it is reported when the file opens.
        if let Ok(bytes) = disk::read_capped(&path.join(LOG), self.cfg.max_file_bytes)
            && let Ok(log) = disk::read_log(&bytes)
            && log.generation != generation
        {
            log::info!("file at {}: its log predates the last compaction; it is ignored", path.display());
        }
        Ok(Entry { meta, links, log: None })
    }

    /// The server key (for managing files).
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Replace the server key; the new one.
    pub fn rotate_key(&mut self) -> Result<String, String> {
        self.key = write_key(&self.root)?;
        Ok(self.key.clone())
    }

    /// What was skipped or found damaged.
    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    /// How many files are kept.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// No files are kept.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(FILES).join(id)
    }

    fn entry(&mut self, id: &str) -> Result<&mut Entry, String> {
        self.files.get_mut(id).ok_or_else(|| "this file isn't on the server (it may have been removed)".to_string())
    }

    fn info(&self, id: &str, e: &Entry, open: Option<OpenFile>) -> FileInfo {
        let dir = self.dir(id);
        let bytes = [SNAPSHOT, LOG].iter().filter_map(|n| fs::metadata(dir.join(n)).ok()).map(|m| m.len()).sum();
        FileInfo {
            id: id.to_string(),
            name: e.meta.name.clone(),
            created: e.meta.created,
            updated: e.meta.updated,
            seq: open.map_or(e.meta.seq, |o| o.seq),
            bytes,
            people: open.map_or(0, |o| o.people),
        }
    }

    /// Keep a document (its snapshot) as a new file with an edit link and a view link.
    pub fn publish(&mut self, name: &str, snapshot: &[Change]) -> Result<(FileInfo, NewLink, NewLink), String> {
        if self.files.len() >= self.cfg.max_files {
            return Err(format!("the server keeps at most {} files", self.cfg.max_files));
        }
        let replica = Replica::from_snapshot(snapshot).map_err(|e| format!("the file couldn't be read: {e}"))?;
        if replica.document().node_count() > self.cfg.core.max_nodes {
            return Err(format!("the file has more than {} layers", self.cfg.core.max_nodes));
        }
        let mut doc = replica.document().clone();
        let name = match clean_name(name) {
            n if n.is_empty() => doc.name.clone(),
            n => n,
        };
        doc.name = name.clone();
        let server = Server::new(doc, "", self.cfg.core);
        let state = server.state().map_err(|e| e.to_string())?;
        let bytes = disk::encode_snapshot(1, &state)?;
        if bytes.len() as u64 > self.cfg.max_file_bytes {
            return Err(format!("the file is {} MiB; this server keeps files up to {} MiB", bytes.len() >> 20, self.cfg.max_file_bytes >> 20));
        }
        let id = loop {
            let id = token::file_id()?;
            if !self.files.contains_key(&id) && !self.dir(&id).exists() {
                break id;
            }
        };
        let t = now();
        let (edit, edit_token) = new_link(Access::Edit, "Published (edit)", t)?;
        let (view, view_token) = new_link(Access::View, "Published (view)", t)?;
        let meta = Meta { name, created: t, updated: t, seq: 0 };
        let dir = self.dir(&id);
        let written = fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|()| disk::write_atomic(&dir.join(SNAPSHOT), &bytes).map_err(|e| e.to_string()))
            .and_then(|()| disk::write_atomic(&dir.join(LOG), &disk::log_header(1)).map_err(|e| e.to_string()))
            .and_then(|()| write_json(&dir.join(LINKS), &[edit.clone(), view.clone()]))
            // Last: a folder without its description is an upload cut short.
            .and_then(|()| write_json(&dir.join(META), &meta));
        if let Err(e) = written {
            let _ = fs::remove_dir_all(&dir);
            return Err(format!("couldn't keep the file: {e}"));
        }
        let entry = Entry { meta, links: vec![edit.clone(), view.clone()], log: None };
        let info = self.info(&id, &entry, None);
        self.files.insert(id.clone(), entry);
        log::info!("published file {id} ({} bytes)", bytes.len());
        Ok((info, NewLink { link: edit.info(&id), token: edit_token }, NewLink { link: view.info(&id), token: view_token }))
    }

    /// The files kept.
    pub fn list(&self, open: &dyn Fn(&str) -> Option<OpenFile>) -> Vec<FileInfo> {
        self.files.iter().map(|(id, e)| self.info(id, e, open(id))).collect()
    }

    /// A file's links.
    pub fn links(&mut self, file: &str) -> Result<Vec<LinkInfo>, String> {
        Ok(self.entry(file)?.links.iter().map(|l| l.info(file)).collect())
    }

    /// A new link to a file.
    pub fn create_link(&mut self, file: &str, access: Access, label: &str) -> Result<NewLink, String> {
        let max = self.cfg.max_links;
        let path = self.dir(file).join(LINKS);
        let entry = self.entry(file)?;
        if entry.links.len() >= max {
            return Err(format!("a file has at most {max} links: revoke one first"));
        }
        let (link, token) = new_link(access, &clean_name(label), now())?;
        entry.links.push(link.clone());
        if let Err(e) = write_json(&path, &entry.links) {
            entry.links.pop();
            return Err(format!("couldn't keep the link: {e}"));
        }
        Ok(NewLink { link: link.info(file), token })
    }

    /// Revoke a link.
    pub fn revoke(&mut self, file: &str, link: &str) -> Result<(), String> {
        let path = self.dir(file).join(LINKS);
        let entry = self.entry(file)?;
        let at = entry.links.iter().position(|l| l.id == link).ok_or("that file has no such link")?;
        let gone = entry.links.remove(at);
        if let Err(e) = write_json(&path, &entry.links) {
            entry.links.insert(at, gone);
            return Err(format!("couldn't revoke the link: {e}"));
        }
        log::info!("revoked link {link} of file {file}");
        Ok(())
    }

    /// Remove a file and its links for good. `Err` when its folder couldn't be deleted (it is no
    /// longer served either way).
    pub fn remove(&mut self, file: &str) -> Result<(), String> {
        // Closed first: an open log would keep the folder from being deleted on Windows.
        let entry = self.files.remove(file).ok_or("this file isn't on the server (it may have been removed)")?;
        drop(entry);
        let dir = self.dir(file);
        fs::remove_dir_all(&dir)
            .map_err(|e| format!("it is no longer served, but its folder {} couldn't be deleted ({e}); delete it by hand", dir.display()))?;
        log::info!("removed file {file}");
        Ok(())
    }

    /// Load a file's session: its snapshot, then its log (a damaged end is dropped and reported).
    fn load(&mut self, file: &str) -> Result<Server, String> {
        let dir = self.dir(file);
        let (generation, state) = disk::decode_snapshot(&disk::read_capped(&dir.join(SNAPSHOT), self.cfg.max_file_bytes)?)?;
        let mut server = Server::restore(state, "", self.cfg.core).map_err(|e| format!("the snapshot can't be used: {e}"))?;
        let mut issues = Vec::new();
        let mut fresh_log = true;
        let mut kept = 0;
        match disk::read_capped(&dir.join(LOG), self.cfg.max_file_bytes).and_then(|b| disk::read_log(&b)) {
            Ok(log) if log.generation == generation => {
                fresh_log = false;
                for entry in log.entries {
                    if let Err(e) = server.replay(entry) {
                        issues.push(format!("file {file}: its log stops making sense after {kept} entries ({e}); the rest was dropped"));
                        fresh_log = true;
                        break;
                    }
                    kept += 1;
                }
                if let Some(why) = log.damaged {
                    issues.push(format!("file {file}: its log ended with {why}; that part was dropped"));
                    fresh_log = true;
                }
            }
            // From before a compaction that was cut short: the snapshot has it all.
            Ok(_) => {}
            Err(why) => issues.push(format!("file {file}: {why}; starting a new log")),
        }
        for i in issues {
            log::warn!("{i}");
            self.problems.push(i);
        }
        server.journaling(true);
        let file_handle = OpenOptions::new().append(true).open(dir.join(LOG));
        let log = match (fresh_log, file_handle) {
            (false, Ok(f)) => {
                let bytes = f.metadata().map_or(LOG_HEADER as u64, |m| m.len());
                // Replayed entries make it worth compacting once everyone has left.
                Log { generation, file: f, pending: Vec::new(), entries: kept, bytes, dirty: kept > 0 }
            }
            // A new snapshot and log of the next generation, so appends start clean.
            _ => self.rewrite(file, generation + 1, &server)?,
        };
        self.entry(file)?.log = Some(log);
        Ok(server)
    }

    /// Write `server`'s state as snapshot `generation` and start an empty log of it.
    fn rewrite(&mut self, file: &str, generation: u64, server: &Server) -> Result<Log, String> {
        let dir = self.dir(file);
        let state = server.state().map_err(|e| e.to_string())?;
        let bytes = disk::encode_snapshot(generation, &state)?;
        if bytes.len() as u64 > self.cfg.max_file_bytes {
            log::warn!("file {file} is {} MiB, over the {} MiB cap (kept anyway)", bytes.len() >> 20, self.cfg.max_file_bytes >> 20);
        }
        disk::write_atomic(&dir.join(SNAPSHOT), &bytes).map_err(|e| format!("couldn't write the snapshot: {e}"))?;
        disk::write_atomic(&dir.join(LOG), &disk::log_header(generation)).map_err(|e| format!("couldn't start the log: {e}"))?;
        let f = OpenOptions::new().append(true).open(dir.join(LOG)).map_err(|e| format!("couldn't open the log: {e}"))?;
        if let Ok(e) = self.entry(file) {
            e.meta.seq = server.seq();
            let meta = e.meta.clone();
            if let Err(err) = write_json(&dir.join(META), &meta) {
                log::warn!("file {file}: couldn't update {META}: {err}");
            }
        }
        Ok(Log { generation, file: f, pending: Vec::new(), entries: 0, bytes: LOG_HEADER as u64, dirty: false })
    }

    /// Compact an open file: its state as the next snapshot, an empty log.
    fn compact(&mut self, file: &str, server: &Server) {
        let Some(generation) = self.files.get(file).and_then(|e| e.log.as_ref()).map(|l| l.generation) else { return };
        match self.rewrite(file, generation + 1, server) {
            Ok(log) => {
                if let Ok(e) = self.entry(file) {
                    e.log = Some(log);
                }
                log::debug!("compacted file {file}");
            }
            // The old snapshot and log still hold everything: carry on appending.
            Err(e) => log::error!("file {file}: compaction failed: {e}"),
        }
    }

    /// Write and sync what's pending in every open log.
    fn flush_all(&mut self) {
        for (id, e) in &mut self.files {
            if let Some(log) = e.log.as_mut() {
                flush_log(id, log);
            }
        }
    }
}

/// Write and sync a log's pending records (kept for the next try when that fails).
fn flush_log(id: &str, log: &mut Log) {
    if log.pending.is_empty() {
        return;
    }
    match log.file.write_all(&log.pending).and_then(|()| log.file.sync_data()) {
        Ok(()) => {
            log.bytes += log.pending.len() as u64;
            log.pending.clear();
        }
        Err(e) => log::error!("file {id}: couldn't write its log: {e}"),
    }
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    disk::write_atomic(path, &text).map_err(|e| e.to_string())
}

/// A new link and its token.
fn new_link(access: Access, label: &str, created: u64) -> Result<(Link, String), String> {
    let token = token::secret()?;
    Ok((Link { id: token::file_id()?, access, label: label.to_string(), created, hash: hash(&token) }, token))
}

impl Backend for Store {
    fn admit(&mut self, file: &str, token: &str) -> Result<Admission, String> {
        let entry = self.entry(file)?;
        let presented = hash(token);
        // Every link is compared (in constant time), so timing doesn't tell which one matched.
        let mut found = None;
        for l in &entry.links {
            if same_secret(&presented, &l.hash) {
                found = Some(Admission { access: l.access, link: Some(l.id.clone()) });
            }
        }
        found.ok_or_else(|| "this link isn't valid (it may have been revoked)".to_string())
    }

    fn open(&mut self, file: &str) -> Result<Server, String> {
        self.load(file).map_err(|e| {
            let why = format!("file {file} can't be opened: {e}");
            log::error!("{why}");
            self.problems.push(why);
            "this file can't be opened on the server (its data is damaged); tell the server's owner".to_string()
        })
    }

    fn changed(&mut self, file: &str, server: &mut Server) {
        let journal = server.take_journal();
        if journal.is_empty() {
            return;
        }
        let (after, max_bytes) = (self.cfg.compact_after, self.cfg.compact_bytes);
        let Some(e) = self.files.get_mut(file) else { return };
        let Some(log) = e.log.as_mut() else { return };
        for j in &journal {
            match disk::record(j) {
                Ok(r) => log.pending.extend(r),
                Err(err) => log::error!("file {file}: a change couldn't be logged: {err}"),
            }
        }
        log.entries += journal.len();
        log.dirty = true;
        e.meta.updated = now();
        if log.entries >= after || log.bytes + log.pending.len() as u64 >= max_bytes {
            // The snapshot holds what's pending too.
            self.compact(file, server);
        }
    }

    fn flush(&mut self) {
        self.flush_all();
    }

    fn idle(&mut self, file: &str, server: &mut Server) -> bool {
        let dirty = self.files.get_mut(file).and_then(|e| e.log.as_mut()).is_some_and(|log| {
            flush_log(file, log);
            log.dirty
        });
        if dirty {
            self.compact(file, server);
        }
        if let Some(e) = self.files.get_mut(file) {
            e.log = None;
        }
        true
    }

    fn stop(&mut self, open: Vec<(&str, &mut Server)>) {
        self.flush_all();
        for (file, server) in open {
            if self.files.get(file).and_then(|e| e.log.as_ref()).is_some_and(|l| l.dirty) {
                self.compact(file, server);
            }
        }
    }

    fn manage(&mut self, key: &str) -> Result<(), String> {
        if same_secret(key, &self.key) { Ok(()) } else { Err("wrong server key".into()) }
    }

    fn admin(&mut self, req: Admin, open: &dyn Fn(&str) -> Option<OpenFile>) -> Handled {
        let failed = |reason: String| Handled::from(AdminReply::Failed { reason });
        match req {
            Admin::Publish { name, snapshot } => match self.publish(&name, &snapshot) {
                Ok((file, edit, view)) => AdminReply::Published { file, edit, view }.into(),
                Err(e) => failed(e),
            },
            Admin::Files => AdminReply::Files(self.list(open)).into(),
            Admin::Links { file } => self.links(&file).map_or_else(failed, |l| AdminReply::Links(l).into()),
            Admin::CreateLink { file, access, label } => self.create_link(&file, access, &label).map_or_else(failed, |l| AdminReply::Link(l).into()),
            Admin::Revoke { file, link } => match self.revoke(&file, &link) {
                Ok(()) => Handled { reply: AdminReply::Done, revoked: Some((file, link)), removed: None },
                Err(e) => failed(e),
            },
            Admin::Remove { file } => {
                if !self.files.contains_key(&file) {
                    return failed("this file isn't on the server (it may have been removed)".into());
                }
                let reply = match self.remove(&file) {
                    Ok(()) => AdminReply::Done,
                    Err(reason) => AdminReply::Failed { reason },
                };
                Handled { reply, revoked: None, removed: Some(file) }
            }
        }
    }

    fn closing(&self) -> Closing {
        Closing::Restarting
    }
}
