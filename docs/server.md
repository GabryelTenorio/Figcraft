# figcraft-server

`figcraft-server` keeps FigCraft files for a team and serves their live sessions. A file published
to it stays there when everyone has left; people open it with a **share link** that lets them
**edit** it or only **view** it. Edits are kept as a snapshot plus a log of what changed, so a
restart (or a crash) loses nothing that was confirmed, and apps reconnect where they were. It is
headless, native only (no web build), and runs on the same platforms as the desktop app.

There are no user accounts yet: a share link is the access to one file, and the **server key**
manages files (publishing, links, removal). Accounts and per-person permissions come later.

The protocol and the design behind it are in [`collab-protocol.md`](collab-protocol.md).

## Running it

```sh
cargo build --release -p figcraft-server
target/release/figcraft-server --listen 0.0.0.0:47900 --data /srv/figcraft
```

- `--listen` defaults to `127.0.0.1:47900`: only this machine can connect. `0.0.0.0:47900` (or an
  interface's address) lets the network in; put it behind a TLS reverse proxy before you expose
  it beyond a trusted network (see below).
- `--data` is the data directory (default `./figcraft-data`), made if missing. One server at a
  time uses a directory: a second one refuses to start.
- Logs go to standard output (`RUST_LOG=debug` for more). They name files, link ids and addresses,
  never keys or tokens.
- Stop it like any service (Ctrl+C, `systemctl stop`, `kill`). Every change it accepted is on disk
  before anyone is told about it, so nothing confirmed is lost; a file's log is folded into a new
  snapshot the next time everyone leaves it. Apps that were in a file keep editing and reconnect
  once the server is back (for about a minute and a half), as the same participants.

## Publishing a file

```sh
figcraft-server --data /srv/figcraft key
```

prints the address to publish to, `ws://<host>:47900/?key=<server key>`. In the desktop app,
**Share ▸ Publish to server…** (or **File ▸ Publish to server…**) asks for that address and the
file's name. The open file is uploaded as a new file and the tab keeps syncing through the
server; until the upload is done the tab is read-only ("Publishing…" in the toolbar). The Share
popover then shows an **edit link** and a **view link** to copy. Agents do the same with
`collab.publish {server, name?}` ([`mcp.md`](mcp.md#live-sessions)).

Whoever has the server key can publish files and manage every file's links: give it only to
people you trust.

## Joining

Share links look like the desktop app's invite links and join the same way (**File ▸ Join live
session…**, opening the link, or `collab.join {link}`):

- `figcraft://join?host=<host:port>&file=<id>&token=<token>` for a plain `ws://` server;
- `figcraft://join?server=wss://figcraft.example.com/&file=<id>&token=<token>` for a server behind
  a TLS reverse proxy or under a path;
- `ws://<host:port>/?file=<id>&token=<token>` or `wss://…` (the same link as a WebSocket URL).

The file opens as a new tab. With a **view link** you see everyone's changes and presence, and
others see your cursor and selection, but you can't change anything: the toolbar shows a **View
only** badge with only the Move and Hand tools, edits are refused with a message (and the server
refuses them too). **Leave session** keeps the file open as your own copy, editable again.

## Managing files and links

| Command | What it does |
|---|---|
| `figcraft-server key [--rotate]` | The address to publish to, with the key; `--rotate` makes a new key (the server must be stopped) |
| `figcraft-server files [list]` | The files: id, name, people in it now, last change, size |
| `figcraft-server files remove FILE` | Remove a file and its links for good (whoever is in it is disconnected) |
| `figcraft-server links list FILE` | A file's links: id, access, created, label |
| `figcraft-server links create FILE --access edit\|view [--label TEXT]` | A new link, printed once |
| `figcraft-server links revoke FILE LINK` | Revoke a link: it no longer joins, and whoever joined with it is disconnected |

Each takes `--data` (or `--config`). When a server is running on the directory, the command asks
it over the same port with the server key (so a revoked link's users are disconnected at once);
otherwise it changes the directory itself. A new link is shown only when it is made: the server
keeps only a hash of its token, so a lost link can't be recovered (revoke it and make another).
Publishing makes an edit link and a view link labelled `Published (edit)` and `Published (view)`.

## Configuration

Every flag is also a key of a TOML file given with `--config server.toml` (flags win; unknown keys
are errors):

| Key | Default | Meaning |
|---|---|---|
| `listen` | `127.0.0.1:47900` | Where to listen |
| `data` | `figcraft-data` | The data directory |
| `public_url` | (the listen address) | How links name the server, when people reach it another way (a reverse proxy: `wss://figcraft.example.com/`) |
| `max_files` | 1000 | Most files kept |
| `max_file_mb` | 64 | Largest file (its snapshot), in MiB; a welcome carries the whole file, and messages are at most 64 MiB |
| `max_links` | 100 | Most share links per file |
| `max_connections` | 512 | Most open connections |
| `max_people` | 256 | Most people in one file at once |
| `compact_after` | 1000 | Compact a file's log into a new snapshot after this many entries… |
| `compact_mb` | 16 | …or once its log is this large (MiB) |

With `listen` on every interface (`0.0.0.0`) and no `public_url`, links name this machine's
network address.

## The data directory

```text
<data>/LOCK                    held while the server (or a command) uses the directory
<data>/server.json             where the running server listens (read by the commands)
<data>/server.key              the server key
<data>/files/<id>/meta.json    the file's name, when it was created and last changed
<data>/files/<id>/links.json   its links: id, access, label, created, SHA-256 of the token
<data>/files/<id>/snapshot.bin its state at the last compaction (with a checksum)
<data>/files/<id>/log.bin      what changed since, one checksummed record per change
```

- **Writes are atomic**: a snapshot, a description or a link list is written to a temporary file
  beside it, flushed, then renamed over the old one. The log is only ever appended to, and synced
  before the server tells anyone about the change.
- **Compaction** writes the file's state as the next snapshot and starts an empty log, after
  `compact_after` entries or `compact_mb` of log, when the last person leaves, and when the server
  stops. A log left over from a compaction cut short is recognised and ignored (the snapshot holds
  it all).
- **Damage is skipped and reported, never deleted.** At startup a folder whose description, links
  or snapshot can't be read is left out and logged; a log that ends in a cut-short or damaged
  record loses only that end (logged when the file opens). Leftover temporary files are removed.
- The server key is stored as plain text (on Unix readable by the owner only): whoever can read
  the data directory can read every file anyway. Link tokens are stored only as hashes.

## Backups

Copy the data directory while the server is stopped, or take an atomic snapshot of the
filesystem it lives on (ZFS, Btrfs, LVM, a VSS shadow copy). A plain copy of a running server's
directory can catch a file between its snapshot and its log; it restores, but may lose that
file's changes since its last compaction. To restore, stop the server, put the directory back and
start it: apps that were in a file catch up with a fresh copy (their unsent edits are kept when
the server still knows them, else they are told what couldn't be kept).

## Behind a reverse proxy (TLS)

The server speaks plain WebSocket (`ws://`); TLS comes from a reverse proxy in front of it, which
gives browsers the `wss://` they need on an `https://` page.

- Run the server on the loopback interface (`listen = "127.0.0.1:47900"`), and set
  `public_url` to the address people use, for example `wss://figcraft.example.com/`. Links and
  the publish address then name the proxy.
- **Caddy**: a site block for your host name whose only directive is a `reverse_proxy` to
  `127.0.0.1:47900`. Caddy obtains the certificate and passes WebSocket upgrades through on its
  own.
- **nginx**: a `server` block listening on 443 with your certificate, and a `location` that
  passes requests to `http://127.0.0.1:47900` over HTTP/1.1 with the `Upgrade` and `Connection`
  request headers forwarded (that is what lets the WebSocket upgrade through). Raise the proxy's
  read timeout above the server's 45-second idle limit or leave its default (60 s): apps ping
  every 15 seconds.
- **Under a path**: the server answers on any path, so the proxy can forward a prefix such as
  `/figcraft/` unchanged; set `public_url = "wss://example.com/figcraft/"`.
- Tokens never travel in the WebSocket request (the client sends them inside its first message),
  so the proxy's access logs don't record them.

The desktop app has no TLS of its own yet: it can't open `wss://` links. Inside a trusted network
or a VPN, let it reach the server's `ws://` address directly (keep the port closed to the
internet), and use `wss://` links from the browser build.

## Security

- **Access** is a share link (128-bit random token) per file and access level, or the server key
  (128-bit) for managing files. Tokens are stored as SHA-256 hashes and every comparison runs in
  constant time; revoking a link disconnects whoever joined with it. A view link's batches are
  refused by the server whatever the client does.
- **Transport**: plain `ws://` on the server's port; anyone on the network path can read traffic
  (presence, changes, tokens) unless TLS terminates in front of it. Use a reverse proxy for
  anything beyond a trusted network.
- **Limits**: the transport's (message sizes, 100 messages a second per connection, idle and
  handshake timeouts, `max_connections`) and the collaboration core's (layers, changes per batch,
  `max_people`), plus `max_files`, `max_file_mb` and `max_links`. Every message is decoded
  defensively, and fuzzed in the tests.
- **Not there yet**: user accounts, per-person permissions, rate limits per address, audit logs.
