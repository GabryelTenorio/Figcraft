# FigCraft Roadmap

**Stage: alpha** · next: beta, ~28 points and ~200–365 h away (one Opus 5.5 agent; ~40–80 h wall-clock with 5 agents)

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (wording: the standalone server with share links to edit or view; numbers unchanged) · **Target:** Figma Design (desktop app 126.10.7)

FigCraft is a clean-room, open-source, pure-Rust reimplementation of the Figma design workflow,
native on macOS, Windows, Linux and FreeBSD and in the browser, fully drivable by agents through
the CLI, a JSON control channel and MCP. This page is the one-page summary; agents choosing work
start with [`docs/gaps.md`](docs/gaps.md).

## Headline numbers

| | Value | Kind |
|---|---|---|
| Menu/command breadth | **98.5%** (200 / 203) | measured, [`docs/parity-checklist.md`](docs/parity-checklist.md) (`cargo xtask parity`) |
| Feature breadth | **~78%** (core editor ~90%) | estimated, [`docs/target-app-parity.md`](docs/target-app-parity.md) |
| Ready for real work | **~47%** | estimated, same doc (written dimension weights and scores) |
| Essentials user | **~53%** | estimated: core features 77.8% × 0.92 launch/stability × 0.92 discoverability × 0.8 opening files people send ([method](docs/target-app-parity.md#essentials-user)) |
| Mainstream practitioner | **~43%** | estimated: weekly areas 66.5% × 0.9 interaction × 0.9 stability × 0.8 file exchange with Figma users ([method](docs/target-app-parity.md#mainstream-practitioner)) |
| Remaining to beta | **~200–365 h** one agent | estimated |
| Remaining to full parity | **~390–745 h** one agent (~95–170 h wall-clock with 5 agents) | estimated |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent wall-clock hours to ~95% | Work that dominates |
|---|---|---|---|
| Full target (ready for real work) | ~47% | ~375–705 h | multiplayer (45–90), localization (60–100), plugins (25–55), the interaction pass (40–60), `.fig` import (30–60), GPU canvas (30–60), AI (30–60) |
| Mainstream practitioner | ~43% | ~180–330 h | core editor depth incl. `.fig` import (98–188), the interaction pass (40–60), real-file corpus and platform QA (25–50), `.fig` export for handing work back (15–30) |
| Essentials user | ~53% | ~85–160 h | `.fig` import so files people send open (30–60), core tool polish (25–45), interaction pass on the core tools (15–25), stability QA (10–20), onboarding and starter templates (5–10) |

Hours are one agent working sequentially, calibrated from this repo's history (about 1 agent-hour
per 1,000 added lines, 1–2.5 h per feature PR, ~7 h per honest depth point in the polish tail; see
[how the hours are calibrated](docs/target-app-parity.md#how-the-hours-are-calibrated)). Each audience's work is a subset of
the next. About 70–80% parallelizes across 4–6 agents on disjoint crates (roughly ÷4 in
wall-clock); the `.fig` reader, the multiplayer core and a plugin runtime each need one owner, and
native-speaker review, real-file licensing and screen-reader QA need people.

Hours are Opus 5.5 agent wall-clock hours, calibrated from this repo's history: the app was built in
~25 wall-clock hours by 4–6 parallel agents (~110–150 agent-hours, ~1 agent-hour per 1,000 added
lines, a typical feature PR 1–2.5 h). Details and what needs a human:
[`docs/target-app-parity.md`](docs/target-app-parity.md#how-the-hours-are-calibrated).

## Stage

**Alpha.** Core workflows exist end to end (frames, shapes, vectors, text, auto layout, components
with variants and overrides, styles, variables, prototyping, Dev Mode, SVG/PNG/JPG/PDF), but
FigCraft can't open Figma's own `.fig` files, has only a preview of multiplayer, and nothing has been checked
against real Figma documents. Ready-for-real-work is ~47%, inside alpha's ~40–75% band, and the alpha gate passes: every daily core workflow (design screens, build a design system, prototype, hand off to developers) works end to end on FigCraft's own files, which save and reopen; review works only asynchronously and `.fig` import is missing, both beta gaps ([alpha gate](docs/roadmap.md#alpha-gate)). Beta needs
~75% **and** reliable `.fig` import: the `.fig` reader (30–60 h), a real-file corpus with measured
fidelity (15–30 h), the interaction pass (25–40 h of the 40–60), core depth to ~80% (40–75 h),
basic multiplayer on the landed core (30–55 h), stability and platform QA (20–40 h), a partial GPU path (15–30 h) and
the localization layer with Figma's own languages (20–35 h).

## By dimension

| Dimension | Ready | Hours | Doc |
|---|---|---|---|
| Features (depth) | 57% (core editor 67%) | 78–146 | [target-app-parity](docs/target-app-parity.md) |
| UI/UX fidelity | 58% | 40–60 | [ui-parity](docs/ui-parity.md) |
| File formats | 40% (no `.fig`) | 30–60 | [file-format-parity](docs/file-format-parity.md) |
| Collaboration | 12% (core in; live sessions with document sync and cursors, the browser joins, invite links open the app; a self-hosted server with edit and view links; preview) | 55–110 | [collaboration-parity](docs/collaboration-parity.md) |
| Stability | 55% | 20–40 | [gaps](docs/gaps.md) |
| Performance | 60% | 30–60 | [hardware-parity](docs/hardware-parity.md) |
| Hardware | 50% | 15–30 | [hardware-parity](docs/hardware-parity.md) |
| Localization | 10% (English only) | 60–100 | [localization-parity](docs/localization-parity.md) |
| Platforms | 75% | 10–20 | [hardware-parity](docs/hardware-parity.md) |
| Ecosystem / plugins | 20% (own WASM plugins) | 25–55 | [gaps](docs/gaps.md) |
| AI features | 0% | 30–60 | [gaps](docs/gaps.md) |

## Features

| Area | Breadth | Ready | Hours | Doc |
|---|---|---|---|---|
| Canvas & navigation | 95% | 72% | 3–6 | [ui-parity](docs/ui-parity.md) |
| Selection & transform | 95% | 68% | 6–10 | [ui-parity](docs/ui-parity.md) |
| Layers panel | 95% | 78% | 2–4 | [ui-parity](docs/ui-parity.md) |
| Shapes & vector | 92% | 66% | 8–14 | [target-app-parity](docs/target-app-parity.md) |
| Paint | 85% | 66% | 5–9 | [target-app-parity](docs/target-app-parity.md) |
| Strokes | 90% | 72% | 2–4 | [target-app-parity](docs/target-app-parity.md) |
| Effects | 100% | 72% | 3–6 | [target-app-parity](docs/target-app-parity.md) |
| Frames, groups, sections, masks | 95% | 75% | 2–4 | [target-app-parity](docs/target-app-parity.md) |
| Auto layout & constraints | 95% | 68% | 6–12 | [auto-layout-parity](docs/auto-layout-parity.md) |
| Text | 85% | 64% | 8–14 | [target-app-parity](docs/target-app-parity.md) |
| Components & variants | 95% | 68% | 6–12 | [components-parity](docs/components-parity.md) |
| Styles & variables | 85% | 70% | 4–8 | [components-parity](docs/components-parity.md) |
| Prototyping | 88% | 70% | 5–9 | [prototyping-parity](docs/prototyping-parity.md) |
| Import/export | 65% | 35% | 30–60 | [file-format-parity](docs/file-format-parity.md) |
| Dev Mode / inspect | 80% | 62% | 4–8 | [dev-mode-parity](docs/dev-mode-parity.md) |
| Comments, versions, misc | 80% | 58% | 4–8 | [collaboration-parity](docs/collaboration-parity.md) |
| Multiplayer & sharing | 15% | 5% | 45–90 | [collaboration-parity](docs/collaboration-parity.md) |
| Plugins & widgets | 35% | 20% | 25–55 | [plugins](docs/plugins.md) |
| Team libraries | 40% | 25% | 10–20 | [components-parity](docs/components-parity.md) |
| Figma Draw extras | 45% | 30% | 10–18 | [gaps](docs/gaps.md) |
| AI features | 0% | 0% | 30–60 | [gaps](docs/gaps.md) |

## Languages

| Language | Code | Strings | Status |
|---|---|---|---|
| English | en | 100% | full |
| Simplified Chinese | zh-Hans | 0% | none |
| Spanish | es | 0% | none |
| Hindi | hi | 0% | none |
| Arabic | ar | 0% | none |
| French | fr | 0% | none |
| Portuguese | pt | 0% | none |
| Indonesian | id | 0% | none |
| Japanese | ja | 0% | none |
| German | de | 0% | none |
| Korean | ko | 0% | none |
| Vietnamese | vi | 0% | none |

No other languages. Figma ships English, Japanese, Korean, French, German, Spanish and Portuguese
(Brazil); FigCraft has no localization layer yet. Document text in any script (bidi, Arabic
shaping, IME) already works. Detail: [`docs/localization-parity.md`](docs/localization-parity.md).

## Upcoming

Ranked; detail and milestones in [`docs/roadmap.md`](docs/roadmap.md).

1. `.fig` import, best effort, with a synthetic `.fig` corpus: 30–60 h.
2. Real-file corpus and measured fidelity: 15–30 h.
3. Tool-by-tool interaction pass against Figma: 40–60 h.
4. Localization layer, then ja, ko, es, pt-BR, fr, de: 35–55 h.
5. Multiplayer (M9): accounts, teams and TLS in the desktop app on top of the live sessions and the standalone server (transport, hosting, invite links that open the app, document sync, presence, per-user undo, reconnect with offline edits, the browser client, `figcraft-server` with persistence and share links to edit or view landed): 45–90 h.

## Progress log

- **2026-10-10** · WASM plugins (#101: sandboxed wasmi runtime, manifest permissions, Plugins menu,
  Run last, Manage, Rust SDK and two examples), docs refresh (#102), the multiplayer core without
  networking (#103: client-scoped ids, change sets, server ordering and validation, client
  reconciliation, convergence tests). Menu breadth 200/203; ready for real work ~47%.
- **2026-10-10** · Full re-measure against Figma desktop 126.10.7 under the craftrules progress-docs
  standard: ready for real work ~46%, feature breadth ~76%, stage alpha; progress docs split out
  (`docs/target-app-parity.md`, `gaps.md`, `roadmap.md`, `architecture.md`, the per-area parity
  docs); `docs/parity.md` renamed `docs/parity-checklist.md`.
- **2026-10-10** · Day 2 (PRs #1–#99): vector edit mode depth (transform box, point snapping, arc
  handles, shape builder, variable width), text depth (variable fonts, bidi, RTL names, style
  browser), auto layout nesting and keyboard reordering, slots and library variables, components
  at scale (2,000 instances), effects fidelity (glass, noise, texture, progressive blur),
  prototyping (conditionals, expressions, device frames, responsive presentation), REST import of
  prototypes and variables, SVG import fidelity, PDF print options, Dev Mode (code, annotations,
  ready for dev, compare changes), comments polish, version history, keyboard shortcut parity,
  accessibility, retained canvas and 50k-layer performance, no redraws while idle; v0.2.0
  released for every platform; multiplayer protocol designed.
- **2026-10-09** · Day 1: foundation (13 crates, layering, command registry, renderer, engine,
  UI3 frontend, control channel), CLI and MCP, web build, PDF export, packaging, first depth
  passes on text, components, prototyping and comments.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Collaboration wording after the standalone server: `figcraft-server` keeps many files, persisted as a snapshot plus a log of what changed (compacted, checksummed, damaged files skipped and reported), restarts lose nothing confirmed and apps resume where they were; the app publishes its file to it; share links per file let people edit or only view (the server refuses a viewer's edits), are revocable and stored as hashes; TLS comes from a reverse proxy (the desktop app has none of its own yet); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Collaboration wording after the collaboration polish: styles, variables, collections, modes and comments created in a live session take ids from their creator's id space (concurrent creations no longer overwrite each other; the server checks it), others' text carets and selections show on the canvas, and typing in a layer someone else is typing in says so; percentages unchanged |
| 2026-10-11 | minor | Wording only: live sessions (#104), then the browser joining them and `figcraft://` invite links opening the desktop app; percentages and hours unchanged until the next re-measure |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | minor | Readiness by audience with hours to ~95% (full ~400–750, mainstream ~180–330, essentials ~85–160) |
| 2026-10-10 | minor | Added essentials user (~53%) and mainstream practitioner (~43%); ready for real work unchanged at ~46% |
| 2026-10-10 | minor | Alpha gate (core workflows) checked under the revised stage rules: passes, stays alpha |
| 2026-10-10 | major | Rewritten to the craftrules progress-docs shape after a full re-measure; rubric, area narrative and inventory moved to docs/target-app-parity.md, gaps to docs/gaps.md, milestones to docs/roadmap.md. Depth ~79% (self-graded, 16 areas) → ready for real work ~46% (all dimensions); full parity 115–195 h → 420–790 h |
| 2026-10-10 | minor | Day-2 refreshes after each batch of PRs (depth 58% → 79%) |
| 2026-10-09 | major | First roadmap |
