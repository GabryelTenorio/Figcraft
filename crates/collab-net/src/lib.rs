//! Live sessions over the network for FigCraft (milestone M9, step 2 of
//! `docs/collab-protocol.md`): the WebSocket transport, the session host the desktop app embeds,
//! invite links, and presence relay. It carries the collaboration core's messages
//! ([`figcraft_collab::message`]) and runs its [`Server`](figcraft_collab::Server) as the host's
//! authority.
//!
//! The app installs a [`NetLink`] into the engine session (`session.live.install(…)`); the
//! engine's `collab.*` commands drive it and the UI reads the engine's live state. Network
//! threads wake the UI through a [`Waker`] only when something arrives; nothing polls.
//!
//! The browser build has a stand-in [`NetLink`] whose host and join say it's not available yet
//! (a `web-sys` WebSocket client is step 4).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod inbox;
pub mod invite;

#[cfg(not(target_arch = "wasm32"))]
pub mod guest;
#[cfg(not(target_arch = "wasm32"))]
pub mod host;
#[cfg(not(target_arch = "wasm32"))]
mod link;
#[cfg(not(target_arch = "wasm32"))]
pub mod net;
#[cfg(not(target_arch = "wasm32"))]
pub mod throttle;
#[cfg(not(target_arch = "wasm32"))]
pub mod token;

#[cfg(target_arch = "wasm32")]
mod web;

pub use inbox::{Inbox, Waker};
pub use invite::Invite;
#[cfg(not(target_arch = "wasm32"))]
pub use link::{DEFAULT_PORT, NetLink, lan_ip};
#[cfg(target_arch = "wasm32")]
pub use web::NetLink;

#[cfg(test)]
mod invite_tests;
#[cfg(test)]
mod loopback_tests;
#[cfg(test)]
mod session_tests;
