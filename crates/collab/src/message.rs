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
    /// Coming back after a dropped connection: who this client was and the last sequence number
    /// it applied (`None`: a new participant).
    pub resume: Option<Resume>,
}

/// What a reconnecting client tells the server in its [`Hello`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resume {
    /// The session it was in ([`ServerMsg::Welcome`]'s `session`).
    pub session: u64,
    /// The client id the server gave this client before.
    pub client_id: u32,
    /// The last sequence number it applied.
    pub seq: u64,
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
    /// The text layer being typed in, with the caret and selection.
    pub editing_text: Option<Box<EditingText>>,
}

/// Someone typing in a text layer: where their caret and selection are (in characters).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditingText {
    pub node: NodeId,
    pub caret: usize,
    /// The selected characters `(start, end)`, if any.
    pub selection: Option<(usize, usize)>,
}

/// A batch the server accepted: its sequence number, who sent it and their batch id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Accepted {
    pub seq: u64,
    pub author: u32,
    pub batch_id: u64,
    pub changes: Vec<Change>,
}

/// What the server did with one client's batches: the last batch id it processed (accepted or
/// refused; 0: none) and the ids of the latest refused ones (at most [`MAX_REFUSED`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Processed {
    pub last_batch: u64,
    pub refused: Vec<u64>,
}

/// The most refused batch ids [`Processed`] remembers per client.
pub const MAX_REFUSED: usize = 1024;

impl Processed {
    /// Note batch `batch_id` as processed (`refused`: not accepted).
    pub fn note(&mut self, batch_id: u64, refused: bool) {
        self.last_batch = self.last_batch.max(batch_id);
        if refused {
            self.refused.push(batch_id);
            let over = self.refused.len().saturating_sub(MAX_REFUSED);
            self.refused.drain(..over);
        }
    }
}

/// How a [`ServerMsg::Welcome`] brings a client up to date.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CatchUp {
    /// The whole document ([`Replica::snapshot`](crate::Replica::snapshot)): a new participant,
    /// or one whose missed batches are no longer in the server's log.
    Snapshot(Vec<Change>),
    /// The batches a reconnecting client missed, in order.
    Delta(Vec<Accepted>),
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
    /// Joined: the session's identity (a server that restarted without its saved state is a new
    /// session), this connection's client id, the current sequence number, the document (or the
    /// batches missed since a reconnecting client's last sequence number), what the server did
    /// with this client's batches so far and who else is here.
    Welcome {
        session: u64,
        client_id: u32,
        seq: u64,
        catch_up: CatchUp,
        processed: Processed,
        peers: Vec<Peer>,
    },
    /// Accepted changes, in order, to everyone (the author's own come back as confirmation).
    Changes(Accepted),
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
