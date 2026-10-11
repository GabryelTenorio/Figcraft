//! Requests to a server outside a live session, each on a connection of its own answered in
//! order: managing its files with the server's key ([`Manage`], [`request`]; the app publishes
//! through it, and the server's command line uses it to reach a running server), and using a
//! file's team library with one of the file's share links ([`OpenLibrary`], [`library`]), and its
//! plugin directory ([`OpenPlugins`], [`plugins`]: anyone browses, the key publishes).

use std::time::Duration;

use figcraft_collab::message::{
    self, Admin, AdminReply, ClientMsg, LibraryReply, LibraryReq, Manage, OpenLibrary, OpenPlugins, PROTOCOL, PluginReply, PluginReq, ServerMsg,
};
use tungstenite::{Bytes, Message, WebSocket};

use crate::invite::{Endpoint, Invite};
use crate::net::{Duplex, NetLimits, config, open};

type Ws = WebSocket<Duplex>;

/// Send `requests` to the server at `server` with its `key`; the replies, in order. Fails when the
/// server can't be reached, refuses the key or closes the connection (a request it can't do is a
/// [`AdminReply::Failed`] reply, not an error). Never echoes the key.
pub fn request(server: &Endpoint, key: &str, requests: Vec<Admin>, limits: &NetLimits, timeout: Duration) -> Result<Vec<AdminReply>, String> {
    let admin = |m: ServerMsg| match m {
        ServerMsg::Admin(r) => Some(*r),
        _ => None,
    };
    let manage = Manage { protocol: PROTOCOL, key: key.to_string(), client_version: env!("CARGO_PKG_VERSION").to_string() };
    let mut ws = connect(server, limits, timeout)?;
    send(&mut ws, &ClientMsg::Manage(manage))?;
    match reply(&mut ws, limits, admin)? {
        AdminReply::Ready => {}
        other => return Err(format!("the server answered {other:?} instead of letting this connection manage files")),
    }
    let mut replies = Vec::with_capacity(requests.len());
    for r in requests {
        send(&mut ws, &ClientMsg::Admin(r))?;
        replies.push(reply(&mut ws, limits, admin)?);
    }
    close(ws);
    Ok(replies)
}

/// Send one library request about the file behind share link `link`; the reply. Fails when the
/// link isn't one, the server can't be reached or refuses the link (a request it can't do is a
/// [`LibraryReply::Failed`] reply). Never echoes the link.
pub fn library(link: &str, req: LibraryReq, limits: &NetLimits, timeout: Duration) -> Result<LibraryReply, String> {
    let lib = |m: ServerMsg| match m {
        ServerMsg::Library(r) => Some(*r),
        _ => None,
    };
    let invite = Invite::parse(link)?;
    let open = OpenLibrary { protocol: PROTOCOL, file: invite.file, token: invite.token, client_version: env!("CARGO_PKG_VERSION").to_string() };
    let mut ws = connect(&invite.server, limits, timeout)?;
    send(&mut ws, &ClientMsg::OpenLibrary(open))?;
    match reply(&mut ws, limits, lib)? {
        LibraryReply::Ready { .. } => {}
        other => return Err(format!("the server answered {other:?} instead of opening the library")),
    }
    send(&mut ws, &ClientMsg::Library(req))?;
    let r = reply(&mut ws, limits, lib)?;
    close(ws);
    Ok(r)
}

/// Send one request to the plugin directory of the server at `server`, with its `key` (empty: only
/// browsing); the reply. Fails when the server can't be reached, refuses the key or has no
/// directory (an older server); a request it can't do is a [`PluginReply::Failed`] reply. Never
/// echoes the key.
pub fn plugins(server: &Endpoint, key: &str, req: PluginReq, limits: &NetLimits, timeout: Duration) -> Result<PluginReply, String> {
    let pick = |m: ServerMsg| match m {
        ServerMsg::Plugins(r) => Some(*r),
        _ => None,
    };
    let open = OpenPlugins { protocol: PROTOCOL, key: key.to_string(), client_version: env!("CARGO_PKG_VERSION").to_string() };
    let mut ws = connect(server, limits, timeout)?;
    send(&mut ws, &ClientMsg::OpenPlugins(open))?;
    match reply(&mut ws, limits, pick) {
        Ok(PluginReply::Ready { .. }) => {}
        // An older server can't read the request at all.
        Err(e) if e.contains("couldn't be read") => return Err(format!("{} has no plugin directory (update the server)", server.display())),
        Err(e) => return Err(e),
        Ok(other) => return Err(format!("the server answered {other:?} instead of opening its plugin directory")),
    }
    send(&mut ws, &ClientMsg::Plugins(req))?;
    let r = reply(&mut ws, limits, pick)?;
    close(ws);
    Ok(r)
}

/// A WebSocket to `server` that waits for answers up to the idle timeout (a large upload takes the
/// server a while to keep).
fn connect(server: &Endpoint, limits: &NetLimits, timeout: Duration) -> Result<Ws, String> {
    let (stream, _, _) = open(server, timeout, limits)?;
    let (ws, _) = tungstenite::client::client_with_config(server.url(), stream, Some(config(limits.max_server_message)))
        .map_err(|e| format!("{} doesn't answer as a FigCraft server: {e}", server.display()))?;
    ws.get_ref().set_read_timeout(Some(limits.idle_timeout)).map_err(|e| e.to_string())?;
    Ok(ws)
}

/// Close politely: read until the server's close arrives (or the socket ends), so it isn't lost.
fn close(mut ws: Ws) {
    let _ = ws.close(None);
    while ws.read().is_ok() {}
}

fn send(ws: &mut Ws, msg: &ClientMsg) -> Result<(), String> {
    let bytes = message::encode(msg).map_err(|e| e.to_string())?;
    ws.send(Message::Binary(Bytes::from(bytes))).map_err(|e| format!("the connection to the server dropped ({e})"))
}

/// The next answer `pick` takes, or why there is none.
fn reply<T>(ws: &mut Ws, limits: &NetLimits, pick: impl Fn(ServerMsg) -> Option<T>) -> Result<T, String> {
    loop {
        return match ws.read() {
            Ok(Message::Binary(b)) => match message::decode::<ServerMsg>(&b, limits.max_server_message) {
                Ok(ServerMsg::Refused { reason }) => Err(format!("the server refused: {reason}")),
                Ok(ServerMsg::Pong) => continue,
                Ok(m) => pick(m).ok_or_else(|| "the server sent something other than an answer".into()),
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
