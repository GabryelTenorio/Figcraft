//! Transport limits, shared by the native transport and the browser client.

use std::time::Duration;

/// Transport limits (the collaboration core has its own: [`figcraft_collab::Limits`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetLimits {
    /// Sockets a host keeps open at once, joined or not.
    pub max_connections: usize,
    /// Largest message a client may send once it has joined (a batch can carry images), and
    /// before: its hello.
    pub max_client_message: usize,
    pub max_hello: usize,
    /// Largest message a client accepts from the host (a welcome carries the whole document).
    pub max_server_message: usize,
    /// Messages a client may send per second on average, and in a burst; beyond that it is
    /// disconnected.
    pub rate: f64,
    pub burst: f64,
    /// A new connection must finish the WebSocket handshake and say hello within this.
    pub hello_timeout: Duration,
    /// A connection with no traffic for this long is dead.
    pub idle_timeout: Duration,
    /// A quiet client pings this often (keeps it under `idle_timeout`).
    pub keepalive: Duration,
    /// Messages queued for one client before it counts as stuck and is disconnected.
    pub queue: usize,
    /// How long a write may block.
    pub write_timeout: Duration,
}

impl Default for NetLimits {
    fn default() -> Self {
        Self {
            max_connections: 64,
            max_client_message: figcraft_collab::Limits::default().max_message_bytes,
            max_hello: 64 << 10,
            max_server_message: figcraft_collab::Limits::default().max_message_bytes,
            rate: 100.0,
            burst: 400.0,
            hello_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(45),
            keepalive: Duration::from_secs(15),
            queue: 1024,
            write_timeout: Duration::from_secs(10),
        }
    }
}
