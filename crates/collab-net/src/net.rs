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

pub use crate::limits::NetLimits;

use tungstenite::protocol::WebSocketConfig;

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

/// Connect to `host` (`name:port`), trying each of its addresses within `timeout`, with Nagle off
/// and reads and writes bounded by `limits`' hello and write timeouts.
pub fn dial(host: &str, timeout: Duration, limits: &NetLimits) -> Result<TcpStream, String> {
    use std::net::ToSocketAddrs;
    let addrs = host.to_socket_addrs().map_err(|e| format!("can't find {host}: {e}"))?;
    let mut last = format!("can't find {host}");
    for a in addrs {
        match TcpStream::connect_timeout(&a, timeout) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                s.set_read_timeout(Some(limits.hello_timeout)).map_err(|e| e.to_string())?;
                s.set_write_timeout(Some(limits.write_timeout)).map_err(|e| e.to_string())?;
                return Ok(s);
            }
            Err(e) => last = format!("no answer from {a}: {e}"),
        }
    }
    Err(last)
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
