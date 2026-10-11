# Collaboration parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (wording: the standalone server with share links to edit or view; numbers unchanged) · **Target:** Figma Design (desktop app 126.10.7)

Figma is multiplayer first; FigCraft is local first. **Ready ~12% (estimated):** comments and version
history work inside one file; the real-time collaboration core landed in #103 (sync model,
server ordering, client reconciliation, convergence tests), and live sessions (preview) let
several apps co-edit a file hosted by one desktop app or kept on a self-hosted `figcraft-server`
with share links to edit or view, joined from the desktop app or the browser, with others'
cursors, selections and text carets shown live ([`collab-protocol.md`](collab-protocol.md), [`server.md`](server.md)). Remaining: 55–110 h
(multiplayer 45–90 h, shared libraries 10–20 h), plus branching 8–15 h.

| Feature | Figma | FigCraft | Hours |
|---|---|---|---|
| Real-time co-editing | yes | **core built, no networking** (#103): client-scoped ids, per-property change sets with fractional positions, server ordering and validation, optimistic clients, binary messages; randomized multi-client convergence tests over real commands. Live sessions over the network landed (`crates/collab-net`): the WebSocket transport (keep-alive, size and rate limits, reconnect with backoff), hosting from the app (this machine or the local network) with invite links, live cursors with name tags, selections, avatars and follow, `collab.*` commands. Document sync between apps works over them (the host's server orders every batch; guests open the host's file as a new tab). Undo is per person (only your own steps, others' later writes left alone), refused edits roll back with a toast, and a reconnect resumes as the same client with what it missed (or a snapshot) and sends what was made offline. The browser build joins (same messages through the browser's WebSocket API); `figcraft://` invite links open the desktop app and join. Styles, variables, collections, modes and comments made in a session take ids from their creator's space (the server checks it), so concurrent creations all survive; modes, per-mode values and replies inside one entry still resolve last-writer-wins. A standalone `figcraft-server` keeps many files, persists them (a snapshot plus a log, compacted), loses nothing confirmed across a restart (apps resume as the same participants) and takes files published from the app (Share ▸ Publish to server…). Left: TLS in the desktop app (browsers use `wss://` through a reverse proxy), packaging the server | 25–55 |
| Live cursors, selections, viewports; follow / observation mode; spotlight | yes | **partial, in live sessions**: named cursors, selection outlines, avatars, follow (click an avatar), others' text carets and selected characters with their names, a note when two people type in one layer; no spotlight | 8–15 |
| Cursor chat, audio, high-fives | yes | no | 4–8 |
| Share links, permissions (view / edit), teams and projects | yes | **partial**: `figcraft-server` share links per file with edit or view access (a viewer sees changes and presence, a "View only" badge and only the moving tools; the server refuses its edits), revocable at once, tokens stored as hashes; managed from the app's Share popover (first links), the server's command line or a managing connection. No accounts, teams or projects | 8–17 |
| Comments: pins on frames and layers, threads, reactions, @mentions, resolve, unread, search and filters, links, in the presenter | yes (shared) | **done, local**: stored in the file, your name from Preferences | (shared once multiplayer lands) |
| Version history: named versions, autosave checkpoints, view before restore, restore, duplicate | yes (cloud) | **done**, saved in the `.figcraft` file | — |
| Branching and merging | yes (paid plans) | no | 8–15 |
| Team / organization libraries | yes | local library files only ([components-parity](components-parity.md)) | 10–20 |
| Notifications, activity | yes | no | with sharing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Collaboration wording after the standalone server: `figcraft-server` keeps many files, persisted as a snapshot plus a log of what changed (compacted, checksummed, damaged files skipped and reported), restarts lose nothing confirmed and apps resume where they were; the app publishes its file to it; share links per file let people edit or only view (the server refuses a viewer's edits), are revocable and stored as hashes; TLS comes from a reverse proxy (the desktop app has none of its own yet); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Collaboration wording after the collaboration polish: styles, variables, collections, modes and comments created in a live session take ids from their creator's id space (concurrent creations no longer overwrite each other; the server checks it), others' text carets and selections show on the canvas, and typing in a layer someone else is typing in says so; percentages unchanged |
| 2026-10-11 | minor | Wording only: live sessions (#104), then the browser joining them and `figcraft://` invite links opening the desktop app; percentages and hours unchanged until the next re-measure |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
