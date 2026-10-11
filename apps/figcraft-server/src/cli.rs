//! The command line: `figcraft-server` serves; `key`, `files` and `links` manage the data
//! directory. When a server is running on the directory they ask it (so revoking a link
//! disconnects whoever joined with it at once); otherwise they change the directory themselves.

use std::fmt::Write as _;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use figcraft_collab::message::{Access, Admin, AdminReply, FileInfo, LinkInfo, NewLink};
use figcraft_collab_net::hub::Backend;
use figcraft_collab_net::{Endpoint, Invite, manage};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::disk;
use crate::store::{self, KEY, Store};

/// Where a running server says it listens.
pub const SERVER_JSON: &str = "server.json";

pub const USAGE: &str = "\
figcraft-server: FigCraft's collaboration server (files, share links, live sessions)

Usage:
  figcraft-server [serve] [--listen ADDR] [--data DIR] [--public-url URL] [--config FILE]
  figcraft-server key [--rotate]                 the address to publish to, with the server key
  figcraft-server files [list]                   the files kept
  figcraft-server files remove FILE              remove a file and its links for good
  figcraft-server links list FILE                a file's share links
  figcraft-server links create FILE --access edit|view [--label TEXT]
  figcraft-server links revoke FILE LINK         revoke a link (whoever joined with it is disconnected)

Options (also keys of the --config TOML file):
  --listen ADDR       where to listen (default 127.0.0.1:47900; 0.0.0.0:47900 for every network)
  --data DIR          the data directory (default ./figcraft-data)
  --public-url URL    how links name the server behind a reverse proxy (wss://host/path/)
  --config FILE       settings: listen, data, public_url, max_files, max_file_mb, max_links,
                      max_connections, max_people, compact_after, compact_mb
  -h, --help          this text
  -V, --version       the version

Logs go to standard output (RUST_LOG=debug for more); they never contain keys or tokens.
";

/// What to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Serve,
    Key { rotate: bool },
    Files,
    RemoveFile { file: String },
    Links { file: String },
    CreateLink { file: String, access: Access, label: String },
    Revoke { file: String, link: String },
    Help,
    Version,
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq)]
pub struct Cli {
    pub config: Config,
    pub command: Command,
}

/// Read the arguments (after the program name).
pub fn parse(args: &[String]) -> Result<Cli, String> {
    // The config file first: flags override it.
    let mut config = match args.iter().position(|a| a == "--config") {
        Some(i) => Config::load(Path::new(args.get(i + 1).ok_or("--config needs a file")?))?,
        None => Config::default(),
    };
    let mut words = Vec::new();
    let mut access = None;
    let mut label = String::new();
    let mut rotate = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "-h" | "--help" => return Ok(Cli { config, command: Command::Help }),
            "-V" | "--version" => return Ok(Cli { config, command: Command::Version }),
            "--config" => {
                value("--config")?;
            }
            "--listen" => config.listen = value("--listen")?,
            "--data" => config.data = PathBuf::from(value("--data")?),
            "--public-url" => config.public_url = Some(value("--public-url")?),
            "--access" => access = Some(Access::parse(&value("--access")?).ok_or("--access must be edit or view")?),
            "--label" => label = value("--label")?,
            "--rotate" => rotate = true,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => words.push(s.to_string()),
        }
    }
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    let command = match w.as_slice() {
        [] | ["serve"] => Command::Serve,
        ["key"] => Command::Key { rotate },
        ["files"] | ["files", "list"] => Command::Files,
        ["files", "remove", file] => Command::RemoveFile { file: file.to_string() },
        ["links", "list", file] => Command::Links { file: file.to_string() },
        ["links", "create", file] => {
            Command::CreateLink { file: file.to_string(), access: access.ok_or("links create needs --access edit or --access view")?, label }
        }
        ["links", "revoke", file, link] => Command::Revoke { file: file.to_string(), link: link.to_string() },
        _ => return Err(format!("unknown command `{}`", words.join(" "))),
    };
    config.check()?;
    Ok(Cli { config, command })
}

