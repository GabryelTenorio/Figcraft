# Collaboration parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (collaboration polish: collision-free ids, remote text carets) · **Target:** Figma Design (desktop app 126.10.7)

Figma is multiplayer first; FigCraft is local first. **Ready ~12% (estimated):** comments and version
history work inside one file; the real-time collaboration core landed in #103 (sync model,
server ordering, client reconciliation, convergence tests) and live sessions hosted from the app
let people on one network co-edit as a preview (no browser client, sharing or permissions yet;
[`collab-protocol.md`](collab-protocol.md)). Remaining: 55–110 h
(multiplayer 45–90 h, shared libraries 10–20 h), plus branching 8–15 h.

| Feature | Figma | FigCraft | Hours |
|---|---|---|---|
| Real-time co-editing | yes | **core built, no networking** (#103): client-scoped ids, per-property change sets with fractional positions, server ordering and validation, optimistic clients, binary messages; randomized multi-client convergence tests over real commands. Live sessions over the network landed (`crates/collab-net`): the WebSocket transport (keep-alive, size and rate limits, reconnect with backoff), hosting from the app (this machine or the local network) with invite links, live cursors with name tags, selections, avatars and follow, `collab.*` commands. Document sync between apps works over them (the host's server orders every batch; guests open the host's file as a new tab). Undo is per person (only your own steps, others' later writes left alone), refused edits roll back with a toast, and a reconnect resumes as the same client with what it missed (or a snapshot) and sends what was made offline. Styles, variables, collections, modes and comments made in a session take ids from their creator's space (the server checks it), so concurrent creations all survive; modes, per-mode values and replies inside one entry still resolve last-writer-wins. Left: browser client, standalone server | 25–55 |
| Live cursors, selections, viewports; follow / observation mode; spotlight | yes | **partial, in live sessions**: named cursors, selection outlines, avatars, follow (click an avatar), others' text carets and selected characters with their names, a note when two people type in one layer; no spotlight | 8–15 |
| Cursor chat, audio, high-fives | yes | no | 4–8 |
| Share links, permissions (view / edit), teams and projects | yes | no | 8–17 |
| Comments: pins on frames and layers, threads, reactions, @mentions, resolve, unread, search and filters, links, in the presenter | yes (shared) | **done, local**: stored in the file, your name from Preferences | (shared once multiplayer lands) |
| Version history: named versions, autosave checkpoints, view before restore, restore, duplicate | yes (cloud) | **done**, saved in the `.figcraft` file | — |
| Branching and merging | yes (paid plans) | no | 8–15 |
| Team / organization libraries | yes | local library files only ([components-parity](components-parity.md)) | 10–20 |
| Notifications, activity | yes | no | with sharing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Collaboration wording after the collaboration polish: styles, variables, collections, modes and comments created in a live session take ids from their creator's id space (concurrent creations no longer overwrite each other; the server checks it), others' text carets and selections show on the canvas, and typing in a layer someone else is typing in says so; percentages unchanged |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
