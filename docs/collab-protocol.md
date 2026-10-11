# Real-time collaboration protocol (draft)

Status: milestone M9. **Step 1 (the core, no networking) is implemented** in `crates/collab`:
identifiers, the object/property model, change sets, ordering and validation, client
reconciliation, derived data and the message encoding. **Step 2 is implemented** in
`crates/collab-net` and the engine: the WebSocket transport, the session host the desktop app
embeds, invite links, document sync end to end (several apps edit one file), live cursors,
selections, avatars and follow. **Step 3 is implemented**: per-user undo and redo, rejection
handling and offline queueing with reconnect (`crates/collab`, wired into the engine's live
session and the transport). **Step 4 is implemented**: the browser build joins sessions a desktop
app hosts, and `figcraft://` invite links open the app and join. Styles, variables, collections,
modes and comments created in a session take ids from their creator's id space, and others' text
carets show on the canvas. **Step 5 is implemented**: the
standalone `figcraft-server` (`apps/figcraft-server`, running it: [`server.md`](server.md)) keeps
many files with persistence and compaction, and share links that let people edit or only view;
the app publishes to it. The rest is the plan the next implementation PRs follow and update;
sections say what is built.

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

*Built.*

- **Client id**: a 32-bit id the server assigns to each connection on join. The server starts
  above every client id space already used by node ids in the file, so ids from earlier sessions
  are never handed out again. The counter is part of the session state a server saves
  (`Server::state` / `Server::restore`), so a restarted server doesn't hand ids out twice either;
  a client that reconnects keeps its id.
- **Node ids** are unique across clients without coordination: with a client id set
  (`Document::set_client_id`), `Document::alloc_id` returns `client_id << 32 | counter`,
  continuing after the highest id of that space already in the document. Ids in existing files
  stay valid (they occupy client id 0), and a document without a client id allocates exactly as
  before. The counter is saved as the file's `next_id`; the client id is never saved, and neither
  is part of document equality.
- An id is never *allocated* twice. A deleted node can come back with its id only by being
  created again whole (what undo does): the server accepts creating an id that doesn't exist only
  if it was deleted before or lies in the sender's own id space.
- **Document-level ids** (`figcraft_doc::ids`): styles, variables, variable collections, modes
  and comment threads created during a session take a number from the same allocator as node ids
  (`client_id << 32 | counter`, never handed out twice): `S:<n>`, `VariableID:<n>`,
  `VariableCollectionId:<n>`, `<collection>/mode<n>` and comment id `n`. So two people creating a
  style (or a variable, a comment…) at the same moment get different ids and both survive; before,
  both got the next free number and the later write replaced the earlier object. Outside a session
  ids are made exactly as before (`S:<k>` with the first free `k`, one above the highest for
  variables, collections and comments, the first free `mode<k>`), so existing files and single-user
  editing are unchanged. The number's high 32 bits are its id space; ids from before (small
  numbers) and ids of other forms (library copies `library:key`, files placed into the document)
  are in space 0. Server client ids start above every space used by any of these, not only by
  layers.
- The server refuses a batch that creates a document-level entry (by `Put` or by replacing a whole
  list) whose id lies in the space of another client this session gave an id to, unless the entry
  was removed earlier in the session (undo puts entries back, whoever made them). Ids in space 0 or
  in spaces this session never handed out (earlier sessions, other files) are accepted: they can't
  collide with what a connected client makes, because a client's numbers continue above every id of
  its space already in the document. Removed entries are part of the saved session state
  (`SessionState::removed`).
- Objects inside an entry are not separate objects: modes are part of their collection, a
  variable's values per mode part of the variable and replies part of their comment thread. Two
  people adding a mode to the same collection, editing different modes of one variable, or replying
  to the same thread at the same moment still resolve last-writer-wins on the whole entry (mode ids
  no longer collide, so nothing is mixed up; one of the two additions is lost).

## The document as objects and properties

*Built* (`figcraft_collab::{Change, Prop, DocProp, Entry, Replica}`).

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

As built:

- **Node properties** are `parent`, `position` and one property per saved field of `Node` (name,
  kind with its per-type data, visibility, lock, opacity, blend, transform, size, fills, strokes,
  stroke settings, effects, corners, constraints, auto layout, child layout, clip, masks, grids,
  guides, exports, interactions, overflow, scroll position, style links, variable bindings and
  modes, instance source, overrides, property references, plugin data, handoff status and note,
  annotations). `children` is not a property: it follows from the children's `parent` and
  `position`. The build fails when `Node` gains a field that isn't listed.
- **Positions** are fractional keys (`figcraft_collab::frac`: base-62 digits after a length-coded
  integer part, so appending keeps keys short; deterministic; never ending in `0`). A replica
  orders every parent's children, and the pages, by `(key, id)`; equal keys from simultaneous
  inserts fall back to id order. Existing documents get keys in their current order on first sync.
  A diff gives new keys only to the children that need them (the longest run of old keys that is
  still increasing keeps its keys) and re-spaces a parent whose keys would grow past 48 characters.