/// What a running server writes into its data directory.
#[derive(Debug, Serialize, Deserialize)]
pub struct Running {
    pub listen: SocketAddr,
    pub pid: u32,
}

/// Note where the server listens (for the command line).
pub fn write_running(data: &Path, listen: SocketAddr) -> Result<(), String> {
    let text = serde_json::to_vec_pretty(&Running { listen, pid: std::process::id() }).map_err(|e| e.to_string())?;
    disk::write_atomic(&data.join(SERVER_JSON), &text).map_err(|e| format!("can't write {SERVER_JSON}: {e}"))
}

/// `addr` as something this machine can connect to (the loopback address for "every interface").
fn reachable(addr: SocketAddr) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => SocketAddr::from(([127, 0, 0, 1], addr.port())),
        IpAddr::V6(ip) if ip.is_unspecified() => SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, addr.port())),
        _ => addr,
    }
}

/// How share links and the publish address name the server: `public_url`, else the listen
/// address (this machine's network address for "every interface").
pub fn public_endpoint(cfg: &Config, listen: SocketAddr) -> Result<Endpoint, String> {
    if let Some(e) = cfg.public()? {
        return Ok(e);
    }
    let ip = if listen.ip().is_unspecified() { figcraft_collab_net::lan_ip().unwrap_or(reachable(listen).ip()) } else { listen.ip() };
    Ok(Endpoint::plain(SocketAddr::new(ip, listen.port()).to_string()))
}

/// Run a command other than serving; what to print.
pub fn run(cli: &Cli) -> Result<String, String> {
    let cfg = &cli.config;
    match &cli.command {
        Command::Help => Ok(USAGE.to_string()),
        Command::Version => Ok(format!("figcraft-server {}\n", env!("CARGO_PKG_VERSION"))),
        Command::Serve => Err("serving runs in the foreground: run figcraft-server without a command".into()),
        Command::Key { rotate } => key(cfg, *rotate),
        Command::Files => match admin(cfg, Admin::Files)? {
            AdminReply::Files(files) => Ok(files_table(&files)),
            other => unexpected(other),
        },
        Command::RemoveFile { file } => match admin(cfg, Admin::Remove { file: file.clone() })? {
            AdminReply::Done => Ok(format!("Removed file {file} and its links.\n")),
            other => unexpected(other),
        },
        Command::Links { file } => match admin(cfg, Admin::Links { file: file.clone() })? {
            AdminReply::Links(links) => Ok(links_table(&links)),
            other => unexpected(other),
        },
        Command::CreateLink { file, access, label } => {
            match admin(cfg, Admin::CreateLink { file: file.clone(), access: *access, label: label.clone() })? {
                AdminReply::Link(l) => Ok(new_link(cfg, &l)?),
                other => unexpected(other),
            }
        }
        Command::Revoke { file, link } => match admin(cfg, Admin::Revoke { file: file.clone(), link: link.clone() })? {
            AdminReply::Done => Ok(format!("Revoked link {link}: it no longer joins, and whoever was in the file with it was disconnected.\n")),
            other => unexpected(other),
        },
    }
}

fn unexpected(reply: AdminReply) -> Result<String, String> {
    match reply {
        AdminReply::Failed { reason } => Err(reason),
        other => Err(format!("the server answered {other:?}")),
    }
}

/// The running server's address, when one holds the data directory.
fn running(data: &Path) -> Result<SocketAddr, String> {
    let bytes = disk::read_capped(&data.join(SERVER_JSON), 1 << 16).map_err(|e| format!("a server is using {} but {e}", data.display()))?;
    let r: Running = serde_json::from_slice(&bytes).map_err(|e| format!("{SERVER_JSON} is damaged ({e})"))?;
    Ok(reachable(r.listen))
}

