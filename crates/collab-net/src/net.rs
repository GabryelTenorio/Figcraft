//! The WebSocket plumbing both ends share: one connection read by one thread and written by
//! another, limits, and rate limiting.
//!
//! Each connection has a reader thread blocked in `read` and a writer thread blocked on its
//! queue, so an idle session costs no CPU. Both sides of a connection are tungstenite state
//! machines over the same socket; every write goes through one lock and writes whole frames, so
//! frames from the two never interleave.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;

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

/// tungstenite's settings for messages up to `max` bytes.
pub fn config(max: usize) -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(max)).max_frame_size(Some(max))
}

/// One side (read or write) of a connection: reads from its own handle, writes whole buffers
/// under the connection's write lock.
pub struct Duplex {
    read: TcpStream,
    write: Arc<Mutex<TcpStream>>,
}

impl Duplex {
    /// Two handles on `stream` (for the reader and the writer) and one to shut it down with.
    pub fn split(stream: TcpStream) -> io::Result<(Duplex, Duplex, TcpStream)> {
        let write = Arc::new(Mutex::new(stream.try_clone()?));
        let a = Duplex { read: stream.try_clone()?, write: write.clone() };
        let b = Duplex { read: stream.try_clone()?, write };
        Ok((a, b, stream))
    }
}

impl Duplex {
    /// How long a read on this handle may block (socket options are per handle: set them on the
    /// one that reads).
    pub fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        self.read.set_read_timeout(d)
    }
}

impl Read for Duplex {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read.read(buf)
    }
}

impl Write for Duplex {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write.lock().unwrap_or_else(PoisonError::into_inner).write_all(buf)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write.lock().unwrap_or_else(PoisonError::into_inner).flush()
    }
}

/// Close the socket both ways (wakes a reader blocked on it).
pub fn shutdown(s: &TcpStream) {
    let _ = s.shutdown(Shutdown::Both);
}

/// A token bucket: `rate` per second on average, `burst` at once.
#[derive(Debug)]
pub struct Bucket {
    rate: f64,
    burst: f64,
    tokens: f64,
    at: Instant,
}

impl Bucket {
    pub fn new(rate: f64, burst: f64) -> Self {
        Self { rate, burst, tokens: burst, at: Instant::now() }
    }

    /// Spend one token at `now`; false when there are none left.
    pub fn take(&mut self, now: Instant) -> bool {
        let dt = now.saturating_duration_since(self.at).as_secs_f64();
        self.at = now;
        self.tokens = (self.tokens + dt * self.rate).min(self.burst);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}
