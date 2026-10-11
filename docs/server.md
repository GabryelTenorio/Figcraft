# figcraft-server

`figcraft-server` keeps FigCraft files for a team and serves their live sessions. A file published
to it stays there when everyone has left; people open it with a **share link** that lets them
**edit** it or only **view** it. Edits are kept as a snapshot plus a log of what changed, so a
restart (or a crash) loses nothing that was confirmed, and apps reconnect where they were. It is
headless, native only (no web build), and runs on the same platforms as the desktop app.

There are no user accounts yet: a share link is the access to one file, and the **server key**
manages files (publishing, links, removal). Accounts and per-person permissions come later. A file
can also hold a **team library** that other files use through one of its links (see
[Team libraries](#team-libraries)), and the server can keep a **plugin directory** for the team (see
[Plugin directory](#plugin-directory)).

The protocol and the design behind it are in [`collab-protocol.md`](collab-protocol.md).

## Running it

```sh
cargo build --release -p figcraft-server
target/release/figcraft-server --listen 0.0.0.0:47900 --data /srv/figcraft
```

Releases also ship it ready-built for Linux (`figcraft-server-<version>-linux-x86_64.tar.gz` and
`-linux-aarch64`). To run it for others on a rented server, see
[Deploy on a Linux server](#deploy-on-a-linux-server).

- `--listen` defaults to `127.0.0.1:47900`: only this machine can connect. `0.0.0.0:47900` (or an
  interface's address) lets the network in; put it behind a TLS reverse proxy before you expose
  it beyond a trusted network (see below).
- `--data` is the data directory (default `./figcraft-data`), made if missing. One server at a
  time uses a directory: a second one refuses to start.
- Logs go to standard output (`RUST_LOG=debug` for more). They name files, link ids and addresses,
  never keys or tokens.
- Stop it like any service (Ctrl+C, `systemctl stop`, `docker stop`, `kill`): on SIGTERM, SIGINT
  or SIGHUP (Ctrl+C or closing the console on Windows) it saves and compacts the open files and
  tells everyone it is restarting. Every change it accepted is on disk before anyone is told about
  it, so even a hard kill loses nothing confirmed (a file's log is then folded into a new snapshot
  the next time everyone leaves it). Apps that were in a file keep editing and reconnect once the
  server is back (for about a minute and a half), as the same participants.

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

## Team libraries

A file on the server can hold a **team library**: its published components, styles and variables,
which other files use without anyone copying the library file around.

- **Publishing.** In the library file, opened from the server with an edit link (or published to
  it), **Assets ▸ Libraries ▸ Publish to server** publishes the chosen assets as the library's next
  version (`library.publishToServer`; with the server's address and key a file not on the server
  yet is kept there first). The server keeps the latest version of each file's library. Each asset
  keeps the version it last changed in, so files using the library update only what changed; the
  server's version is the one that counts, so when someone published since, the next publish
  follows it. Publishing needs an edit link: a view link's publish is refused.
- **Using.** In another file, paste one of the library file's links (a **view link** is enough)
  into **Assets ▸ Libraries** (`library.add {path: <link>}`). The library is listed in the Assets
  panel like a library file: placing a component or applying a style copies it in as a read-only
  linked copy, as for library files. While the Assets panel shows, the app asks the server for a
  newer version at once and then every 30 seconds (a few bytes when nothing changed) and shows
  **Library updates available**; accepting keeps instance overrides. While the server can't be
  reached, the last version fetched stays in use. The file keeps the link, token included:
  sharing that file shares read access to the library.
- **Usage.** Each time a file's app asks for the library it reports which of its components the
  file uses and how often. **Show usage** in the library file (`library.usage`) lists the files and
  the instances of each component. Files are told apart by their names; a file's report changes
  only when its usage does.
- **Swapping.** `library.swap` (the swap button by an enabled library) moves a file's instances,
  styles and variables from one library to another's of the same names, keeping overrides.

The app never waits for the server: publishing, adding a library by its link and **Show usage**
send their requests on a thread of their own, show that they're on their way (with **Cancel**) and
finish when the answer arrives, acting on the file they started in. Scripts, the CLI and the MCP
server run the same commands and wait for the answer.

The browser build can't use team libraries yet (it has no library support of its own).

## Plugin directory

The server keeps a directory of plugins ([`plugins.md`](plugins.md#plugin-directories)) on the same
port as everything else; no other service is involved.

- **Who can do what.** Anyone who can reach the server lists, searches and downloads its plugins
  (no link or key needed: put the server behind your network or proxy if that's too open). Only
  whoever has the **server key** publishes and unpublishes, the same key that publishes files: there
  are no publisher accounts, so give it only to people you trust to put code on your team's
  machines. A plugin's author is shown as the publisher typed it in the manifest; nothing verifies
  it.
- **Publishing.** `figcraft-cli plugin publish FOLDER --server ws://host:47900/` with the key in
  `FIGCRAFT_SERVER_KEY` (or `?key=` in the address), or `plugin.directory.publish` from the app's
  scripts and MCP. Each version must be newer than the last (`1.0.0` → `1.0.1`); the server keeps
  every version until it is unpublished, and lists the latest.
- **What the server checks** before keeping a bundle: its size (`max_plugin_mb`), that it decodes
  (an uncompressed format: nothing expands), that its manifest parses and asks for a plugin API this
  server's FigCraft runs, a version of one to four numbers newer than the one kept, that the module
  is WebAssembly that compiles and imports only what FigCraft's sandbox provides (it is never run on
  the server), and that asset names are plain file names. Then the caps: `max_plugins` versions in
  all and `max_plugins_mb` of bundles.
- **Downloads** carry the bundle's SHA-256, which the app checks before reading it, and which the
  server checks again when it reads the bundle back from disk (a damaged one is never sent, and is
  logged). Downloads share a budget of about 32 MiB a second, so many at once are turned away with
  "busy" rather than queued in memory.
- **Moderation** is the operator's: `figcraft-server plugins` lists every plugin and its versions,
  `figcraft-server plugins remove ID [--version V]` takes a version, or the whole plugin, down (it
  works on a running server too). Installed copies keep working; users can't tell the server to
  remove anything without the key.
- **Turning it off**: `max_plugins = 0`.

## Managing files and links

| Command | What it does |
|---|---|
| `figcraft-server key [--rotate]` | The address to publish to, with the key; `--rotate` makes a new key (the server must be stopped) |
| `figcraft-server files [list]` | The files: id, name, people in it now, last change, size |
| `figcraft-server files remove FILE` | Remove a file and its links for good (whoever is in it is disconnected) |
| `figcraft-server links list FILE` | A file's links: id, access, created, label |
| `figcraft-server links create FILE --access edit\|view [--label TEXT]` | A new link, printed once |
| `figcraft-server links revoke FILE LINK` | Revoke a link: it no longer joins, and whoever joined with it is disconnected |
| `figcraft-server plugins [list]` | The plugin directory: every plugin, its versions, author, access, size and SHA-256 |
| `figcraft-server plugins remove ID [--version V]` | Unpublish a version of a plugin, or every version |

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
| `max_plugins` | 500 | Most plugin versions the plugin directory keeps (all plugins together); 0 turns the directory off |
| `max_plugin_mb` | 16 | Largest plugin bundle, in MiB (at most 32) |
| `max_plugins_mb` | 1024 | Most MiB of plugin bundles kept |

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
<data>/files/<id>/library.bin  its team library, when one was published (a .figcraft archive)
<data>/files/<id>/usage.json   which files use that library's components, as their apps report it
<data>/plugins/index.json      the plugin directory: every version kept (manifest fields, size, SHA-256)
<data>/plugins/<sha256>.bin    a plugin bundle, named by the SHA-256 of its bytes
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
- An index entry whose bundle is missing or the wrong size is left out at startup and reported;
  bundles are deleted only when their last version is unpublished. A `plugins/index.json` that
  can't be read lists nothing and refuses publishing (nothing is written over it) until it is
  restored or removed.

## Backups

Copy the data directory (the plugin directory, `plugins/`, included) while the server is stopped,
or take an atomic snapshot of the filesystem it lives on (ZFS, Btrfs, LVM, a VSS shadow copy). A plain copy of a running server's
directory can catch a file between its snapshot and its log; it restores, but may lose that
file's changes since its last compaction. To restore, stop the server, put the directory back and
start it: apps that were in a file catch up with a fresh copy (their unsent edits are kept when
the server still knows them, else they are told what couldn't be kept).

## Behind a reverse proxy (TLS)

The server speaks plain WebSocket (`ws://`); TLS comes from a reverse proxy in front of it, which
gives people `wss://` links (and browsers the `wss://` they need on an `https://` page).

- Run the server on the loopback interface (`listen = "127.0.0.1:47900"`), and set
  `public_url` to the address people use, for example `wss://figcraft.example.com/`. Links and
  the publish address then name the proxy.
- **Caddy** ([`packaging/server/Caddyfile`](../packaging/server/Caddyfile)): a site block for
  your host name with a `reverse_proxy` to `127.0.0.1:47900`. Caddy obtains the certificate and
  passes WebSocket upgrades through on its own.
- **nginx** ([`packaging/server/nginx.conf`](../packaging/server/nginx.conf)): a `server` block
  on 443 with your certificate that passes requests to `http://127.0.0.1:47900` over HTTP/1.1
  with the `Upgrade` and `Connection` headers forwarded (that is what lets the WebSocket upgrade
  through), and a read timeout above the server's 45-second idle limit (apps ping every 15
  seconds).
- **Under a path**: the server answers on any path, so the proxy can forward a prefix such as
  `/figcraft/` unchanged; set `public_url = "wss://example.com/figcraft/"`.
- Tokens never travel in the WebSocket request (the client sends them inside its first message),
  so the proxy's access logs don't record them.

The desktop app, `figcraft-cli` (and so agents through MCP) and the browser build all open
`wss://` links. The desktop app and the CLI check the certificate against Mozilla's list of root
authorities built into them (not the operating system's store), so a certificate from a public
authority such as Let's Encrypt just works. For a certificate from a private authority (or a
self-signed one, such as Caddy's `tls internal`), set the environment variable `FIGCRAFT_CA_FILE`
to that authority's certificate (a PEM file) before starting the app: it is trusted in addition
to the built-in list. There is no way to skip the check: a certificate that has expired, is for
another name or comes from an unknown authority is refused, and the app says which.

## Deploy on a Linux server

Hosting for people outside your network: a small Linux server (1 CPU and 1 GB of memory are
plenty for a team), a DNS name pointing at it (`figcraft.example.com` below: use yours), the
server behind Caddy or nginx for TLS, and systemd keeping it running. The commands are for Debian
and Ubuntu; other distributions differ only in how packages are installed.

**1. Install the binary**, from the latest release (x86_64 or aarch64), checked against the
release's `SHA256SUMS.txt`:

```sh
VERSION=$(curl -fsSL https://api.github.com/repos/storytold/figcraft/releases/latest | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p')
NAME="figcraft-server-$VERSION-linux-$(uname -m)"
curl -fLO "https://github.com/storytold/figcraft/releases/download/v$VERSION/$NAME.tar.gz"
curl -fLO "https://github.com/storytold/figcraft/releases/download/v$VERSION/SHA256SUMS.txt"
sha256sum --ignore-missing -c SHA256SUMS.txt
tar -xzf "$NAME.tar.gz" && cd "$NAME"
sudo install -m 755 figcraft-server /usr/local/bin/
```

or from source (it needs a C compiler for the TLS library; the example files are then in
`packaging/server/`):

```sh
sudo apt install -y build-essential git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && . "$HOME/.cargo/env"
git clone https://github.com/storytold/figcraft && cd figcraft
cargo build --release --locked -p figcraft-server
sudo install -m 755 target/release/figcraft-server /usr/local/bin/
cd packaging/server
```

**2. A user of its own, its data directory and its settings:**

```sh
sudo useradd --system --home-dir /var/lib/figcraft-server --shell /usr/sbin/nologin figcraft
sudo install -d -o figcraft -g figcraft -m 700 /var/lib/figcraft-server
sudo install -d -m 755 /etc/figcraft-server
sudo tee /etc/figcraft-server/server.toml >/dev/null <<'EOF'
listen = "127.0.0.1:47900"
data = "/var/lib/figcraft-server"
public_url = "wss://figcraft.example.com/"
EOF
```

**3. Run it with systemd.** [`figcraft-server.service`](../packaging/server/figcraft-server.service)
runs it as that user, restarts it if it fails, stops it gracefully, and locks it down: no new
privileges, a read-only system with only the data directory writable, no capabilities, private
`/tmp` and devices, and a system-call filter.

```sh
sudo cp figcraft-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now figcraft-server
systemctl status figcraft-server        # logs: journalctl -u figcraft-server
```

**4. TLS in front of it.** Point the DNS name's A (and AAAA) record at the server first, then set
`HOST` to it. Either **Caddy** (simplest: it gets and renews the certificate itself; packaged in
Debian 12+ and Ubuntu 24.04+):

```sh
HOST=figcraft.example.com
sudo apt install -y caddy
sed "s/figcraft.example.com/$HOST/" Caddyfile | sudo tee /etc/caddy/Caddyfile >/dev/null
sudo systemctl reload caddy
```

or **nginx** with a certificate from Let's Encrypt (certbot renews it):

```sh
HOST=figcraft.example.com
sudo apt install -y nginx certbot python3-certbot-nginx
sudo certbot certonly --nginx -d "$HOST"
sed "s/figcraft.example.com/$HOST/g" nginx.conf | sudo tee /etc/nginx/conf.d/figcraft.conf >/dev/null
sudo nginx -t && sudo systemctl reload nginx
```

**5. Firewall: only SSH, 80 and 443.** Port 80 answers certificate challenges and redirects to
https; the server's own port stays closed (it listens on the loopback interface anyway).

```sh
sudo ufw allow OpenSSH
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw enable
```

**6. Publish and share.** Print the publish address (`wss://figcraft.example.com/?key=…`) and use
it in the app's **Share ▸ Publish to server…** (see [Publishing a file](#publishing-a-file)):

```sh
sudo -u figcraft figcraft-server --config /etc/figcraft-server/server.toml key
```

The other commands ([Managing files and links](#managing-files-and-links)) run the same way, as
the `figcraft` user with `--config /etc/figcraft-server/server.toml`.

**In Docker instead of systemd.** [`packaging/server/Dockerfile`](../packaging/server/Dockerfile)
builds a small image (the binary on a distroless base, no shell) that runs as an unprivileged
user, keeps its files in the `/data` volume and stops gracefully on `docker stop`. Build it from
the repository root, give it a settings file with your `public_url`, and publish its port on the
loopback interface only (ports Docker publishes bypass `ufw`), with the proxy from step 4 in front:

```sh
docker build -f packaging/server/Dockerfile -t figcraft-server .
sudo install -d /etc/figcraft-server
printf 'listen = "0.0.0.0:47900"\ndata = "/data"\npublic_url = "wss://figcraft.example.com/"\n' | sudo tee /etc/figcraft-server/server.toml
docker run -d --name figcraft-server --restart unless-stopped -p 127.0.0.1:47900:47900 \
  -v figcraft-data:/data -v /etc/figcraft-server/server.toml:/etc/figcraft-server/server.toml:ro figcraft-server
docker exec figcraft-server /usr/local/bin/figcraft-server --config /etc/figcraft-server/server.toml key
```

**Upgrading.** Install the new binary as in step 1, then restart: the restart is graceful and apps
reconnect on their own. An app speaking another protocol version is refused with a message saying
so: upgrade apps and server together when the release notes say the protocol changed.

```sh
sudo systemctl restart figcraft-server
```

With Docker: rebuild the image, then `docker rm -f figcraft-server` and the `docker run` above
again (the volume keeps the files).

**Backups.** See [Backups](#backups); with systemd, for example:

```sh
sudo systemctl stop figcraft-server
sudo tar -C /var/lib -czf "figcraft-backup-$(date +%F).tar.gz" figcraft-server
sudo systemctl start figcraft-server
```

## Security

- **Access** is a share link (128-bit random token) per file and access level, or the server key
  (128-bit) for managing files. Tokens are stored as SHA-256 hashes and every comparison runs in
  constant time; revoking a link disconnects whoever joined with it. A view link's batches are
  refused by the server whatever the client does.
- **Transport**: plain `ws://` on the server's port; anyone on the network path can read traffic
  (presence, changes, tokens) unless TLS terminates in front of it. Use a reverse proxy for
  anything beyond a trusted network ([Deploy on a Linux server](#deploy-on-a-linux-server)).
- **Limits**: the transport's (message sizes, 100 messages a second per connection, idle and
  handshake timeouts, `max_connections`) and the collaboration core's (layers, changes per batch,
  `max_people`), plus `max_files`, `max_file_mb` and `max_links`. Every message is decoded
  defensively, and fuzzed in the tests. A library is read in full before it is kept (as files
  using it read it: size limits on every part, at most `max_file_mb`), must be the file's library
  at a newer version, and is checked on the server's main thread; usage reports keep at most 2,000
  components per file and 500 files per library.
- **Team libraries**: any link of a file reads its library and its usage reports (file names and
  counts); only an edit link publishes it.
- **Plugin directory**: anyone who reaches the server reads it; only the server key writes to it.
  Everything published is checked in full and never run on the server; a bundle's name on disk is
  its hash, so nothing a publisher chooses reaches the file system; downloads are hash-checked at
  both ends and share a bandwidth budget. Installing still means trusting the publisher: a plugin
  runs in FigCraft's sandbox with the permissions its manifest asks for, which the app shows before
  it installs anything.
- **Not there yet**: user accounts, per-person permissions, rate limits per address, audit logs.
