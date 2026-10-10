//! Messages between clients and the session server, and their two encodings: a compact binary form
//! (a version byte, then postcard) and a JSON form for debugging and agents.

use figcraft_doc::NodeId;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::change::Change;

/// The protocol version a client announces in [`Hello`].
pub const PROTOCOL: u32 = 1;

/// The first byte of every binary message.
pub const WIRE_VERSION: u8 = 1;

/// Join a file's session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    /// The file's session id (from the invite).
    pub file: String,
    /// The session's secret (from the invite; empty when the session has none).
    pub token: String,
    /// The name shown on this person's cursor.
    pub name: String,
    /// Their colour (`#RRGGBB`).
    pub color: String,
    pub client_version: String,
}

/// Someone in the session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Peer {
    pub client_id: u32,
    pub name: String,
    pub color: String,
}

/// Where someone is and what they look at; ephemeral (never stored), latest wins.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    /// The page the pointer is on and its position in page coordinates.
    pub cursor: Option<(NodeId, f64, f64)>,
    pub selection: Option<Vec<NodeId>>,
    /// The page shown and the visible rectangle `(x, y, width, height)` in page coordinates.
    pub viewport: Option<(NodeId, f64, f64, f64, f64)>,
    /// The text layer being typed in.
    pub editing_text: Option<NodeId>,
}

/// Client → server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientMsg {
    /// Join (the first message on a connection).
    Hello(Hello),
    /// One user action's changes, made on top of server state `base_seq`.
    Changes {
        base_seq: u64,
        batch_id: u64,
        changes: Vec<Change>,
    },
    Presence(Presence),
    /// The highest sequence number applied.
    Ack {
        seq: u64,
    },
    Ping,
}

/// Server → client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerMsg {
    /// Joined: this connection's client id, the current sequence number, the document as a
    /// snapshot ([`Replica::snapshot`](crate::Replica::snapshot)) and who else is here.
    Welcome {
        client_id: u32,
        seq: u64,
        snapshot: Vec<Change>,
        peers: Vec<Peer>,
    },
    /// Accepted changes, in order, to everyone (the author's own come back as confirmation).
    Changes {
        seq: u64,
        author: u32,
        batch_id: u64,
        changes: Vec<Change>,
    },
    /// A batch was refused (nothing of it applies): why, and the current values of what it
    /// touched.
    Reject {
        batch_id: u64,
        reason: String,
        fixes: Vec<Change>,
    },
    /// Someone's presence.
    Presence {
        client_id: u32,
        presence: Presence,
    },
    PeerJoined(Peer),
    PeerLeft {
        client_id: u32,
    },
    /// The connection is refused (bad protocol version or token, session full, a message out of
    /// place).
    Refused {
        reason: String,
    },
    Pong,
}

/// The binary form of `msg`: [`WIRE_VERSION`], then postcard.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, Error> {
    let body = postcard::to_allocvec(msg).map_err(|e| Error::Encode(e.to_string()))?;
    let mut out = Vec::with_capacity(body.len() + 1);
    out.push(WIRE_VERSION);
    out.extend(body);
    Ok(out)
}

/// Read a binary message of at most `max_bytes` (untrusted input: never panics, allocates at most
/// about the message's size, and refuses trailing bytes).
pub fn decode<T: DeserializeOwned>(bytes: &[u8], max_bytes: usize) -> Result<T, Error> {
    if bytes.len() > max_bytes {
        return Err(Error::Decode(format!("message of {} bytes is over the {max_bytes}-byte limit", bytes.len())));
    }
    let Some((&version, body)) = bytes.split_first() else { return Err(Error::Decode("empty message".into())) };
    if version != WIRE_VERSION {
        return Err(Error::Decode(format!("wire version {version}, expected {WIRE_VERSION}")));
    }
    let (msg, rest) = postcard::take_from_bytes::<T>(body).map_err(|e| Error::Decode(e.to_string()))?;
    if !rest.is_empty() {
        return Err(Error::Decode(format!("{} bytes after the message", rest.len())));
    }
    Ok(msg)
}

/// The JSON form of `msg` (`{"<type>": {…}}`).
pub fn encode_json<T: Serialize>(msg: &T) -> Result<String, Error> {
    serde_json::to_string(msg).map_err(|e| Error::Encode(e.to_string()))
}

/// Read a JSON message of at most `max_bytes`.
pub fn decode_json<T: DeserializeOwned>(text: &str, max_bytes: usize) -> Result<T, Error> {
    if text.len() > max_bytes {
        return Err(Error::Decode(format!("message of {} bytes is over the {max_bytes}-byte limit", text.len())));
    }
    serde_json::from_str(text).map_err(|e| Error::Decode(e.to_string()))
}