- **Document-level objects covered now**: the file name, the current page, document colours, the
  colour profile, image presets, annotation categories and library data (whole values); styles,
  comment threads, variable collections and variables (one entry per id plus the order of ids;
  the whole list when ids aren't unique); dev-mode snapshots (per layer); images (per content
  hash, the encoded bytes as base64). The page order is the pages' positions.
- **Next**: this file's published library (it embeds a snapshot document; it moves with an asset
  channel, together with images, which then stop travelling inside batches); the version history
  (kept beside the document, not in it) on the standalone server; finer entries for modes,
  per-mode variable values and comment replies (see Identifiers).
- **Changes** (`Change`): `Create {id, kind, parent, position, props}` (only the properties that
  differ from a new node of that kind), `Set {id, prop, value}`, `Delete {id}` (the subtree; a
  delete of something already gone does nothing), and for document-level data
  `SetDoc {prop, value}`, `Put {entry, value}` and `Remove {entry}`. Values are JSON.
- **Diff** (`figcraft_collab::diff(replica, edited_document)`): only nodes whose `Arc` differs are
  visited (the persistent map skips shared structure), so the cost follows the size of the edit. A
  batch lists field writes, creations (parents first), moves, deletions (one per subtree, after
  the moves out of it), then document-level changes. `Replica::apply` applies a batch strictly
  (stop at the first invalid change, then check the result: the server's mode) or leniently (skip
  what no longer applies: a client replaying its pending edits). For random edits made by real
  engine commands, `apply(diff(a, b), a) == b`, positions included (tested).

## Messages

