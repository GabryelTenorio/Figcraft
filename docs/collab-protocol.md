# Real-time collaboration protocol (draft)

Status: **proposed design**, milestone M9. Nothing here is implemented yet; this page is the plan
the implementation PRs follow and update.

Goal: several people edit the same `.figcraft` file at the same time and see each other's edits,
cursors and selections live, on the desktop app and in the browser, without a central cloud
service being required.

## Design in one paragraph

Clients talk to a **session server** over **WebSockets** using small **binary messages**. The
server is the single authority for ordering: it assigns every accepted change a sequence number and
rebroadcasts it. The document syncs as **state, not operations**: each change says "property P of
object O is now V" (plus create, delete and move), and conflicting writes to the same property
resolve **last-writer-wins in server order**. Clients apply their own edits immediately
(optimistically) and reconcile when the server echoes or overrides them. Cursors, selections and
viewports are **ephemeral presence** messages that are never stored. The desktop app can **host a
session itself** (LAN or a forwarded port), so no public server is needed to start; the same
server code also builds as a standalone `figcraft-server` binary for self-hosting.

## Why this shape

Public engineering write-ups about multiplayer design tools converge on the same choices, and they
fit FigCraft's architecture:

- **WebSockets**: one long-lived, bidirectional, ordered connection; works natively (tungstenite)
  and in the browser build (the browser's WebSocket API through `web-sys`), passes through proxies
  and TLS (`wss://`) like ordinary HTTPS. Plain HTTP polling adds latency and overhead; WebRTC
  peer-to-peer complicates NAT traversal and gives up a single ordering authority.
- **A central authority instead of OT or a full CRDT**: a design document is a tree of objects with
  properties, not a text buffer. Operational transformation is complex and fragile for that shape;
  general CRDTs carry metadata and tombstones we don't need when a server orders everything. With
  per-property last-writer-wins plus a few tree rules (below), every client converges to the
  server's state, and the server can reject invalid changes (cycles, edits to deleted objects).
- **State-based changes instead of replaying commands**: FigCraft's commands are deterministic
  locally, but replaying them on other clients would require identical documents, fonts and
  floating point everywhere. Instead a command runs once, on the client that issued it, and the
  resulting document difference is what gets sent. FigCraft's document is a persistent map of
  `Arc<Node>` with dirty tracking, so computing the exact changed nodes and properties after a
  command is cheap.
- **Binary encoding**: cursor traffic is frequent and documents are large; a compact, schema-based
  binary format keeps messages small and fast to decode. JSON stays available for debugging and
  for agents (the same messages have a JSON form).

## Identifiers

- **Client id**: a 32-bit id the server assigns to each connection on join (never reused within a
  file's lifetime; stored with the file).
- **Object ids** must be unique across clients without coordination: `NodeId(u64)` becomes
  `client_id << 32 | counter` for objects created in a session (each client counts its own).
  Ids in existing files stay valid (they occupy client id 0). The same scheme covers pages, styles,
  variables, collections, comments and every other synced object.

## The document as objects and properties

Every synced thing is an **object** with an id, a type and a set of **properties**:

- Nodes: every field of `Node` that is saved in the file is a property (name, geometry, fills,
  strokes, effects, auto layout, text content and runs, component data, bindings, prototype
  interactions…). Large or list-valued fields (fills, text runs, vector networks) are a single
  property each: replaced whole, last writer wins.
- **Tree structure** is two properties on the child: `parent` (an object id) and `position` (a
  **fractional index** string, e.g. between `"a0"` and `"a1"` sits `"a0V"`). A parent's children
  are its children sorted by `position`, so two clients inserting into the same frame never
  conflict, and a move is one property write. The server rejects a `parent` write that would
  create a cycle or point at a deleted object, and replies with the current value.
- Document-level data (pages order, styles, variables, collections, modes, published library
  metadata, comments, versions) are objects with properties in the same way.

## Messages

All messages share an envelope `{type, …}`. Encoding: a compact schema-based binary format
(postcard/serde or protobuf via `prost` derive, decided in the first implementation PR with
benchmarks), with a 1-byte protocol version up front; every message also has a JSON form.

Client → server:

| Message | Meaning |
|---|---|
| `Hello {protocol, file, token, name, color, client_version}` | Join a file's session. |
| `Changes {base_seq, batch_id, changes: [Change]}` | One user action's changes (one undo step). |
| `Presence {cursor?, selection?, viewport?, editing_text?}` | Ephemeral; throttled to ~30 Hz, latest wins. |
| `Ack {seq}` | Highest sequence applied (lets the server trim per-client buffers). |
| `Ping` | Keep-alive. |

Server → client:

| Message | Meaning |
|---|---|
| `Welcome {client_id, seq, snapshot or delta, peers}` | Join accepted: the full document (or changes since a cached `seq`). |
| `Changes {seq, author, batch_id, changes}` | Accepted changes, in order, to everyone (the author gets its own back as confirmation). |
| `Reject {batch_id, reason, fixes: [Change]}` | A batch (or part of it) was invalid: the current values to roll back to. |
| `Presence {client_id, …}` / `PeerJoined` / `PeerLeft` | Other people's cursors, selections and arrivals. |
| `Pong` | Keep-alive. |

`Change` is one of: `Create {id, type, props}`, `Set {id, prop, value}`, `Delete {id}` (deleting a
node deletes its subtree; deleted ids are never reused).

## Ordering, optimism and convergence

1. A user action runs locally as today (one command or gesture = one undo step). The client diffs
   the document before and after, sends the `Changes` batch and shows the result immediately.
2. The client keeps its **unconfirmed batches** in order. Changes from others are applied to the
   confirmed state; the client's unconfirmed property values are then re-applied on top, so a
   user never sees their own pending edit flicker away.
3. When the server echoes a batch, it moves from unconfirmed to confirmed. On `Reject`, the client
   drops the rejected values and applies the server's `fixes`.
4. Result: every client's document equals the server's once everyone has caught up (a randomized
   multi-client convergence test is part of the first implementation).

Derived data is **not** synced: auto layout results, component instance contents and variable
resolution are recomputed locally from the synced inputs where possible, so clients don't fight
over computed values. Where FigCraft currently stores derived values in nodes (instance copies,
laid-out positions of auto layout children), the implementation either marks those properties as
derived (recomputed by each client after applying remote changes) or syncs them with the authoring
change; this is decided per property in the first PR and listed here.

## Undo and redo

Undo stays per user and undoes only **your own** actions: an undo turns your last batch into a new
batch that writes back the values your batch replaced, for the properties nobody has changed since
(a property someone else changed afterwards is left alone). Redo is the inverse. Remote changes
never enter your undo history.

## Presence

- Cursor position in page coordinates plus the page id; selection ids; viewport rectangle; whether
  the user is typing in a text layer (to show their caret and selection inside it).
- Shown as named, coloured cursors on the canvas, selection outlines in the user's colour, avatars
  in the title bar; click an avatar to **follow** that user's viewport (Escape or any pan stops).
- Never stored; dropped when a client disconnects; the server rate-limits and coalesces.

## Hosting and joining

- **Host from the app** (first step, no public server): "Share ▸ Start live session" starts the
  embedded server on the local machine for the open file and shows an invite link
  (`figcraft://join?host=192.168.1.20:47900&file=<id>&token=<secret>`; also a plain
  `ws://`/`wss://` URL). Others on the same network (or through a forwarded port / VPN such as a
  mesh VPN) join with "File ▸ Join live session" or by opening the link. When the host leaves, the
  session ends; everyone keeps a local copy.
- **Self-hosted server** (next step): the same server as a `figcraft-server` binary that keeps many
  files, persists them (a snapshot plus an append-only log of accepted batches, compacted into new
  snapshots), and survives restarts. TLS comes from a reverse proxy (`wss://`).
- **Auth**: an unguessable per-session token in the invite link (first step); accounts and per-file
  permissions (view, edit) come with the standalone server.

## Offline and reconnect

On a dropped connection the client keeps editing locally, queues batches, and on reconnect sends
`Hello` with its last confirmed `seq`; the server replies with the missed changes, then the queued
batches are sent (they may be partly rejected if their objects were deleted meanwhile).

## Agents, MCP and the CLI

Everything stays a command: `collab.host`, `collab.join`, `collab.leave`, `collab.peers`,
`collab.follow` are ordinary commands, so the CLI, the control channel and MCP can host and join
sessions; an agent can be a participant with its own cursor.

## Security

The server validates every change (types, ranges, the same caps the engine applies to commands,
tree rules, message size limits) and never trusts client-computed data. The server and client
parsers are fuzzed like the file importers. Sessions bind to all interfaces only when the user
chooses to share on the network; the default is the invite link with a random token.

## Implementation plan

1. **Core (no networking):** client-scoped ids, the object/property model over `Document`, diff
   after a command → `Changes`, apply `Changes`, fractional positions, server-side validation and
   ordering as a pure library (`crates/collab`, engine level), and a randomized N-client
   convergence test with an in-memory transport.
2. **Transport + embedded host:** WebSocket server/client (native), the binary encoding, the
   `collab.*` commands, Share / Join UI, live cursors, selections, avatars and follow.
3. **Per-user undo**, reconnect and offline queueing, rejection handling, presence for text
   editing.
4. **Browser client** (WebSocket through `web-sys`).
5. **Standalone `figcraft-server`** with persistence, compaction and accounts.
