//! Invite links: `figcraft://join?host=<ip:port>&file=<id>&token=<secret>`, or the same session as a
//! plain WebSocket URL `ws://<ip:port>/?file=<id>&token=<secret>`.
//!
//! The token never travels in the WebSocket request (a proxy's logs would keep it): the client
//! connects to `ws://<host>/` and sends the token in its `Hello`. Errors never echo the link.

/// What an invite link names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    /// `host:port` (an IP address, `[v6]:port`, or a host name).
    pub host: String,
    /// The shared file's session id.
    pub file: String,
    /// The session's secret.
    pub token: String,
}

/// Longest link accepted (anything longer isn't one of ours).
const MAX_LINK: usize = 2048;

impl Invite {
    /// The `figcraft://join?…` link.
    pub fn link(&self) -> String {
        format!("figcraft://join?host={}&file={}&token={}", self.host, self.file, self.token)
    }

    /// The plain WebSocket URL of the same session.
    pub fn url(&self) -> String {
        format!("ws://{}/?file={}&token={}", self.host, self.file, self.token)
    }

    /// What the client connects to (no secrets in it).
    pub fn endpoint(&self) -> String {
        format!("ws://{}/", self.host)
    }

    /// The host is this machine (a loopback address or `localhost`).
    pub fn loopback(&self) -> bool {
        match self.host.parse::<std::net::SocketAddr>() {
            Ok(a) => a.ip().is_loopback(),
            Err(_) => self.host.rsplit_once(':').is_some_and(|(name, _)| name.eq_ignore_ascii_case("localhost")),
        }
    }

    /// Read an invite link (either form; surrounding spaces, quotes or angle brackets are fine).
    pub fn parse(link: &str) -> Result<Invite, String> {
        const NOT: &str = "that's not a live session invite: expected figcraft://join?host=…&file=…&token=… or ws://host:port/?file=…&token=…";
        let link = link.trim().trim_matches(|c| matches!(c, '<' | '>' | '"' | '\''));
        if link.len() > MAX_LINK {
            return Err(NOT.into());
        }
        let (scheme, rest) = link.split_once("://").ok_or(NOT)?;
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let mut host = None;
        let mut file = None;
        let mut token = None;
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = percent_decode(v).ok_or(NOT)?;
            match k {
                "host" => host = Some(v),
                "file" => file = Some(v),
                "token" => token = Some(v),
                _ => {}
            }
        }
        if scheme.eq_ignore_ascii_case("figcraft") {
            if !path.trim_end_matches('/').eq_ignore_ascii_case("join") {
                return Err(NOT.into());
            }
        } else if scheme.eq_ignore_ascii_case("ws") {
            // ws://host:port/… : the host is the authority.
            host = Some(path.split('/').next().unwrap_or("").to_string());
        } else if scheme.eq_ignore_ascii_case("wss") {
            return Err("secure (wss://) live sessions need a server behind TLS, which comes with the standalone server".into());
        } else {
            return Err(NOT.into());
        }
        let host = host.filter(|h| valid_host(h)).ok_or("the invite link's host isn't an address with a port (like 192.168.1.20:47900)")?;
        let file = file.filter(|f| valid_id(f, 1, 64)).ok_or("the invite link has no valid file id")?;
        let token = token.filter(|t| valid_id(t, 16, 128)).ok_or("the invite link has no valid token")?;
        Ok(Invite { host, file, token })
    }
}

/// Ids and tokens are URL-safe base64 / hex characters.
fn valid_id(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `ip:port`, `[v6]:port` or `name:port` with a non-zero port.
fn valid_host(s: &str) -> bool {
    if s.parse::<std::net::SocketAddr>().is_ok_and(|a| a.port() != 0) {
        return true;
    }
    let Some((name, port)) = s.rsplit_once(':') else { return false };
    port.parse::<u16>().is_ok_and(|p| p != 0)
        && (1..=253).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        && !name.starts_with(['-', '.'])
}

/// `%XX` escapes decoded (UTF-8); `None` for broken escapes.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(if b == b'+' { b' ' } else { b });
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
