//! Managing a server's files from a client: one connection that opens with the server's key
//! ([`Manage`]) and sends requests, each answered in order. The app publishes through it, and the
//! server's command line uses it to reach a running server.

use std::time::Duration;

use figcraft_collab::message::{self, Admin, AdminReply, ClientMsg, Manage, PROTOCOL, ServerMsg};
use tungstenite::{Bytes, Message, WebSocket};

use crate::invite::Endpoint;
use crate::net::{NetLimits, config, dial};

/// Why the desktop app can't reach a `wss://` address (it has no TLS of its own yet).
pub const NO_TLS: &str =
    "The desktop app can't open secure (wss://) connections yet: use the server's ws:// address, or join from FigCraft in the browser";

/// Send `requests` to the server at `server` with its `key`; the replies, in order. Fails when the
/// server can't be reached, refuses the key or closes the connection (a request it can't do is a
/// [`AdminReply::Failed`] reply, not an error). Never echoes the key.
pub fn request(server: &Endpoint, key: &str, requests: Vec<Admin>, limits: &NetLimits, timeout: Duration) -> Result<Vec<AdminReply>, String> {
    if server.secure {
        return Err(NO_TLS.into());
    }
    let stream = dial(&server.host, timeout, limits)?;
    let (mut ws, _) = tungstenite::client::client_with_config(server.url(), stream, Some(config(limits.max_server_message)))
        .map_err(|e| format!("{} doesn't answer as a FigCraft server: {e}", server.display()))?;
    // A large upload takes the server a while to keep.
    ws.get_ref().set_read_timeout(Some(limits.idle_timeout)).map_err(|e| e.to_string())?;
    let manage = Manage { protocol: PROTOCOL, key: key.to_string(), client_version: env!("CARGO_PKG_VERSION").to_string() };
    send(&mut ws, &ClientMsg::Manage(manage))?;
    match reply(&mut ws, limits)? {
        AdminReply::Ready => {}
        other => return Err(format!("the server answered {other:?} instead of letting this connection manage files")),
    }
    let mut replies = Vec::with_capacity(requests.len());
    for r in requests {
        send(&mut ws, &ClientMsg::Admin(r))?;
        replies.push(reply(&mut ws, limits)?);
    }
    let _ = ws.close(None);
    // Read until the server's close arrives (or the socket ends), so the close isn't lost.
    while ws.read().is_ok() {}
    Ok(replies)
}

fn send(ws: &mut WebSocket<std::net::TcpStream>, msg: &ClientMsg) -> Result<(), String> {
    let bytes = message::encode(msg).map_err(|e| e.to_string())?;
    ws.send(Message::Binary(Bytes::from(bytes))).map_err(|e| format!("the connection to the server dropped ({e})"))
}

/// The next answer: an admin reply, or why there is none.
fn reply(ws: &mut WebSocket<std::net::TcpStream>, limits: &NetLimits) -> Result<AdminReply, String> {
    loop {
        return match ws.read() {
            Ok(Message::Binary(b)) => match message::decode::<ServerMsg>(&b, limits.max_server_message) {
                Ok(ServerMsg::Admin(r)) => Ok(*r),
                Ok(ServerMsg::Refused { reason }) => Err(format!("the server refused: {reason}")),
                Ok(ServerMsg::Pong) => continue,
                Ok(_) => Err("the server sent something other than an answer".into()),
                Err(e) => Err(format!("the server sent a message this version can't read ({e})")),
            },
            Ok(Message::Close(f)) => Err(match f.map(|f| f.reason.to_string()).filter(|r| !r.is_empty()) {
                Some(r) => format!("the server closed the connection: {r}"),
                None => "the server closed the connection".into(),
            }),
            Ok(Message::Text(_)) => Err("the server sent a text message".into()),
            Ok(_) => continue,
            Err(e) => Err(format!("no answer from the server ({e})")),
        };
    }
}
