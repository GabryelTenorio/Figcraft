# FigCraft roadmap: milestones and what's next

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created; milestones moved from ROADMAP.md, next steps re-planned from the 2026-10-10 re-measure) · **Target:** Figma Design (desktop app 126.10.7)

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
5. **Multiplayer M9** per [`collab-protocol.md`](collab-protocol.md): 60–110 h.

## Milestones

| Milestone | Status | Left | Hours |
|---|---|---|---|
| M0 Foundation: workspace, model, renderer, engine, UI shell, app, control channel | done (2026-10-09) | — | — |
| M0.9 CLI + MCP, web build, release packaging, PDF export | done (v0.2.0 released 2026-10-10 for every platform) | — | — |
| M1 Core editing completeness (tools, panels, menus, clipboard, files) | ~75% | Open recent, outline mode variants, keyboard box selection, interaction pass | 8–14 + pass |
| M2 Layout depth | ~80% | grid `fr` rows, real-file QA | 6–12 |
| M3 Components & styles depth | ~75% | extended collections, variables import/export, real libraries | 10–20 |
| M4 Vector & text depth | ~70% | live booleans, spell check, Figma Draw (brushes, pressure, text on a path, patterns) | 24–40 |
| M5 Prototype & present | ~85% | media, gamepad, separate window | 5–9 |
| M6 Interop & Dev Mode | ~55% | `.fig` import, video/HEIC/TIFF, CMYK intent, Code Connect | 40–80 |
| M7 Release & fidelity (signed builds, look & feel, shortcuts) | builds done; fidelity pass started | tool-by-tool pass | 40–60 |
| M8 Hardening & performance (bench, retained canvas, accessibility, idle) | ~60% | real-file corpus, platform QA, GPU canvas, screen readers | 60–110 |
| M9 Real-time collaboration | protocol designed | everything | 60–110 |
| M10 Localization (new) | not started | layer + 11 languages | 60–100 |
| M11 Plugins (new, owner decision) | not started | runtime, API, manager | 40–80 |
| M12 AI features (new, owner decision) | not started | local models | 30–60 |

## Next, after the current focus

- Shared libraries on top of the multiplayer server (10–20 h), branching (8–15 h).
- GPU canvas held to the CPU renderer (30–60 h).
- Figma Draw tools (10–18 h).
- Mobile viewer and prototype mirror (20–40 h).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created: milestone table moved from ROADMAP.md with statuses re-checked; added M10–M12; current focus re-planned |