*Built* (`figcraft_collab::message`): the types below and both encodings. Protocol version 2 (the
`Hello`'s `protocol`): the welcome says what the client may do, and a connection can manage a
server's files instead of joining one; a client of another version is refused with the reason.

**Encoding (decided):** a binary message is a 1-byte wire version followed by **postcard** (serde),
which is compact and needs no schema files. Property values inside are compact JSON text: the
document types use tagged and flattened serde forms that only self-describing formats can read
back, and JSON keeps them exact (floats round-trip). The **JSON form** of every message is the same
types through `serde_json`, externally tagged (`{"changes": {"base_seq": …}}`; postcard can't read
internally tagged enums). Measured: a typical edit (one layer moved: its transform) is **43 bytes
binary vs 139 JSON**; a cursor update is **23 bytes vs 93**. Decoders take a size limit, refuse
trailing bytes and never allocate much beyond the message's size; JSON nesting is capped by the
parser; both are fuzzed.

Client → server:

| Message | Meaning |
|---|---|
| `Hello {protocol, file, token, name, color, client_version, resume?}` | Join a file's session; `resume {session, client_id, seq}` when coming back after a dropped connection. |
| `Changes {base_seq, batch_id, changes: [Change]}` | One user action's changes (one undo step). |
| `Presence {cursor?, selection?, viewport?, editing_text? {node, caret, selection?}}` | Ephemeral; throttled to ~30 Hz, latest wins. |
| `Ack {seq}` | Highest sequence applied (lets the server trim per-client buffers). |
| `Ping` | Keep-alive. |
| `Manage {protocol, key, client_version}` | Instead of `Hello`: manage a server's files with its key. |
| `Admin {publish {name, snapshot} \| files \| links {file} \| createLink {file, access, label} \| revoke {file, link} \| remove {file}}` | A request on a managing connection; each gets one `Admin` reply, in order. |

Server → client:

| Message | Meaning |
|---|---|
| `Welcome {session, client_id, seq, catch_up: Snapshot or Delta, processed, peers, access}` | Join accepted: the full document, or for a reconnecting client the batches it missed; what the server did with that client's batches (`processed {last_batch, refused}`); `access` is `edit` or `view` (a view link: every batch is refused). |
| `Changes {seq, author, batch_id, changes}` | Accepted changes, in order, to everyone (the author gets its own back as confirmation). |
| `Reject {batch_id, reason, fixes: [Change]}` | A batch (or part of it) was invalid: the current values to roll back to. |
| `Presence {client_id, …}` / `PeerJoined` / `PeerLeft` | Other people's cursors, selections and arrivals. |
| `Refused {reason}` | The connection is refused (protocol version, token, session full, a message out of place). |
| `Pong` | Keep-alive. |
| `Admin {ready \| published {file, edit, view} \| files \| links \| link \| done \| failed {reason}}` | A managing connection's answer: the key is right, a published file with its first two links (tokens shown once), listings, a new link, done, or why not. |

`Change` is one of: `Create {id, type, props}`, `Set {id, prop, value}`, `Delete {id}` (deleting a
node deletes its subtree; ids are never allocated twice), plus the document-level `SetDoc`, `Put`
and `Remove` (see above). A snapshot is the list of changes that builds the document from nothing;
`Welcome` carries one, or the missed batches when a client reconnects (see Offline and
reconnect). A whole batch is accepted or rejected.

## Ordering, optimism and convergence

*Built* (`figcraft_collab::{Server, Client}`: pure state machines; the transport only moves
messages).

1. A user action runs locally as today (one command or gesture = one undo step). The client diffs
   the document before and after, sends the `Changes` batch and shows the result immediately.
2. The client keeps its **unconfirmed batches** in order. Changes from others are applied to the
   confirmed state; the client's unconfirmed property values are then re-applied on top, so a
   user never sees their own pending edit flicker away.
3. When the server echoes a batch, it moves from unconfirmed to confirmed. On `Reject`, the client
   drops the rejected values and applies the server's `fixes`.
4. Result: every client's document equals the server's once everyone has caught up.

As built: the server validates each batch as a whole by applying it to a copy (limits on changes
per batch and layers per file; values of the right type; finite, in-range sizes, transforms and
opacity; known nodes; parents that exist and can hold layers; pages only at the top; no cycles; at
least one page; new layer ids from the sender's space; new styles, variables, collections and
comments not in another client's space), then either commits it with the next sequence
number or rejects it with the current values of what it touched as fixes. The client keeps the
confirmed state, applies its own batches at once, and on someone else's batch or a rejection
rebuilds its view as the confirmed state with its pending batches replayed leniently (the fixes
are for clients that keep no confirmed copy). `figcraft_collab::sim` runs a server and N clients
over in-memory links that deliver in order per connection after random delays, through the binary
or the JSON encoding. Tests drive it with synthetic edits and, in the engine, with random real
commands (creating, moving and reparenting layers including cycle attempts, deleting, grouping,
auto layout, components and instances, styles, variables, text, pages, comments): after quiescence
every client's state equals the server's, for every seed (more seeds run as an ignored test).
The same runs with undo, redo and dropped connections are described below.

### Rejections

*Built.* When the server refuses a batch (a layer it changes was deleted by someone else, a move
would make a cycle, a value is out of range…), the `Reject` carries a short reason (a deleted
layer reads "a layer it changes was deleted by someone else") and the current values of what the
batch touched. The client drops the batch, applies the fixes to its confirmed copy (a no-op there:
every batch ordered before the rejection arrived first; they matter to clients that keep no
confirmed copy), rebuilds its view from the confirmed state and its remaining batches, and takes
the batch's step out of the undo history, together with any undo or redo of it still waiting to be
sent. It then queues a notice (`Client::take_notices`, `Notice::message`); the engine shows it as
the UI's toast (`Session::message`).

### Derived data

*Built.* Decided per property:

- **Synced with the authoring change** (the client making an edit runs the engine's normalize pass
  before diffing, so its batch carries consistent values, which receivers apply as they are): auto
  layout positions and sizes of children and hugging frames, text box auto-size (it depends on the
  fonts installed, so it is computed once, never per machine), group and boolean bounds, instance
  contents (copies of their mains with their own ids, overrides respected), values written by
  variable bindings and style links, ready-for-dev "changed" flags and snapshots, comment pins
  following their layers.
- **Repaired after concurrent edits:** when a client's batch is confirmed after someone else's
  batch was ordered before it, that client (only the author) runs the recompute hook over the
  nodes its batch touched, on the confirmed state, and sends any difference as a follow-up batch
  (leaving out properties its own later pending batches write). The engine's hook is its normalize
  pass (`figcraft_engine::collab::EngineRecompute`). Because only the author recomputes, clients
  never fight over a value, even with different fonts; a test checks that a derived invariant
  broken by concurrent edits is restored.
- **Not synced:** each person's selection, viewport, tool and text caret (presence), the id
  counter and client id, and (for now) this file's published library.

## Undo and redo

*Built* (`Client::{undo, redo}`; in the engine, `edit.undo` / `edit.redo` and the UI's undo route
there while the document is in a live session). Outside a live session the engine's snapshot undo
works exactly as before; a document entering or leaving a session starts with an empty history.

Undo stays per user and undoes only **your own** actions: an undo turns your last batch into a new
batch that writes back the values your batch replaced, for the properties nobody has changed since
(a property someone else changed afterwards is left alone). Redo is the inverse. Remote changes
never enter your undo history.

Decisions, as built:

- **A step per batch.** Every undoable batch of the user (one command, one gesture, one
  `command.batch` / `Session::grouped` group) becomes a step: per target (a node property, a
  document-level property or entry) the value before and after, plus the nodes it created and
  deleted, each as the change that creates it as it was. Batches that aren't commands' edits
  (derived-data follow-ups) never become steps.
- **"Nobody has changed it since"** is decided by server order: a batch from someone else that the
  server ordered after the step's last confirmed batch takes the targets it writes away from the
  step for good (a write to a list as a whole takes every entry of that list; creating a node again
  takes all its properties; a taken parent takes the position too). Batches ordered before it are
  what the step replaced: the step learns their values when its batch is confirmed or replayed, so
  an undo restores the value that was there in server order, not the one the user happened to see.
- **Creates become deletes, deletes create the subtree again** with the same ids, positions and
  values (the server accepts re-creating deleted ids). A created layer someone else has since put
  layers into is not deleted (and if nothing else of the step is left, the step is skipped); a
  layer whose parent is gone is not brought back. Changes are checked in order against the current
  state, so an undo is never refused for what the client can see.
- **An undo or redo waits for what came before it.** It shows at once, but goes out only when
  every earlier batch of the client is confirmed or refused, and is then made again from the
  confirmed state. So it never rests on an edit the server refused (the step and its undo are
  dropped with the edit) or on values the client hadn't seen yet. Undoing right after an edit
  costs one round trip before it reaches the others; edits made meanwhile queue behind it.
- **Derived data** that the restored values imply (auto layout, groups…) is recomputed with the
  recompute hook and sent as a follow-up batch, like the repairs after concurrent edits.
- **A step that no longer changes anything is skipped** (the next undo goes one step further).
- Concurrent edits the undoing client couldn't have seen (made while its undo was on the way, or
  while it was offline) are ordered last-writer-wins like any other edits.

Tests: the in-memory simulation checks every undo and redo the server accepts against the log of
accepted batches: a value it changes was last written by its author (or after the client's last
seen sequence number), and every layer it deletes was created by its author or moved into the
deleted layer after that. Random sessions of synthetic edits and of real engine commands, with
undo, redo and dropped connections, converge with no violation (thousands of seeds were run while
building it; an ignored test runs more). The engine's tests (its live session with a scripted
link) also cover a gesture staying one step while others edit, a group of commands syncing as one
step, an undo right after an edit waiting for it, a refused edit's toast, a dropped connection
resuming with its offline edits, and leaving the session going back to snapshot undo.