/// Do one request: through the running server, or on the data directory.
fn admin(cfg: &Config, req: Admin) -> Result<AdminReply, String> {
    match Store::try_open(&cfg.data, cfg.store())? {
        Some(mut store) => Ok(store.admin(req, &|_| None).reply),
        None => {
            let addr = running(&cfg.data)?;
            let key = std::fs::read_to_string(cfg.data.join(KEY)).map_err(|e| format!("can't read the server key: {e}"))?;
            let replies = manage::request(&Endpoint::plain(addr.to_string()), key.trim(), vec![req], &cfg.net(), Duration::from_secs(5))?;
            replies.into_iter().next().ok_or_else(|| "the server didn't answer".to_string())
        }
    }
}

/// The address to publish to, with the key.
fn key(cfg: &Config, rotate: bool) -> Result<String, String> {
    let mut store = Store::try_open(&cfg.data, cfg.store())?;
    let listen = match &store {
        Some(_) => cfg.addr()?,
        None => running(&cfg.data)?,
    };
    let key = match (store.as_mut(), rotate) {
        (Some(s), true) => s.rotate_key()?,
        (Some(s), false) => s.key().to_string(),
        (None, true) => return Err("stop the server to replace its key".into()),
        (None, false) => std::fs::read_to_string(cfg.data.join(KEY)).map_err(|e| format!("can't read the server key: {e}"))?.trim().to_string(),
    };
    let public = public_endpoint(cfg, listen)?;
    let mut out = String::new();
    if rotate {
        out.push_str("A new server key: the old one no longer works.\n");
    }
    let _ = writeln!(out, "Publish to this address (in FigCraft: Share > Publish to server…):");
    let _ = writeln!(out, "  {}?key={key}", public.url());
    let _ = writeln!(out, "Whoever has it can publish files and manage their links: share it only with people you trust.");
    Ok(out)
}

fn new_link(cfg: &Config, l: &NewLink) -> Result<String, String> {
    let listen = match running(&cfg.data) {
        Ok(a) if store::lock(&cfg.data)?.is_none() => a,
        _ => cfg.addr()?,
    };
    let invite = Invite { server: public_endpoint(cfg, listen)?, file: l.link.file.clone(), token: l.token.clone() };
    let mut out = String::new();
    let what = if l.link.access == Access::View { "view" } else { "edit" };
    let _ = writeln!(out, "New {what} link {} for file {}:", l.link.id, l.link.file);
    let _ = writeln!(out, "  {}", invite.link());
    let _ = writeln!(out, "  {}", invite.url());
    let _ = writeln!(out, "This is the only time the link is shown (the server keeps only a hash of its token).");
    Ok(out)
}

fn files_table(files: &[FileInfo]) -> String {
    if files.is_empty() {
        return "No files yet (publish one from FigCraft: Share > Publish to server…).\n".into();
    }
    let mut out = String::from("ID                NAME                            PEOPLE  UPDATED               SIZE\n");
    for f in files {
        let _ = writeln!(out, "{:<16}  {:<30}  {:>6}  {:<20}  {}", f.id, elide(&f.name, 30), f.people, date(f.updated), size(f.bytes));
    }
    out
}

fn links_table(links: &[LinkInfo]) -> String {
    if links.is_empty() {
        return "No links (make one with `figcraft-server links create FILE --access view`).\n".into();
    }
    let mut out = String::from("LINK              ACCESS  CREATED               LABEL\n");
    for l in links {
        let _ = writeln!(out, "{:<16}  {:<6}  {:<20}  {}", l.id, l.access.id(), date(l.created), l.label);
    }
    out
}

fn elide(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / f64::from(1 << 20)),
        b if b >= 1 << 10 => format!("{:.1} KiB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

/// `YYYY-MM-DD HH:MM UTC` for seconds since 1970.
pub fn date(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (h, m) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Days to a civil date (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}
