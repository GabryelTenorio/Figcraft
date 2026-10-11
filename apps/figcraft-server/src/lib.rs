//! `figcraft-server`: FigCraft's standalone collaboration server (milestone M9, step 5 of
//! `docs/collab-protocol.md`; running it: `docs/server.md`).
//!
//! It keeps many files in a data directory ([`store`]), each with share links that let people
//! edit or only view, and serves their live sessions through the same hub the desktop app hosts
//! with ([`figcraft_collab_net::hub`]). Files persist as a snapshot plus an append-only log of what
//! changed ([`disk`]), compacted into a new snapshot from time to time, so a restart loses nothing
//! that was confirmed and clients resume where they were. There are no user accounts yet: share
//! links are the access, and the server key manages files.

pub mod cli;
pub mod config;
pub mod disk;
pub mod store;

use std::net::{SocketAddr, TcpListener};

use figcraft_collab_net::hub::Hosting;

use crate::config::Config;
use crate::store::Store;

/// A running server. Dropping it stops it: open files are saved, everyone is told the server is
/// restarting (their apps reconnect once it is back).
pub struct Running {
    hosting: Hosting,
}

impl Running {
    /// Where it listens.
    pub fn addr(&self) -> SocketAddr {
        self.hosting.addr()
    }

    /// Open connections.
    pub fn connections(&self) -> usize {
        self.hosting.connections()
    }

    /// Stop (see the type's docs).
    pub fn stop(&mut self) {
        self.hosting.stop();
    }
}

/// Start serving `cfg.data` on `cfg.listen`. Files that can't be read are skipped and logged.
pub fn start(cfg: &Config) -> Result<Running, String> {
    cfg.check()?;
    let store = Store::open(&cfg.data, cfg.store())?;
    for p in store.problems() {
        log::warn!("{p}");
    }
    let files = store.len();
    let addr = cfg.addr()?;
    let listener = TcpListener::bind(addr).map_err(|e| format!("can't listen on {addr}: {e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    cli::write_running(&cfg.data, addr)?;
    let public = cli::public_endpoint(cfg, addr)?;
    let hosting = Hosting::serve(listener, store, cfg.net()).map_err(|e| format!("can't start serving: {e}"))?;
    log::info!(
        "figcraft-server {}: {files} file{} in {}; listening on {addr}; links name {}",
        env!("CARGO_PKG_VERSION"),
        if files == 1 { "" } else { "s" },
        cfg.data.display(),
        public.url()
    );
    if addr.ip().is_loopback() {
        log::info!("only this machine can connect: listen on 0.0.0.0:{} (or behind a reverse proxy) to share", addr.port());
    }
    Ok(Running { hosting })
}

#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod server_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod test_util;
