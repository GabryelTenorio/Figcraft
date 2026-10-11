//! The server's settings: defaults, then a TOML file (`--config server.toml`), then flags.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use figcraft_collab_net::{Endpoint, NetLimits};
use serde::Deserialize;

use crate::store::StoreConfig;

/// The port the server listens on unless told otherwise (the app's sessions try it first too).
pub const DEFAULT_PORT: u16 = figcraft_collab_net::DEFAULT_PORT;

/// Everything the server can be told (the TOML file's keys are these names).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where to listen: `127.0.0.1:47900` (this machine only) unless told otherwise.
    pub listen: String,
    /// The data directory (files, links, the server key).
    pub data: PathBuf,
    /// How share links name this server when it isn't reached at `listen` (a reverse proxy:
    /// `wss://figcraft.example.com/`).
    pub public_url: Option<String>,
    /// Most files kept.
    pub max_files: usize,
    /// Largest file (its snapshot), in MiB (a welcome carries the whole file, and messages are at
    /// most 64 MiB).
    pub max_file_mb: u64,
    /// Most share links per file.
    pub max_links: usize,
    /// Most open connections.
    pub max_connections: usize,
    /// Most people in one file at once.
    pub max_people: usize,
    /// Compact a file's log into a new snapshot after this many entries…
    pub compact_after: usize,
    /// …or once the log is this large (MiB).
    pub compact_mb: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: format!("127.0.0.1:{DEFAULT_PORT}"),
            data: PathBuf::from("figcraft-data"),
            public_url: None,
            max_files: 1000,
            max_file_mb: 64,
            max_links: 100,
            max_connections: 512,
            max_people: 256,
            compact_after: 1000,
            compact_mb: 16,
        }
    }
}

impl Config {
    /// Read a TOML file's settings (unknown keys are errors).
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The listen address.
    pub fn addr(&self) -> Result<SocketAddr, String> {
        self.listen.trim().parse().map_err(|_| format!("`listen` must be an address with a port, like 0.0.0.0:{DEFAULT_PORT}"))
    }

    /// The public URL, when set: a `ws://` or `wss://` address without a query.
    pub fn public(&self) -> Result<Option<Endpoint>, String> {
        let Some(url) = self.public_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) else { return Ok(None) };
        let (e, query) = Endpoint::parse(url).map_err(|e| format!("`public_url`: {e}"))?;
        if !query.is_empty() {
            return Err("`public_url` can't have a query (no keys or tokens in it)".into());
        }
        Ok(Some(e))
    }

    /// Check everything (before starting).
    pub fn check(&self) -> Result<(), String> {
        self.addr()?;
        self.public()?;
        for (name, v) in [
            ("max_files", self.max_files as u64),
            ("max_file_mb", self.max_file_mb),
            ("max_links", self.max_links as u64),
            ("max_connections", self.max_connections as u64),
            ("max_people", self.max_people as u64),
            ("compact_after", self.compact_after as u64),
            ("compact_mb", self.compact_mb),
        ] {
            if v == 0 {
                return Err(format!("`{name}` must be at least 1"));
            }
        }
        if self.max_file_mb > 4096 || self.compact_mb > 4096 {
            return Err("sizes are in MiB and at most 4096".into());
        }
        Ok(())
    }

    /// The store's part.
    pub fn store(&self) -> StoreConfig {
        let mib = |n: u64| n.saturating_mul(1 << 20);
        StoreConfig {
            max_files: self.max_files,
            max_file_bytes: mib(self.max_file_mb),
            max_links: self.max_links,
            compact_after: self.compact_after,
            compact_bytes: mib(self.compact_mb),
            core: figcraft_collab::Limits { max_peers: self.max_people, ..figcraft_collab::Limits::default() },
        }
    }

    /// The transport's part.
    pub fn net(&self) -> NetLimits {
        NetLimits { max_connections: self.max_connections, ..NetLimits::default() }
    }
}
