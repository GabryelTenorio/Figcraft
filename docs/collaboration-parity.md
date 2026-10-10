# Collaboration parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created; multiplayer status from `docs/collab-protocol.md`) · **Target:** Figma Design (desktop app 126.10.7)

Figma is multiplayer first; FigCraft is local first. **Ready ~8% (estimated):** comments and version
history work inside one file on one machine; real-time collaboration is designed
([`collab-protocol.md`](collab-protocol.md)) but not built. Remaining: 70–130 h (multiplayer
60–110 h, shared libraries 10–20 h), plus branching 8–15 h.

| Feature | Figma | FigCraft | Hours |
|---|---|---|---|
| Real-time co-editing | yes | **designed only**: WebSocket sessions hosted by the app or a `figcraft-server`, server-ordered per-property changes, last writer wins | 40–70 |
| Live cursors, selections, viewports; follow / observation mode; spotlight | yes | no (presence messages designed) | 8–15 |
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
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
