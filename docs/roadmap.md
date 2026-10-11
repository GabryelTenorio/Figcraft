# FigCraft roadmap: milestones and what's next

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (spell check while editing text on Windows and macOS; menu breadth 201/204; the standalone server with share links to edit or view, wording only; live boolean editing and the vector tail: feature breadth ~79%) · **Target:** Figma Design (desktop app 126.10.7)

Forward-looking plan. Where FigCraft stands today: [`ROADMAP.md`](../ROADMAP.md) and
[`target-app-parity.md`](target-app-parity.md); the full ranked list: [`gaps.md`](gaps.md). Hours
are one Opus 5.5 agent; most milestones split across 4–6 agents on disjoint crates. There is no
`cargo xtask scorecard` in this repo; the measured number is `cargo xtask parity`.

## Current focus

1. **`.fig` import** (best effort) with a synthetic `.fig` corpus saved from the installed Figma:
   30–60 h. Unblocks beta and every real-file measurement.
2. **Real-file corpus and fidelity measurement**: import real files, compare against Figma's own
   exports, turn estimated percentages into measured ones: 15–30 h.
3. **Tool-by-tool interaction pass** against the running Figma ([`ui-parity.md`](ui-parity.md)):
   40–60 h, parallel by tool.
4. **Localization layer**, then Figma's own languages first (ja, ko, es, pt-BR, fr, de): 15–25 h
   for the layer, 3–5 h per language plus native review.
5. **Multiplayer M9**: accounts, teams and TLS in the desktop app on top of the live sessions and the standalone server (the WebSocket transport, hosting from the app, invite links that open the app, document sync, live cursors, selections, text carets, avatars and follow, per-user undo, reconnect with offline edits, collision-free ids for styles, variables, collections and comments, joining from the browser, and `figcraft-server` with persistence, publishing from the app and share links to edit or view landed), per [`collab-protocol.md`](collab-protocol.md) and [`server.md`](server.md): 45–90 h.

## Alpha gate

The core workflows a Figma Design user runs every day, checked end to end on macOS (the main
platform) with work saved to `.figcraft` and reopened. Evidence is the repo's tests and the
builders' side-by-side checks with synthetic files; nothing has been run on real Figma files yet.

| Workflow | Works end to end? | Evidence | Hours to pass |
|---|---|---|---|
| Design screens: frames, shapes, vectors, text, auto layout, styles; save, reopen, export assets | yes | tools and panels in `crates/engine` and `crates/ui-egui`; `.figcraft` round trips (`crates/format/src/large_tests.rs`); PNG/JPG/SVG/PDF export tests | — |
| Build and use a design system: components, variants, properties, variables and modes, libraries | yes | `components_scale_tests.rs` (300 components, 2,000 instances), `library_tests.rs`, `library_variables_tests.rs`; libraries are local files, not shared | — |
| Prototype and present: interactions, flows, overlays, smart animate, variables | yes | `proto_*_tests.rs`, `present_*_tests.rs` | — |
| Hand off to developers: Dev Mode inspect, code, measurements, asset export, ready for dev | yes | `devinspect_tests.rs`, `component_codegen_tests.rs`, `devcompare_tests.rs`, `annotations_tests.rs` | — |
| Review with others: comments, versions, sharing the work | partial (not blocking) | comments with threads, mentions and resolve plus version history work inside a file (`comments_tests.rs`, `version_tests.rs`); the file is shared by passing it along; live sessions (preview: hosted from the desktop app or a self-hosted `figcraft-server` with edit and view links, joined from the app or the browser, undo per person); no accounts or teams ([collaboration-parity](collaboration-parity.md)) | beta gap: 45–90 |
| Continue existing Figma work (open `.fig`) | no (beta gap) | no `.fig` reader; REST JSON import works for files fetched through Figma's API ([file-format-parity](file-format-parity.md)) | beta gap: 30–60 |

**Passes, so FigCraft stays alpha:** every daily design workflow works end to end on FigCraft's
own files, which save and reopen, and ready for real work (~47%) is above the ~40% bar. The two
gaps are real but don't break a workflow done in FigCraft: review works asynchronously through
comments in a shared file, and opening `.fig` is the step that moves work across from Figma, which
the standard requires for beta, not alpha. Both stay at the top of [`gaps.md`](gaps.md) as beta
blockers.

## Milestones