## Presence

*Built* (`figcraft_engine::live`, `crates/collab-net`, `crates/ui-egui/src/live.rs`).

- Cursor position in page coordinates plus the page id; selection ids; viewport rectangle; whether
  the user is typing in a text layer, with their caret and selection inside it
  (`editing_text {node, caret, selection}`, in characters). The message is the core's `Presence`; the engine builds yours from the active document (page, selection, view,
  edited text) and the pointer over the canvas, and sends it only when it changed.
- Shown as named, coloured cursors on the canvas (a pointer drawn in code with a name tag in the
  person's colour, gliding to each reported position; reduced motion makes it jump), selection
  outlines in the person's colour, and avatars (initials) at the top of the right panel beside
  Share; click an avatar to **follow** that person's viewport (`collab.follow {client}`), click
  again, press Escape or pan/zoom yourself to stop. The chevron lists everyone.
- **Text carets.** While someone types in a text layer of the shared document on your page, their
  caret and selected characters show on it in their colour (the text layout's own caret and
  selection geometry, so they sit at the right glyphs of your copy), with their name in a tag above
  the caret. Their caret counts characters of their own copy: typing syncs as one batch when they
  finish (text editing is one gesture), so until then a caret past the end of your copy sits at its
  end. Remote carets don't blink, so they never ask for frames of their own.
- **Two people in one text layer.** Starting to type in a layer someone else is typing in shows
  "Hana is editing this text too: the version finished last is kept" (the session's message), and
  their caret's tag reads "Hana is editing this text" while you type. Both edits sync when each
  person finishes, as whole values: the one the server orders last wins, as for any property.
  `collab.peers` reports who types where (`editing {node, layer, caret, selection}` per person, and
  yours).
- Never stored; dropped when a client disconnects. Throttled to about 30 updates a second on both
  ends, latest wins (a value offered too soon waits and is replaced by newer ones, so the last
  position always arrives). A newcomer gets everyone's latest presence after its welcome.
- Presence lines up with your canvas while the session's document is in front
  (`Session::live_shared`): cursors on your page, selections outlined, following switches pages.
  With another document in front (or before the welcome arrives), cursors show in page
  coordinates on whatever page you look at and nothing is outlined.
- Colours and names: the host hands out the first free colour of an eight-colour palette and
  "Guest N" to people without a name (Preferences: your name, `pref.userName`; also in the Share
  popover). After a newcomer's `Welcome` the host sends it its own `PeerJoined`, which is how it
  learns its name and colour.
- Idle cost is zero: network threads block, wake the window (`request_repaint`) only when a
  message arrives, cursors only ask for frames while they glide, and text carets never do.

## Hosting and joining

- **Host from the app** (first step, no public server): "Share ▸ Start live session" starts the
  embedded server on the local machine for the open file and shows an invite link
  (`figcraft://join?host=192.168.1.20:47900&file=<id>&token=<secret>`; also a plain
  `ws://`/`wss://` URL). Others on the same network (or through a forwarded port / VPN such as a
  mesh VPN) join with "File ▸ Join live session" or by opening the link. When the host leaves, the
  session ends; everyone keeps a local copy.
- **Self-hosted server**: the same server as a `figcraft-server` binary that keeps many files,
  persists them (a snapshot plus an append-only log of accepted batches, compacted into new
  snapshots), and survives restarts. TLS comes from a reverse proxy (`wss://`). *Built*, below.
- **Auth**: an unguessable per-session token in the invite link (hosting from the app); per-file
  share links that let people edit or only view, and a server key that manages files (the
  standalone server). User accounts come later.

*Built* (hosting from the app and joining):

- "Share" in the right panel opens a popover: your name, **Share on the local network** (off:
  127.0.0.1 only), **Start live session**; while hosting, the invite link and the `ws://` URL with
  Copy buttons, how many joined, and **Stop session**; while joined, **Leave session**. "File ▸
  Join live session…" (and "Join a session…" in the popover) asks for the link. Commands:
  `collab.host {port?, bind?: "local" | "lan" | <ip>}`, `collab.stop`, `collab.join {link}`,
  `collab.leave`, `collab.peers`, `collab.follow {client?}`, `collab.cursor {x?, y?}` (an agent's
  pointer).
- The host listens on port 47900 (any free port when it's taken, or `port`), on 127.0.0.1 unless
  the user chooses the local network (`lan`: the address of the interface the default route uses)
  or names an interface (`bind: "<ip>"`, e.g. a VPN's). It runs the core's `Server` as the
  authority and takes part itself without a socket.
- Links: `figcraft://join?host=<ip:port>&file=<id>&token=<secret>` or
  `ws://<ip:port>/?file=<id>&token=<secret>` (either works for joining). The client connects to
  `ws://<ip:port>/` and sends the token only inside its `Hello`. Links are pasted into the Join
  dialog (its field has the keyboard when it opens; Escape closes it), or opened from anywhere:
  see *Opening invite links* below.
- Reconnect: a connection that drops after it was live reconnects with growing pauses (0.5 s,
  doubling up to 10 s, 12 attempts); a refusal (wrong token, full) or the host ending the session
  is final; a first join that can't reach the host fails at once. A reconnect resumes as the same
  client: the guest's connection keeps the session id, client id and last sequence number from
  the messages it read and sends them in its next `Hello` (see Offline and reconnect).
- The browser build joins (see *Built (the browser client)*); hosting needs the desktop app.

*Built* (document sync between apps):

- The host's session runs the core's `Server`, seeded with the document it shared; the host's
  window is a `Client` of it like everyone else (it joins through the hub, without a socket), so
  its own edits go through the same ordering. A guest's welcome carries the whole document
  (pages, layers, styles, variables, images): it opens as a new tab, the guest's own files stay
  open, and leaving keeps it as a local copy.
- The engine syncs one open document per session (`Live::doc_uid`). Every command, group of
  commands or gesture on it is diffed against the client's view into one batch
  (`ClientMsg::Changes`) that is one step of the user's undo history (`Session::record`); a gesture
  (a drag, a resize) is sent once, when it ends, and nothing is sent while a version or a live
  text preview shows (anything changed outside a command goes out at the next frame, not
  undoable). Batches the server orders (`Changes`, `Reject`) go through
  `Client::receive` with the engine's recompute hook, and the resulting document is installed with
  `Session::apply_remote`, which keeps your selection, page and view and never adds an undo step.
  Batches that arrive during a gesture wait for it to end. New layers take ids in the client's
  space (`client_id << 32 | counter`). Closing the session's document leaves the session.
- Each applied batch wakes the window (the inbox calls `request_repaint`); nothing runs while
  nobody edits or moves.

*Built* (the browser client):

- The web build's link speaks the same binary messages through the browser's WebSocket API
  (`web-sys`, binary frames as `ArrayBuffer`). Its connection logic is a target-independent state
  machine (`crates/collab-net/src/driver.rs`) that mirrors the native guest: hello on open
  (resuming as the same client after a drop), presence held until the welcome and changes made
  for an earlier connection dropped (the sync client sends them again once caught up), presence throttled to about 30 updates a second
  (latest wins), a `Ping` after 15 s without sending (browsers don't expose WebSocket ping frames;
  the host answers `Pong`), a connection silent for 45 s counted as dropped, reconnects with the
  same growing pauses after a live connection drops, refusals and the end of the session final,
  and a first join that can't reach the host failing at once. The browser layer only feeds it the
  socket's events and one `setTimeout` for its next deadline, and every arrival wakes egui
  (`request_repaint`): an idle session draws nothing. The machine is tested natively against a
  scripted socket and against a real host over loopback.
- The page runs on one thread: sockets, callbacks and timers live in a thread-local table and the
  engine's link (which must be `Send`) holds only the session's number and its inbox.
- Hosting stays desktop-only (a page can't listen on a socket); the Share popover in the browser
  says so and offers only joining, and `collab.peers` reports `canHost: false`.
- **Mixed content.** A desktop-hosted session is plain `ws://`. Browsers block `ws://` from a page
  loaded over `https://`, except to this machine (`127.0.0.1`, `[::1]`, `localhost`), so an https
  page refuses other hosts up front with a message instead of failing obscurely. To join from
  other machines, serve the web build over `http://` on a trusted network (locally:
  `python3 -m http.server 8767` in the built site, then `http://localhost:8767/` or the machine's
  LAN address), or use `wss://` links of a `figcraft-server` behind a TLS reverse proxy (the
  browser opens them; see *Built (the standalone server)*). Some browsers also ask before a public
  site may reach private network addresses.

*Built* (the standalone server, `apps/figcraft-server`; running it: [`server.md`](server.md)):

- **One hub for both hosts.** The app's session host and the server run the same hub
  (`crates/collab-net/src/hub.rs`): the listener, a reader and a writer thread per connection,
  and one thread owning a core `Server` per open file. What differs sits behind a `Backend`: which
  hellos it admits and with what access, where a file that isn't open comes from, what it keeps
  of the changes, and the managing connections. The app's backend has one file and its session
  token; the server's is its data directory. Hellos carry the token to the backend, which checks
  it; the sessions themselves keep no secret.
- **Files and share links.** Each file has links, each with an access (`edit` or `view`), a label
  and a 128-bit token stored only as its SHA-256 hash (compared in constant time). A hello names
  the file and a link's token; the server joins it with that link's access (`Server::join_as`). A
  viewer gets every change and everyone's presence and sends its own presence; each batch it
  sends is refused (`Reject`, "you can only view this file") and rolled back. Revoking a link
  (or removing a file) disconnects whoever joined with it, with the reason; the link no longer
  joins.
- **Links** name the server's endpoint: `figcraft://join?host=<host:port>&file=…&token=…` for a
  plain `ws://` endpoint at `/`, `figcraft://join?server=<ws(s)://host[:port][/path]>&file=…&token=…`
  for any endpoint (TLS, a path behind a proxy), or `ws(s)://…?file=…&token=…`. The desktop app
  can't open `wss://` yet (no TLS of its own); the browser can.
- **Managing files.** A connection that opens with `Manage {key}` instead of `Hello` sends
  `Admin` requests: publish a document (its snapshot) as a new file, which returns an edit link
  and a view link; list files and links; make and revoke links; remove a file. The server key is
  128 bits in the data directory. The server's command line (`figcraft-server files|links|key`)
  uses the same requests when a server is running on the directory, else it changes the directory
  itself (one process holds it at a time: an OS file lock).
- **Publishing from the app.** `collab.publish {server, name?}` (Share ▸ Publish to server…)
  uploads the open document over a managing connection, then joins the new file with its edit
  link on the same connection thread; the tab becomes the session's synced document (read-only
  until the welcome arrives, its undo history per person after) and `collab.peers` reports
  `published {server, file, name, edit, view}`.
- **Persistence.** Each open file's server keeps a journal of what changes its lasting state
  (`Server::journaling`: client ids handed out, accepted batches, refused batch ids); the store
  appends it to the file's log and syncs it once per burst of messages, before the hub sends what
  the server said, so a confirmed edit is never lost to a crash. A restart restores the last
  snapshot (`Server::restore`) and replays the log (`Server::replay`; entries already in the
  snapshot change nothing, a log that stops making sense is cut there). Compaction writes the
  next generation's snapshot (`Server::state`) and an empty log of that generation (a log of an
  older generation is from a compaction cut short and is ignored), after 1,000 entries or 16 MiB
  of log, when everyone leaves (the file is then closed) and when the server stops. Snapshots and
  logs carry CRC-32 checksums; writes go to a temporary file that is flushed and renamed.
  Damaged files are skipped and reported at startup, never deleted; a damaged log end is dropped.
- **Restarts.** A stopping server closes connections with code 1012 (service restart), which
  clients take as a dropped connection: they reconnect with their usual backoff and resume as the
  same clients (the session id, client ids and log come back with the file), sending what they
  made meanwhile. The app's session ending (code 1001) stays final.

*Built* (opening invite links):

- The desktop app takes an invite link on its command line (`figcraft "figcraft://join?…"`): once
  the window is up it joins (`collab.join`); a link it can't use opens the Join dialog with the
  link and the reason. Other arguments are files, as paths or `file://` URLs.
- **Windows**: the app registers itself per user as the opener of `figcraft://` under
  `HKEY_CURRENT_USER\Software\Classes\figcraft` (no administrator rights): the scheme, its icon
  and `"<figcraft.exe>" "%1"`. Preferences ▸ **Open invite links in FigCraft** (and
  `collab.registerLinks {on?}`) turns it on or off; it defaults to on for installed builds, which
  register on first run, and off for builds from source (inside a Cargo build directory, or debug
  builds), which leave any registration alone. Turning it off removes only this copy's
  registration. The registry sits behind a trait, so tests use a fake.
- **macOS**: the bundle declares the scheme (`CFBundleURLTypes`), so links open FigCraft; the
  link itself arrives as an Apple Event, which the windowing layer doesn't pass on yet, so the app
  opens without joining (paste the link) until it does.
- **Linux and FreeBSD**: the desktop entry declares `x-scheme-handler/figcraft` and passes URLs
  (`%U`); the app joins from its command line.
- An app that is already running doesn't receive the link: a second window starts and joins.

## Offline and reconnect

*Built* (`Client::{disconnect, reconnect}`, the server's log, the guest's connection in
`crates/collab-net`, the engine's live session). On a dropped connection the client keeps
editing locally, queues batches, and on reconnect sends `Hello` with its last confirmed `seq`; the
server replies with the missed changes, then the queued batches are sent (they may be partly
rejected if their objects were deleted meanwhile).

Decisions, as built:

- **Same identity.** `Hello.resume` carries the session id (random per server, kept in its saved
  state), the client id and the last sequence number applied. A client this session gave its id to
  keeps it (a stale connection with that id is taken over); anyone else joins as new. A client
  welcomed as new after all (the server restarted without its state) starts over from the
  snapshot and is told how many unsent changes couldn't be kept.
- **Catch-up.** The server keeps a log of accepted batches (`Limits::max_log_batches`, default
  10 000, and `max_log_changes`, default 1 000 000). If every batch after the client's sequence
  number is still in it, the welcome carries them (`CatchUp::Delta`); otherwise the whole document
  (`CatchUp::Snapshot`).
- **What happened to the batches in flight.** The server remembers per client the last batch id it
  processed and the latest refused ids (at most 1 024). Batches up to that id that the catch-up
  doesn't confirm were refused (the user is told); the others are sent again, in order, keeping
  the sequence number they were made on. Batches the server confirmed but whose echo was lost are
  confirmed by the catch-up and never sent twice.
- **Undo across a gap.** With a delta, the missed batches mark the steps they conflict with exactly
  as they would have live. With a snapshot, which batches came from others isn't known: a step
  keeps a value only if it is still the one the step wrote (someone else setting exactly the same
  value can't be told apart). An undo or redo lost in flight is made again from the caught-up
  state, treating what others wrote meanwhile as later than the step.
- **Over the network.** The guest's connection keeps what it needs to resume from the messages it
  reads (the welcome's session and client id, the last sequence number) and sends it in the next
  `Hello`; the engine takes the connection's "reconnecting" status as the client going offline and
  the next welcome as its catch-up, in order with the batches before it. What the engine sent for
  a dropped connection is not carried over (the client sends again what the server didn't get),
  the server ignores a batch id it already processed (a copy sent twice around a reconnect), and
  the host's hub hands a resumed client id from its stale connection to the new one.
- **Session state.** `Server::state` returns what a server needs to carry on after a restart
  (`SessionState`: snapshot, sequence number, session id, client-id counter, the clients that can
  reconnect and what was done with their batches, deleted ids, the log), as one serde value; the
  standalone server stores it as each file's snapshot, with a journal of the changes since
  (`Server::journaling`, `Server::replay`).

Tests: random sessions where clients drop and come back mid-session converge, with the default log
and with logs short enough that long absences fall back to snapshots; targeted tests cover a lost
echo, a batch refused while offline, a long absence, an unknown client and a restart from the
saved state.

## Agents, MCP and the CLI

Everything stays a command: `collab.host`, `collab.join`, `collab.publish`, `collab.leave`,
`collab.peers`, `collab.follow` are ordinary commands, so the CLI, the control channel and MCP can
host, publish and join sessions; an agent can be a participant with its own cursor.

*Built*: the desktop app (control channel) and the MCP server's headless session (`figcraft-cli
mcp`) both have the network link installed; `collab.join` returns at once with status
`"connecting"` and `collab.peers` tells when it is live (or why it failed) and who is typing in
which text layer. An agent's pointer is `collab.cursor {x, y}`. The one-shot CLI (`figcraft-cli run`) has no link: there the commands say
live sessions aren't available.

## Security

The server validates every change (types, ranges, the same caps the engine applies to commands,
tree rules, message size limits) and never trusts client-computed data. The server and client
parsers are fuzzed like the file importers. Sessions bind to all interfaces only when the user
chooses to share on the network; the default is the invite link with a random token.

*Built* (transport and host), and the threat model it assumes:

- **Who can connect.** By default the host listens on 127.0.0.1: only programs on the same
  machine can reach it. With "Share on the local network" it listens on the LAN interface's address
  (not on all interfaces), and anyone who can reach that address and port can open a connection;
  joining then needs the token. Untrusted networks (public Wi-Fi) are where not to turn it on; the
  popover says who can join.
- **The token.** 128 bits from the operating system's random number generator, URL-safe base64,
  new for every session (stopping and starting again invalidates old links). It is compared in
  constant time (the hub's backend). It is in the invite link by design, so whoever holds the link
  can join until the session stops: share it like a password. It never travels in the WebSocket
  request (the path is `/`), so proxies don't log it; FigCraft never logs it (logs name addresses
  and ports only), and errors about links never echo the link. Traffic is not encrypted (`ws://`):
  on a shared network, others on the path could read presence and the token. Use a trusted LAN
  or a VPN; TLS (`wss://`) comes from a reverse proxy in front of a `figcraft-server`.
- **Limits.** At most 64 open sockets (joined or not) and the core's 256 people; a connection must
  finish the WebSocket handshake and say hello within 10 s, and one silent for 45 s is dropped
  (clients ping every 15 s); client messages are at most 16 MiB (what the host sends keeps the
  core's 64 MiB cap: a welcome carries the whole document); a client sending more than 100
  messages a second on average (bursts of 400) is disconnected, and so is one whose 1,024-message
  outgoing queue fills (it can't keep up); names are cut to 64 characters without control
  characters; presence numbers must be finite and selections are cut to 10,000 ids.
- **Malformed input.** Anything that isn't a binary message of the current wire version that
  decodes in full is answered with `Refused` and a close; the first message must be `Hello` (for
  this file) or, on a server, `Manage`. The invite parser and the host's reader are fuzzed
  (`crates/collab-net`), and so are the server's readers of its own files and its port
  (`apps/figcraft-server`).

*Built* (the standalone server), and what it adds to the threat model:

- **Who can connect.** Whoever can reach the server's port (127.0.0.1 by default; the operator
  chooses an interface or a reverse proxy). Joining a file needs one of its link tokens; managing
  files needs the server key. There are no accounts: a link is a bearer secret for one file and
  one access level, so share it like a password and revoke it when it leaks; the server key is
  the administrator's and should stay with trusted people.
- **Secrets at rest.** Link tokens are kept only as SHA-256 hashes, so a copy of the data
  directory can't be used to join (the documents themselves are in it, unencrypted). The server
  key is plain text in the data directory (owner-only on Unix): reading the directory already
  gives every file. A lost link can't be shown again; revoke it and make a new one.
- **View access is enforced by the server**, not by the app: a viewer's batches are refused
  whatever client sends them; the app's read-only state is for clarity.
- **Abuse limits.** On top of the transport's: at most `max_files` files, `max_file_mb` per file
  (a publish over it is refused), `max_links` links per file, `max_people` per file and
  `max_connections` sockets.
- **TLS.** None in the server: a reverse proxy terminates it. Tokens and keys are never in the
  WebSocket request (only in the first message), so proxies don't log them, and the server logs
  file and link ids, never secrets.
- **Not yet**: accounts and per-person permissions, per-address rate limits, audit logs,
  encryption at rest.

## Implementation plan

1. **Core (no networking):** client-scoped ids, the object/property model over `Document`, diff
   after a command → `Changes`, apply `Changes`, fractional positions, server-side validation and
   ordering as a pure library (`crates/collab`, layer 2: it depends on the document model only;
   the engine depends on it for the recompute hook and `collab.diff`, which shows the batch an
   edit syncs as), and a randomized N-client convergence test with an in-memory transport.
   **Done.**
2. **Transport + embedded host:** WebSocket server/client (native), the binary encoding, the
   `collab.*` commands, Share / Join UI, live cursors, selections, avatars and follow.
   **Done** (`crates/collab-net`, layer 6 beside the UI: blocking `tungstenite` with a reader and
   a writer thread per connection, no async runtime; the engine holds the live state and the sync
   client and reaches the network only through the `LiveLink` the app installs).
3. **Per-user undo**, reconnect and offline queueing, rejection handling, presence for text
   editing. **Done** (the core's client and server, the engine's live session, the guest's
   resuming connection), followed by client-scoped ids for document-level objects and remote text
   carets on the canvas.
4. **Browser client** (WebSocket through `web-sys`). **Done** (the web build joins
   desktop-hosted sessions; `figcraft://` links open the desktop app and join).
5. **Standalone `figcraft-server`** with persistence, compaction, share links and permissions.
   **Done** (`apps/figcraft-server` on the shared hub; edit and view links, the server key,
   publishing from the app; see [`server.md`](server.md)).
6. **Accounts**: people signing in, per-person permissions, teams and projects; TLS in the
   desktop app.