| Milestone | Status | Left | Hours |
|---|---|---|---|
| M0 Foundation: workspace, model, renderer, engine, UI shell, app, control channel | done (2026-10-09) | — | — |
| M0.9 CLI + MCP, web build, release packaging, PDF export | done (v0.2.0 released 2026-10-10 for every platform) | — | — |
| M1 Core editing completeness (tools, panels, menus, clipboard, files) | ~75% | Open recent, outline mode variants, keyboard box selection, interaction pass | 8–14 + pass |
| M2 Layout depth | ~80% | grid `fr` rows, real-file QA | 6–12 |
| M3 Components & styles depth | ~75% | extended collections, variables import/export, real libraries | 10–20 |
| M4 Vector & text depth | ~75% (live booleans, arc fields, shape builder on strokes, one-sided widths landed 2026-10-11) | spell check on Linux and the web, Figma Draw (brushes, pressure, text on a path, patterns), boolean hit testing on the result | 18–32 |
| M5 Prototype & present | ~85% | media, gamepad, separate window | 5–9 |
| M6 Interop & Dev Mode | ~55% | `.fig` import, video/HEIC/TIFF, CMYK intent, Code Connect | 40–80 |
| M7 Release & fidelity (signed builds, look & feel, shortcuts) | builds done; fidelity pass started | tool-by-tool pass | 40–60 |
| M8 Hardening & performance (bench, retained canvas, accessibility, idle) | ~60% | real-file corpus, platform QA, GPU canvas, screen readers | 60–110 |
| M9 Real-time collaboration | core done (#103: ids, change sets, server ordering, convergence); live sessions (WebSocket transport with limits and reconnect, hosting from the app, invite links, document sync between apps, cursors, selections, avatars, follow, `collab.*` commands); per-user undo, rejection handling, offline queue and resuming after a reconnect; styles, variables, collections, modes and comments created in a session get ids from their creator's space; others' text carets and selections on the canvas; the browser client (joins desktop-hosted sessions); `figcraft://` links open the app and join (Windows registers per user; macOS and Linux packages declare the scheme); the standalone `figcraft-server` (persistence with compaction, restarts that lose nothing confirmed, publishing from the app, share links to edit or view, revocable) | accounts, teams, TLS in the desktop app, packaging the server | 45–90 |
| M10 Localization (new) | not started | layer + 11 languages | 60–100 |
| M11 Plugins | runtime, API, menu, manager and SDK done (#101) | run-time plugin UI, settings storage, distribution, widgets; Figma plugin compatibility is an owner decision | 25–55 |
| M12 AI features (new, owner decision) | not started | local models | 30–60 |

## Next, after the current focus

- Shared libraries on top of the multiplayer server (10–20 h), branching (8–15 h).
- GPU canvas held to the CPU renderer (30–60 h).
- Figma Draw tools (10–18 h).
- Mobile viewer and prototype mirror (20–40 h).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Spell check while editing text: misspelled words underlined, suggestions, Ignore and Add to dictionary in the context menu, Text ▸ Spelling ▸ Check spelling (the system's checker on Windows and macOS, in the system's language; none on Linux, FreeBSD or the web yet). Text 85% → 88% breadth, 64% → 66% ready, 8–14 h → 5–10 h; menu breadth 200/203 → 201/204 (98.5%); gap 11 narrowed to the remaining platforms and languages (4–8 h → 2–5 h); weighted percentages unchanged, hour totals down 3–4 h (headline ranges unchanged within their rounding) |
| 2026-10-11 | minor | Collaboration wording after the standalone server: `figcraft-server` keeps many files, persisted as a snapshot plus a log of what changed (compacted, checksummed, damaged files skipped and reported), restarts lose nothing confirmed and apps resume where they were; the app publishes its file to it; share links per file let people edit or only view (the server refuses a viewer's edits), are revocable and stored as hashes; TLS comes from a reverse proxy (the desktop app has none of its own yet); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Live boolean editing, Design panel arc fields, the shape builder on strokes and open paths and one-sided variable widths landed (gap 13): Shapes & vector breadth 92% → 97%, ready 66% → 68%, 8–14 h → 1–3 h; feature breadth ~78% → ~79%; ready for real work unchanged (~47%); full parity ~390–745 h → ~385–730 h |
| 2026-10-11 | minor | Collaboration wording after the collaboration polish: styles, variables, collections, modes and comments created in a live session take ids from their creator's id space (concurrent creations no longer overwrite each other; the server checks it), others' text carets and selections show on the canvas, and typing in a layer someone else is typing in says so; percentages unchanged |
| 2026-10-11 | minor | Wording only: live sessions (#104), then the browser joining them and `figcraft://` invite links opening the desktop app; percentages and hours unchanged until the next re-measure |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | minor | Added the alpha gate (core-workflow table): passes, stage stays alpha |
| 2026-10-10 | major | Created: milestone table moved from ROADMAP.md with statuses re-checked; added M10–M12; current focus re-planned |
