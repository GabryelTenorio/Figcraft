# FigCraft Roadmap

**Stage: alpha** · next: beta, ~26 points and ~170–305 h away (one Opus 5.5 agent; ~35–70 h wall-clock with 5 agents)

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-12 · **Change:** minor (extended collections tail; branch version history and finer list merges; headline percentages recomputed from the area table: feature breadth ~84%, core ~94%, mainstream ~50%; text lists, hanging punctuation and decoration styles; comments and version history behaviour suite and fixes; canvas navigation, guides and grids behaviour suite and fixes; export and inspect behaviour suite and fixes; .fig import: editable vector networks and more layer kinds; shapes and vector behaviour suite and fixes; paint and effects behaviour suite and fixes; frames, groups, sections and masks behaviour suite and fixes; styles and variables behaviour suite and fixes; prototyping behaviour suite and fixes; components behaviour suite and fixes; embedded image colour profiles; layers panel keyboard: F2 rename, Shift+F10 context menu, keyboard box selection, 10k-layer QA; per-side stroke weight variables; text behaviour suite: CJK line breaking, metrics, vertical trim; selection and transform behaviour suite and fixes; `.fig` import, best effort; Windows QA pass with eight fixes; auto layout behaviour suite and fixes; Display P3: gamut warning and soft-proof (true P3 blocked upstream); several windows; localization layer with a draft Spanish catalog; hosting figcraft-server: wss in the desktop app, server releases, deployment guide; plugin panels and settings; canvas fixes: boolean hit testing on the result, group drags, frame outlines and slices; code connect; stretch and scatter brushes, pen pressure; text on a path and pattern fills; team libraries on the server and swap library; branches: create, update from main, review and merge; animated GIF fills and TIFF images; Open recent, outlines for the selection and mask outlines: menu breadth 100%; extended collections, variables import/export, gradient stop and variant bindings; feature breadth ~80%; ready ~49%) · **Target:** Figma Design (desktop app 126.10.7)

FigCraft is a clean-room, open-source, pure-Rust reimplementation of the Figma design workflow,
native on macOS, Windows, Linux and FreeBSD and in the browser, fully drivable by agents through
the CLI, a JSON control channel and MCP. This page is the one-page summary; agents choosing work
start with [`docs/gaps.md`](docs/gaps.md).

## Headline numbers

| | Value | Kind |
|---|---|---|
| Menu/command breadth | **100%** (204 / 204) | measured, [`docs/parity-checklist.md`](docs/parity-checklist.md) (`cargo xtask parity`) |
| Feature breadth | **~84%** (core editor ~94%) | estimated, [`docs/target-app-parity.md`](docs/target-app-parity.md) |
| Ready for real work | **~51%** | estimated, same doc (written dimension weights and scores) |
| Essentials user | **~57%** | estimated: core features 77.8% × 0.92 launch/stability × 0.92 discoverability × 0.88 opening files people send ([method](docs/target-app-parity.md#essentials-user)) |
| Mainstream practitioner | **~50%** | estimated: weekly areas 72.3% × 0.9 interaction × 0.9 stability × 0.85 file exchange with Figma users ([method](docs/target-app-parity.md#mainstream-practitioner)) |
| Remaining to beta | **~170–305 h** one agent | estimated |
| Remaining to full parity | **~302–589 h** one agent (~86–150 h wall-clock with 5 agents) | estimated |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent wall-clock hours to ~95% | Work that dominates |
|---|---|---|---|
| Full target (ready for real work) | ~51% | ~296–569 h | multiplayer (45–90), localization (45–75), plugins (15–40), the interaction pass (40–60), GPU canvas (30–60), AI (30–60), the `.fig` import's tail (5–10) |
| Mainstream practitioner | ~50% | ~131–245 h | core editor depth incl. the `.fig` import's tail (49–98), the interaction pass (40–60), real-file corpus and platform QA (25–50), `.fig` export for handing work back (15–30) |
| Essentials user | ~57% | ~60–110 h | the `.fig` import's tail so the files people send open well (5–10), core tool polish (25–45), interaction pass on the core tools (15–25), stability QA (10–20), onboarding and starter templates (5–10) |

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
FigCraft opens `.fig` files only best effort, has only a preview of multiplayer, and nothing has been checked
against real Figma documents. Ready-for-real-work is ~51%, inside alpha's ~40–75% band, and the alpha gate passes: every daily core workflow (design screens, build a design system, prototype, hand off to developers) works end to end on FigCraft's own files, which save and reopen; review works only asynchronously and `.fig` import is unproven on real files, both beta gaps ([alpha gate](docs/roadmap.md#alpha-gate)). Beta needs
~75% **and** reliable `.fig` import: the `.fig` import's tail (5–10 h), a real-file corpus with measured
fidelity (15–30 h), the interaction pass (25–40 h of the 40–60), core depth to ~80% (40–75 h),
basic multiplayer on the landed core (30–55 h), stability and platform QA (20–40 h), a partial GPU path (15–30 h) and
Figma's own languages on the landed localization layer (15–25 h).

## By dimension

| Dimension | Ready | Hours | Doc |
|---|---|---|---|
| Features (depth) | 61% (core editor 72%) | 42–88 | [target-app-parity](docs/target-app-parity.md) |
| UI/UX fidelity | 58% | 40–60 | [ui-parity](docs/ui-parity.md) |
| File formats | 59% (`.fig` read best effort with editable vectors, no write) | 5–10 | [file-format-parity](docs/file-format-parity.md) |
| Collaboration | 13% (core in; live sessions with document sync, cursors and text carets, the browser joins, invite links open the app; a self-hosted server with edit and view links and team libraries; preview) | 49–98 | [collaboration-parity](docs/collaboration-parity.md) |
| Stability | 55% | 20–40 | [gaps](docs/gaps.md) |
| Performance | 60% | 30–60 | [hardware-parity](docs/hardware-parity.md) |
| Hardware | 60% | 10–21 | [hardware-parity](docs/hardware-parity.md) |
| Localization | 20% (layer; English, Spanish draft) | 45–75 | [localization-parity](docs/localization-parity.md) |
| Platforms | 75% | 10–20 | [hardware-parity](docs/hardware-parity.md) |
| Ecosystem / plugins | 28% (own WASM plugins with panels and settings) | 15–40 | [gaps](docs/gaps.md) |
| AI features | 0% | 30–60 | [gaps](docs/gaps.md) |

## Features

| Area | Breadth | Ready | Hours | Doc |
|---|---|---|---|---|
| Canvas & navigation | 98% | 74% | 1–4 | [ui-parity](docs/ui-parity.md) |
| Selection & transform | 97% | 72% | 3–5 | [ui-parity](docs/ui-parity.md) |
| Layers panel | 98% | 82% | 1–2 | [ui-parity](docs/ui-parity.md) |
| Shapes & vector | 98% | 72% | 2–4 | [target-app-parity](docs/target-app-parity.md) |
| Paint | 90% | 71% | 2–5 | [target-app-parity](docs/target-app-parity.md) |
| Strokes | 95% | 75% | 1–2 | [target-app-parity](docs/target-app-parity.md) |
| Effects | 100% | 75% | 2–5 | [target-app-parity](docs/target-app-parity.md) |
| Frames, groups, sections, masks | 97% | 80% | 1–3 | [target-app-parity](docs/target-app-parity.md) |
| Auto layout & constraints | 98% | 74% | 3–6 | [auto-layout-parity](docs/auto-layout-parity.md) |
| Text | 90% | 70% | 3–7 | [target-app-parity](docs/target-app-parity.md) |
| Components & variants | 96% | 72% | 4–9 | [components-parity](docs/components-parity.md) |
| Styles & variables | 95% | 76% | 2–4 | [components-parity](docs/components-parity.md) |
| Prototyping | 90% | 74% | 3–6 | [prototyping-parity](docs/prototyping-parity.md) |
| Import/export | 84% | 57% | 5–10 | [file-format-parity](docs/file-format-parity.md) |
| Dev Mode / inspect | 93% | 70% | 1–3 | [dev-mode-parity](docs/dev-mode-parity.md) |
| Comments, versions, misc | 87% | 63% | 1–3 | [collaboration-parity](docs/collaboration-parity.md) |
| Multiplayer & sharing | 15% | 5% | 45–90 | [collaboration-parity](docs/collaboration-parity.md) |
| Plugins & widgets | 45% | 28% | 15–40 | [plugins](docs/plugins.md) |
| Team libraries | 70% | 40% | 4–8 | [components-parity](docs/components-parity.md) |
| Figma Draw extras | 80% | 55% | 2–5 | [gaps](docs/gaps.md) |
| AI features | 0% | 0% | 30–60 | [gaps](docs/gaps.md) |

## Languages

| Language | Code | Strings | Status |
|---|---|---|---|
| English | en | 100% | full |
| Simplified Chinese | zh-Hans | 0% | none |
| Spanish | es | 100% | draft (needs native review) |
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
(Brazil); FigCraft has the localization layer, English and a Spanish draft (Preferences ▸ Language;
adding a language: [`docs/translating.md`](docs/translating.md)). Document text in any script (bidi, Arabic
shaping, IME) already works. Detail: [`docs/localization-parity.md`](docs/localization-parity.md).

## Upcoming

Ranked; detail and milestones in [`docs/roadmap.md`](docs/roadmap.md).

1. `.fig` import's tail: fixes from real files, connector labels, table cells, filled areas with their own fills (the best-effort import, editable vector networks, the other layer kinds and variable prototype actions landed 2026-10-11): 5–10 h.
2. Real-file corpus and measured fidelity: 15–30 h.
3. Tool-by-tool interaction pass against Figma: 40–60 h.
4. Languages on the landed localization layer: ja, ko, pt-BR, fr, de, and a native review of es: 15–30 h.
5. Multiplayer (M9): accounts and teams on top of the live sessions and the standalone server (transport, hosting, invite links that open the app, document sync, presence with remote text carets, per-user undo, reconnect with offline edits, collision-free ids for styles, variables and comments, the browser client, `figcraft-server` with persistence and share links to edit or view, team libraries on the server landed): 45–90 h.

## Progress log

- **2026-10-11** · Canvas navigation, guides and grids behaviour suite (gap 4): zoom, panning, the pixel
  grid (now from 400%), rulers, guides (top-level frames only; select, Delete, nudge, Remove guide,
  Alt-drag copies, Alt measures while dragging), layout grids (Auto counts; hidden on rotated frames),
  outline mode (clicks reach layers behind) and measuring, each rule from the documentation.
- **2026-10-11** · Export and inspect behaviour suite (gap 4): exports cover what a layer paints (shadows
  only where they fall, true stroke widths, no room for hidden layers, a mask group's mask), slices export what's under
  them, layer names with slashes export into folders; export settings gained contents only, include bounding box and
  the SVG options (outline text, ids, simplified strokes); Inspect shows colours in hex, RGB, CSS, HSL or HSB, CSS in
  rem, blend modes and positions from the frame, and lists icons and images as assets. Dev Mode 70%.
- **2026-10-11** · `.fig` import's tail (gap 1): vectors open as editable vector networks with their
  corner radii, handle mirroring and end caps (outlines when the network is missing or damaged); sticky notes, shapes
  with text, connectors, tables and similar layers import as frames, text and vectors instead of being dropped;
  set-variable and conditional prototype actions come across. File formats 58%, ready ~49%.
- **2026-10-11** · Paint and effects behaviour suite: every documented rule for fills, blend modes,
  gradients, images, colour models, shadows, blurs and effect exports is a table-driven case. Fixes: Normal
  groups isolate and clipping frames don't, plus darker is drawn, drop shadows hide behind translucent
  layers in every export, Fill images reach the box's edges, "Normal" no longer means pass-through.
  Paint 70%, effects 75%.
- **2026-10-11** · `.fig` import, best effort (gap 1): `file.open`, `file.import`, drag and drop, the CLI and
  MCP open `.fig` files. A generic kiwi decoder reads each file's own schema, and the decoded nodes map by field name
  onto the REST importer: layers, paints and styles, text runs, vectors as outlines, auto layout, components and
  instances (expanded with their overrides and property values), variables, prototyping and images from the archive;
  the rest is a warning. File formats 55%, ready ~49%.
- **2026-10-11** · Gamut warnings (gap 18): the colour picker of a Display P3 file flags colours
  outside sRGB and View ▸ Soft-proof sRGB stripes them on the canvas. Showing P3 itself waits on
  eframe/egui-wgpu exposing the surface colour space wgpu 30 already supports.
- **2026-10-11** · Several windows (gap 18): File ▸ New window, Open in new window on a tab or a
  tab dragged off the strip; each window has its own tabs, panels and canvas, draws only when it
  needs to and asks about unsaved changes when it closes (the main window too); the control channel
  and MCP open, list, focus and capture windows. Hardware 60%.
- **2026-10-11** · Hosting `figcraft-server`: the desktop app and `figcraft-cli` open `wss://` links (rustls,
  certificates always verified; `FIGCRAFT_CA_FILE` for a private authority), releases ship the server for Linux
  x86_64 and aarch64, it stops gracefully on SIGTERM or Ctrl+C, and `docs/server.md` walks through deploying it on
  a rented Linux server (systemd unit, Caddy or nginx, firewall, Docker, upgrades, backups).
- **2026-10-11** · Plugin panels and settings (gap 6, ABI v2; v1 plugins run unchanged): a plugin shows a panel of
  fields and buttons drawn with FigCraft's widgets and is called back for each button or changed value (one undo step
  per callback, same sandbox limits), `plugin.ui` / `plugin.event` / `plugin.close` drive panels from scripts and MCP,
  and per-plugin settings are kept with the preferences; Swatch grid gained a palette panel. Plugins & widgets 45%.
- **2026-10-11** · Canvas fixes: clicks hit boolean groups where their result is (a click in a
  subtracted hole reaches what's behind), a layer dragged inside a group stays in it, View ▸ Frame
  outlines and View ▸ Slices draw, and a flaky idle test now waits for the render thread (gap 13).
- **2026-10-11** · Team libraries on `figcraft-server` (gap 10): Assets ▸ Libraries ▸ Publish to server keeps
  the file's library on the server under the file (per-asset versions; the server's version is followed); other files
  enable it by pasting a share link (a view link is enough), get "Library updates available" from a background check
  and accept with overrides kept; the server counts which files use which components (Show usage, `library.usage`);
  `library.swap` re-points instances, styles and variables to another library by names. Team libraries 70%.
- **2026-10-11** · Animated GIF fills (first frame on the canvas and in exports, played in the
  presenter and the preview, waking the window only for the next frame) and TIFF images (gap 12).
- **2026-10-11** · The last three menu items got commands: File ▸ Open recent (kept in the
  preferences, missing files greyed, Clear recent), outlines for the selection and mask outlines
  (gap 21). Menu breadth 203/203.
- **2026-10-11** · Extended variable collections (a library collection's modes and variables in a
  collection of the file, overridden per mode with reset, following library updates), variables
  import/export (the REST local variables JSON, merged by name; CSS custom properties), gradient
  stops and variant properties bound to variables (gap 14).
- **2026-10-11** · Live boolean editing: operands stay editable layers (double-click or Cmd/Ctrl-click
  selects one, moving, resizing or editing its points redraws the result during the drag with every
  operand outlined, per-operand operations from the boolean controls, prefix-cached recompute);
  Design panel arc fields; the shape builder on strokes and open paths; one-sided variable widths
  (gap 13). Feature breadth ~79%.
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
| 2026-10-12 | minor | Extended collections' tail (gap 14): variables a library adds to a collection this file extends now count as a library update of their own (listed by name without a copy yet) and arrive inheriting when accepted, instead of waiting for the library's next other update; a library collection that is itself an extended collection can be extended again, values flowing down the chain as each file accepts updates with every level's overrides kept (checked end to end). Styles & variables breadth 94% → 95%, ready unchanged (76%), 3–5 h → 2–4 h; gap 14 1–2 h → 0–1 h (real-library checks left); feature breadth (~82%) and ready for real work (~51%) unchanged at this rounding; full parity ~305–594 h → ~304–593 h |
| 2026-10-12 | minor | A branch's own version history and finer list merges (gap 17): versions saved or autosaved on a branch tab go into the branch (viewed, restored, renamed, deleted and duplicated there, saved in the file with it); merging adds a version to the main file's history and the merged or archived branch keeps its own; fills, strokes and effects merge item by item, conflicting only when the same item changed both ways, an item was removed on one side or both sides added different items. Comments, versions, find, misc breadth 85% → 87%, ready 60% → 61%, 2–5 h → 1–4 h; gap 17 1–3 h → 1–2 h; feature breadth (~82%) and ready for real work (~51%) unchanged at this rounding; full parity ~306–595 h → ~305–594 h (the Features table here still showed 80%/58%/4–8 h from before branches landed) |
| 2026-10-12 | minor | Headline percentages recomputed from the area table by its own method (weights × written scores): feature breadth ~82% → ~84% (core editor ~93% → ~94%), weighted ready ~61% → ~63%, core ready ~70% → ~72%, mainstream weekly areas 71.3% → 72.3% and mainstream ~49% → ~50%; several earlier rows had moved the area scores without carrying them into the totals. Hours are unchanged (left for the next full re-measure) |
| 2026-10-12 | minor | Text lists, hanging punctuation and decoration styles, with a behaviour suite written from the public documentation (7 layout tests and 7 command tests, about 60 table rows): lists are set per paragraph (the list buttons, shortcuts and Type settings act on the paragraphs the selection touches, the whole layer when it isn't being edited), nest up to 8 levels (Tab and Shift+Tab; numbers go 1. a. i., bullets • ◦ ▪, counted per level), Enter on an empty item and Backspace at an item's start step out a level and then out of the list, typing `- `, `* ` or `1. ` at a paragraph's start starts one; list spacing (between items, in place of paragraph spacing), hanging lists (markers outside the box) and hanging punctuation (opening punctuation outside a start-aligned edge, closing punctuation outside an end-aligned one, taking no room when lines wrap); underline and strikethrough style (solid, dotted, wavy), thickness, underline offset, skip ink and colour. Canvas, PNG, SVG and PDF; `node.set` keys (`indentation`, `listSpacing`, `hangingPunctuation`, `hangingList`, `textDecorationStyle`/`Thickness`/`Offset`/`SkipInk`/`Color`), inspect (`lineTypes`, `lineIndentations`), CSS (`text-decoration-style`/`-thickness`/`-color`, `text-underline-offset`, `text-decoration-skip-ink`) and the REST import (`lineTypes` with `lineIndentations` per paragraph, which collapsed to one list style before). Text 87%/68% → 90%/70%, 5–10 h → 3–7 h; mainstream weekly areas 71.0% → 71.2%; other percentages unchanged at this rounding; full parity ~306–595 h → ~304–592 h |
| 2026-10-12 | minor | Comments and version history behaviour suite written from the public documentation (15 tests, about 80 table rows plus stepwise checks; 4 tests failed before the fixes) and its fixes: in a live session, threads made in the session couldn't be replied to, edited, reacted to, resolved, moved, deleted or opened (their ids, from their creator's id space, were refused as out of range); restoring a version brought back that version's comments, losing newer threads and replies (comments now stay, those on layers the version lacks staying where they were); restoring now keeps the replaced design in the history as an autosave first, unless a version of it was just saved or restored; giving an autosave a title makes it a named version (it could be dropped to make room); merging a branch adds the main file as it was and then a `Merged “<branch>”` version to its history. Pinned where the documentation is silent: pins go to the innermost frame under them, copies, duplicates and duplicated pages come without comments, replying reopens a resolved thread, versions stay with whoever saved them in a live session (a shared history needs accounts). Left: a branch's own version history, comparing two versions. Comments, versions, misc 85%/60% → 85%/62%, 2–5 h → 2–4 h (this table still showed 80%/58%/4–8 h from before branches landed); percentages unchanged at this rounding; full parity ~306–595 h → ~306–594 h |
| 2026-10-11 | minor | Canvas navigation, rulers, guides, layout grids, outline mode and measuring behaviour suite written from the public documentation (18 tests, about 120 table rows and checks) and its fixes: the pixel grid shows from 400% (it waited for 800%); guides belong to the page or a top-level frame (a guide dropped over a nested frame goes to the frame holding it, and a frame moved into another keeps its guides but they stop showing, snapping and measuring); a guide can be selected, deleted with Delete, nudged with the arrows and removed from the context menu (`guide.select`, `guide.remove` without an index); Alt-dragging a guide drops a copy and holding Alt while dragging one measures to the nearest edges in the frame it crosses; columns and rows count Auto (as many fixed-size ones as fit); layout grids don't show or snap on a rotated frame; in outline mode shapes are clicked on their outlines, reaching layers behind. Left: outline mode's Include hidden layers and Include object bounds. Canvas & navigation 97%/72% → 98%/74%, 2–5 h → 1–4 h (this table still showed 95%/72%/3–6 h from before the outlines landed); percentages unchanged at this rounding; full parity ~320–615 h → ~319–614 h |
| 2026-10-11 | minor | Export and inspect behaviour suite written from the public documentation (139 cases in 19 tests; 47 failed before the fixes) and its fixes: an export covers what the layer paints, so a drop shadow adds room only where it falls, strokes on boxes and ellipses reach their real width (they added up to four times as much), hidden layers take no room and a mask group exports the mask's area; a slice exports everything under it; slashes in layer names export into folders; Copy as PNG copies the whole selection. Added: the export settings' contents only and include bounding box, and for SVG outline text (off writes `<text>`), unique id attributes and simplified strokes (stroke attributes where exact); Inspect colour formats (hex, RGB, CSS, HSL, HSB), CSS in rem, a Blend row, positions measured from the frame around a layer (groups aren't frames), CSS for rotation, blend modes, gradient directions and kinds, text alignment, case and decoration; icons and images in the asset list (`file.exportLayers {assets}`). Import/export 80%/50% → 82%/53% (the row still showed the figures from before `.fig` import); Dev Mode 90%/64% → 93%/70%; mainstream weekly areas 68.1% → 68.4%; percentages and full parity hours unchanged at this rounding |
| 2026-10-11 | minor | `.fig` import's tail (gap 1): vectors open as editable vector networks (points, curved segments, filled areas with their winding, per-point corner radius and handle mirroring, end caps from the end points), falling back to outlines when the network is missing or damaged; sticky notes, shapes with text, connectors, tables, code blocks, slides and similar layers import as the closest frames, text layers and vectors with a warning instead of being dropped; set-variable, set-variable-mode and conditional prototype actions come across. New parsers fuzzed on fixtures generated in code. File formats 55% → 58%, 8–15 h → 5–10 h; import/export breadth 80% → 82%, ready 50% → 54%; feature and area totals and ready for real work (~49%) unchanged; beta ~173–310 h → ~170–305 h; full parity ~321–618 h → ~318–613 h |
| 2026-10-11 | minor | Shapes and vector behaviour suite written from the public documentation and its fixes: rounded vector points are drawn and exported, a boolean group looks like its top layer (a subtraction like the bottom one), Alt draws lines from the centre, stars keep to 3–60 points, delete and heal follows the curve, Shift-click on a point at a corner of the points' box deselects it, outline stroke keeps the stroke's colour style, opacity and effects, the miter field shows degrees, individual corners only for rectangles and frames, Option/Alt+Shift+F flattens, MCP `key` reaches the edit tools. Added: corner radius and star ratio handles, Count and Ratio fields, cut drags that divide shapes into layers, paint bucket drags. Shapes & vector 97%/68% → 98%/72%, 1–3 h → 2–4 h. Totals recomputed from the area table, which earlier rows had left stale: feature breadth ~80% → ~82% (core ~91% → ~93%), Features (depth) 57% → 61% (core 68% → 69%), 60–115 h → 56–109 h, ready for real work ~49% → ~51%, mainstream ~46% → ~48%; full parity ~321–618 h → ~319–615 h (the selection suite's −3/−5 h hadn't reached the totals) |
| 2026-10-11 | minor | Paint and effects behaviour suite written from the public documentation and its fixes: Normal groups and frames isolate their children and clipping frames don't (canvas, SVG, PDF); plus darker drawn on the canvas and in PNG exports; drop shadows hidden behind what the layer covers unless shown behind transparent areas, SVG included, with filter regions that reach as far as the effects; Fill images cover the box to its edges; "Normal" names Normal in commands and the blend menu. Paint 85%/66% → 87%/70% (breadth from the GIF/TIFF row), 5–9 h → 3–6 h; effects 72% → 75%, 3–6 h → 2–5 h; percentages unchanged at this rounding; full parity ~321–618 h → ~319–616 h |
| 2026-10-11 | minor | Frames, groups, sections and masks behaviour suite written from the public documentation (15 frame, group and section tests, 6 mask tests) and its fixes: a layer a mask hides can't be clicked there (clicks reach what's behind; Select layer agrees), a mask group's box and exports are the mask's (plus any layers under it), sections stay on the page or in sections (moves, drags, pastes, grouping and new sections in frames refused or kept on the page), drawing a frame or section around layers puts those fully inside it in it, resizing a section leaves its contents where they are (and a frame resized with Cmd/Ctrl held keeps its layers' sizes too), and X/Y of layers in groups are measured in the frame (groups have no coordinate space). Frames, groups, sections, masks 95%/75% → 97%/80%, 2–4 h → 1–3 h; percentages unchanged at this rounding; full parity ~323–621 h → ~322–620 h |
| 2026-10-11 | minor | Styles and variables behaviour suite written from the public documentation (18 command-level tests) and its fixes: editing a styled fill, stroke, effect or layout grid list (directly, a paint row, an added fill or a bound variable) unlinks that style instead of being overwritten by the next style edit; applying a style replaces variables bound to what it sets; text styles go on text layers only; deleting a variable (or its collection) unbinds the layers using it, which keep their last value, and aliases to it take its values; bindings are type-checked (a colour variable can't drive a width) and number variables fill text content; aliases are checked when a variable is created. Added: duplicate, move (reorder, into a folder) and rename folder for styles, with a Local styles list grouped by type and folder (click to edit, right-click, drag to reorder); duplicate mode, set as default and move column for modes; per-platform code syntax (Web, iOS, Android) used by generated CSS and the variables JSON; font weight, paragraph spacing/indent and text content scopes for number variables. Found missing: variables inside styles. Styles & variables 92%/72% → 94%/76%, 1–2 h → 3–5 h; percentages unchanged at this rounding; full parity ~323–621 h → ~325–624 h |
| 2026-10-11 | minor | Prototyping behaviour suite written from the public documentation (7 model tests, 12 player tests with 21 gestures) and its fixes: on drag starts only for a drag its animation's way and the animation follows the pointer, key combinations (`Shift+K`) fire (and are recorded and imported), an overlay's after-delay timer starts when it opens, instances of interactive components play their variant's interactions and "Change to" changes the right instance, spring presets are physical springs that play until they settle, smart animate overshoots with spring and back easings, fixed layers draw above scrolling content; added scroll to offsets. Prototyping 88%/70% → 90%/74%, 5–9 h → 3–6 h; mainstream weekly areas 68.1% → 68.4%, mainstream ~46% → ~47%; other percentages unchanged at this rounding; full parity ~320–615 h → ~318–612 h |
| 2026-10-11 | minor | Components behaviour suite written from the public documentation (10 tests, 45 table cases) and its fixes: instances now keep overrides of auto layout padding, gap and alignment, layout grids, export settings, prototype interactions and every text property (resizing, paragraph spacing, lists…; these were lost when the main changed); moving, rotating, resizing or re-constraining layers inside an instance, and its auto layout direction, are refused (the instance itself resizes); switching variants keeps an override only where both variants had the same value (text layers keep theirs); swapping from the Assets panel keeps only text overrides; the default variant is the top-left one (instances, Assets thumbnails, code); hidden exposed instances hide their properties; pushing changes from a nested layer goes to the outer main. Added: variant property reordering and duplicate combination warnings. Components 95%/68% → 96%/72%, 6–12 h → 4–9 h; with #133's selection and transform change, core editor ready ~68% → ~69% and mainstream ~46% → ~47% (weekly areas 68.1% → 68.8%); full parity ~320–615 h → ~315–607 h (the totals now also carry #133's area 2 hours) |
| 2026-10-11 | minor | Embedded image colour profiles (ICC in PNG/JPG/WebP/TIFF, PNG sRGB/gAMA/cHRM) converted into the file's colour space when decoded. Paint 85%/66% → 90%/67% (the row had lagged the parity doc), 5–9 h → 3–6 h; totals unchanged at this rounding |
| 2026-10-11 | minor | Layers panel keyboard and stroke variables: F2 renames the selected layer (also from a layer row with the keyboard, which gets it back; Escape there cancels), Shift+F10 (`view.contextMenu`) opens the context menu at the focused row or below the canvas selection and the menu takes the keyboard, F8 (`select.area`) draws a selection box for keyboard-only users (arrows move it, Shift+arrows size it, Enter selects like a marquee; agents pass a box), long lists checked at 10k layers (revealing a layer, expanding or collapsing everything and arrow keys take at most 1.3 ms a frame in the release UI bench; the revealed row takes the keyboard); colour styles on strokes were already complete, and each side's stroke weight now binds to a number variable (`strokeTopWeight`…, bindable fields in the Design panel and REST import). Layers panel 95%/78% → 98%/82%, 2–4 h → 1–2 h; Strokes 90%/72% → 95%/75%, 1–3 h → 1–2 h; mainstream depth 67.8% → 68.1%; feature breadth (~80%), ready for real work (~49%), essentials (~57%) and mainstream (~46%) unchanged at this rounding; full parity ~321–618 h → ~320–615 h |
| 2026-10-11 | minor | Text behaviour suite written from the public documentation (13 text cases, 5 command tests) and its fixes: CJK lines break between ideographs, kana and Hangul and before and after Latin words without breaking inside them, with the line-start and line-end rules (closing punctuation, small kana and the prolonged sound mark never start a line, opening brackets never end one); a style's line height, baseline and decoration come from its own font, so lines with CJK or emoji drawn by fallback fonts keep the spacing of the others; paragraph indent applies to start-aligned text only; carets, Delete and Backspace step over whole grapheme clusters (emoji sequences, flags, combining marks), which also shape with one font; double-click and word moves take one script's run in text without spaces. Added: vertical trim (cap height to baseline; `leadingTrim`, Type settings, REST import, inspect, CSS `text-box`). Text 85%/64% → 87%/68%, 7–13 h → 5–10 h; percentages unchanged at this rounding; full parity ~323–621 h → ~321–618 h |
| 2026-10-11 | minor | Selection and transform behaviour suite written from the public documentation (5 selection tests with 38 table rows, 11 transform tests) and its fixes: Alt-resizing shrinks about the centre too (it could only grow); groups, frames with Scale or stretch constraints and several layers with different rotations scale each layer along its own axes, keeping its rotation and flip (children turned 90° grew along the wrong axis, nested turned groups too); several layers dragged past an edge mirror and flip; a flipped layer reads back the rotation typed; resizes land on whole pixels with the pixel grid on; a press inside a layer smaller than its handles moves it and the nearest handle wins (the first in reach did), with its handles hidden under 16 px; locked layers selected in the layers panel get no handles and aren't dragged with the rest; the background of a top-level frame with layers starts a marquee inside it (it moved the frame); Select inverse skips hidden layers like Select all. Added: equal spacing snapping with labelled gaps, Cmd/Ctrl while resizing a frame ignores its constraints. Selection & transform 95%/68% → 97%/72%, 6–10 h → 3–5 h; core editor ready ~67% → ~68%; feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~345–666 h → ~342–661 h |
| 2026-10-11 | minor | Windows QA pass (gap 8): everyday workflows on Windows 11 in the desktop app and through MCP; eight fixes: recovery copies of unsaved documents reopen after a crash or a forced quit (`file.recover`), Ctrl moves and deletes by word in text on Windows and Linux, scrubbed fields keep their value, typing on a field Tab reached edits it, Escape closes dialogs, linear gradients keep square bands, panel hints wrap, the window-close question and "Open in new window" are translated. Percentages and hours unchanged |
| 2026-10-11 | minor | Auto layout behaviour suite written from the public documentation (64 layout cases, 8 command tests) and its fixes: a frame is never smaller than its padding, only the frame's inside stroke counts when strokes are included (and new auto layout includes them), auto gaps stop at 0, grid `fr` tracks behave like the CSS `fr` unit (a track never gets less than its content), canvas stacking is drawn, exported and hit (it was stored only). Added: auto gaps evenly and around, vertical wrap, grid cell alignment, strokes and canvas stacking controls. Auto layout 95%/68% → 98%/74%, 6–12 h → 3–6 h; percentages unchanged at this rounding; full parity ~348–672 h → ~345–666 h |
| 2026-10-11 | minor | Display P3 honest minimum (gap 18): the colour picker of a Display P3 file warns when a colour is outside sRGB (Outside sRGB, with what the canvas and exports do), and View ▸ Soft-proof sRGB (`view.proofSrgb`) stripes the canvas pixels sRGB can't show. Why the canvas still shows sRGB: wgpu 30 can present Display P3 or extended-range sRGB surfaces (Metal, DX12, Vulkan, WebGPU canvases), but eframe/egui-wgpu 0.36 configure an 8-bit sRGB surface with no option for the format or colour space, and egui's renderer writes sRGB-encoded UI colours; showing P3 needs those two upstream (or a patched egui-wgpu). Hardware 13–27 h → 12–26 h; gap 18 6–12 h → 5–11 h; percentages, feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~363–697 h → ~362–696 h |
| 2026-10-11 | minor | Several windows (gap 18): File ▸ New window, a tab's Open in new window or a tab dragged off the tab strip opens files in another window with its own tabs, panels and canvas, sharing the files, preferences, libraries and clipboard; each window draws only when it needs to (idle: ~0% CPU), closing one closes its files after asking about unsaved changes (the main window too, which takes over another window instead of quitting; the last window quits), and the control channel and MCP open, list, focus and capture windows (`window.new`, `window.list`, `window.focus`, `window` on any request); the screen-reader tree covers the main window only (eframe 0.36), so with a screen reader running files stay tabs; one window in the browser. Hardware 55% → 60%, 13–27 h → 10–21 h; gap 18 6–12 h → 4–8 h (P3 on screen left); feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~363–697 h → ~360–691 h |
| 2026-10-11 | minor | Localization layer landed (gap 5): per-language catalogs baked in at build time with English text as keys and fallback, placeholders and plurals, command labels by id, Preferences ▸ Language (System follows the OS or the browser) applied live and `pref.language`; English catalog and a complete Spanish draft awaiting native review. Localization 10% → 20%, 60–100 h → 45–75 h; ready for real work unchanged (~47%); full parity ~363–697 h → ~348–672 h; beta ~200–365 h → ~195–355 h |
| 2026-10-11 | minor | Hosting `figcraft-server`: the desktop app and `figcraft-cli` open `wss://` links (rustls with Mozilla's root certificates, always verified; `FIGCRAFT_CA_FILE` adds a private authority), releases ship the server for Linux x86_64 and aarch64, it stops gracefully on SIGTERM or Ctrl+C, and `docs/server.md` has a deployment guide (systemd unit, Caddy, nginx, Docker); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Plugin panels and settings landed (gap 6, ABI v2; v1 plugins unchanged): a plugin shows a panel of notes, fields (text, number, checkbox, menu, list, colour) and buttons drawn with FigCraft's widgets; a button or a changed value calls it back with every value (same limits per callback, one undo step per callback that edits, rolled back on failure), its module staying loaded while the panel is open; `plugin.ui` / `plugin.event` / `plugin.close` drive panels headless; per-plugin settings (64 KB) kept with the preferences; the Swatch grid example gained a palette panel that remembers the last palette. Plugins & widgets breadth 35% → 45%, ready 20% → 28%, 25–55 h → 15–40 h; ecosystem 20% → 28%; feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~373–712 h → ~363–697 h |
| 2026-10-11 | minor | Canvas fixes (gap 13 closed): boolean groups are clicked where their result is (a click in a subtracted hole reaches what's behind; with the group or an operand selected its operands are hit, so each can still be picked), a layer dragged inside a group stays in the group (frames and auto layout still take other drops), View ▸ Frame outlines and View ▸ Slices draw on the canvas, and the idle tests wait for the render thread instead of counting frames. Shapes & vector 1–3 h → 1–2 h (vector QA with real files left); percentages, feature breadth (~79%), ready for real work (~47%) and full parity hours unchanged at this rounding |
| 2026-10-11 | minor | Code connect and Dev Mode plugins (gap 16): a component or set links to its code component per language (import path, name, a template with `{property}` and `{property ? "a" : "b"}` placeholders); `inspect.code`, `inspect.componentCode` and the Inspect panel show that snippet with the instance's values (also inside generated layer trees), edited in Connect to code with a live preview; links are saved, synced and undoable, and travel as `figcraft.connect.json` (`codeConnect.export` / `codeConnect.import`). Plugins that only read run from the Inspect tab. Dev Mode breadth 80% → 90%, ready 62% → 64%, 4–8 h → 1–3 h; feature breadth (~79%) and ready for real work (~47%) unchanged; full parity ~378–722 h → ~375–717 h |
| 2026-10-11 | minor | Stretch and scatter brushes and pen pressure landed (gap 9): eight brushes drawn in code (four stretched along the path, four scattered with spacing, size, rotation and scatter jitter from a seed) in the stroke settings, drawn everywhere strokes are and flattened by outline stroke; the Pencil records pen pressure as the stroke's width profile (Windows pens, browser styluses; macOS and Linux tablets report none) and can draw with a brush; Figma Draw extras breadth 45% → 65%, ready 30% → 45%, 10–18 h → 5–9 h; Strokes 2–4 h → 1–3 h; Hardware 50% → 55%, 15–30 h → 13–27 h; feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~378–722 h → ~358–687 h |
| 2026-10-11 | minor | Text on a path and pattern fills landed (gap 9): text follows a path or shape (start, side, alignment; edited in place; canvas, PNG, SVG and PDF; SVG `<textPath>` imports as text on a path) and pattern fills tile a layer (grid or half-shifted rows or columns, scale, spacing, alignment; live as the source changes; SVG `<pattern>`, PDF tiling patterns); Figma Draw extras breadth 45% → 65%, ready 30% → 45%, 10–18 h → 5–10 h; Text 8–14 h → 7–13 h; feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~380–725 h → ~375–715 h |
| 2026-10-11 | minor | Team libraries on `figcraft-server` landed (gap 10): a file's library is published to the server under the file and enabled in other files by a share link (per-asset versions; updates noticed in the background and accepted with overrides kept), the server counts which files use which components, and a whole library can be swapped for another by names; Team libraries breadth 40% → 70%, ready 25% → 40%, 10–20 h → 4–8 h; collaboration 12% → 13%, 55–110 h → 49–98 h; feature breadth (~79%) and ready for real work (~47%) unchanged; full parity ~380–725 h → ~375–715 h |
| 2026-10-11 | minor | Branches landed (gap 17): File ▸ Create branch… copies the file into a branch tab that remembers its base; Review and merge shows both sides' changes (layers with Inspect-style property changes, styles, variables, comments) and the conflicts (the same property changed on both sides, or a layer deleted on one side and edited on the other), each resolved for the branch or main; merging is one undo step and archives the branch; Update from main; branches are saved in the `.figcraft` file. Comments, versions, find, misc breadth 80% → 85%, ready 58% → 60%, 4–8 h → 2–5 h; collaboration 12% → 13%; menu breadth 204/204 (the catalog gained File ▸ Create branch…); feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~380–725 h → ~378–722 h |
| 2026-10-11 | minor | Animated GIF fills and TIFF images (gap 12, without video and HEIC): GIF fills keep the original file, show the first frame on the canvas and in exports and play in the presenter and the preview (frames decoded on first play, at most 500 frames and 128 MB; always looping; the window wakes only for a GIF's next frame); TIFF images open, import, place and paste. Paint breadth 85% → 87%, 5–9 h → 4–7 h; gap 12 6–12 h → 5–10 h (video playback and HEIC left, both needing non-Rust decoders); feature breadth (~79%), ready for real work (~47%) and full parity hours unchanged at this rounding |
| 2026-10-11 | minor | The last three catalogued menu items got commands (gap 21): File ▸ Open recent (recent files kept in the preferences, at most 20, a file that's gone greyed in the menu and dropped when picked, Clear recent), View ▸ Outlines ▸ Show outlines for selection and View ▸ Mask outlines: menu breadth 98.5% → 100% (203/203); Canvas & navigation breadth 95% → 97%, 3–6 h → 2–5 h; feature breadth (~79%), ready for real work (~47%) and full parity hours unchanged at this rounding |
| 2026-10-11 | minor | Extended collections, variables import/export and gradient stop and variant property bindings landed (gap 14): Styles & variables breadth 85% → 92%, ready 70% → 72%, 4–8 h → 1–2 h; feature breadth unchanged (~79%); ready for real work unchanged (~47%); full parity ~385–730 h → ~380–725 h |
| 2026-10-11 | minor | Collaboration wording after the standalone server: `figcraft-server` keeps many files, persisted as a snapshot plus a log of what changed (compacted, checksummed, damaged files skipped and reported), restarts lose nothing confirmed and apps resume where they were; the app publishes its file to it; share links per file let people edit or only view (the server refuses a viewer's edits), are revocable and stored as hashes; TLS comes from a reverse proxy (the desktop app has none of its own yet); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Live boolean editing, Design panel arc fields, the shape builder on strokes and open paths and one-sided variable widths landed (gap 13): Shapes & vector breadth 92% → 97%, ready 66% → 68%, 8–14 h → 1–3 h; feature breadth ~78% → ~79%; ready for real work unchanged (~47%); full parity ~390–745 h → ~385–730 h |
| 2026-10-11 | minor | Collaboration wording after the collaboration polish: styles, variables, collections, modes and comments created in a live session take ids from their creator's id space (concurrent creations no longer overwrite each other; the server checks it), others' text carets and selections show on the canvas, and typing in a layer someone else is typing in says so; percentages unchanged |
| 2026-10-11 | minor | Wording only: live sessions (#104), then the browser joining them and `figcraft://` invite links opening the desktop app; percentages and hours unchanged until the next re-measure |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | minor | Readiness by audience with hours to ~95% (full ~400–750, mainstream ~180–330, essentials ~85–160) |
| 2026-10-10 | minor | Added essentials user (~53%) and mainstream practitioner (~43%); ready for real work unchanged at ~46% |
| 2026-10-10 | minor | Alpha gate (core workflows) checked under the revised stage rules: passes, stays alpha |
| 2026-10-10 | major | Rewritten to the craftrules progress-docs shape after a full re-measure; rubric, area narrative and inventory moved to docs/target-app-parity.md, gaps to docs/gaps.md, milestones to docs/roadmap.md. Depth ~79% (self-graded, 16 areas) → ready for real work ~46% (all dimensions); full parity 115–195 h → 420–790 h |
| 2026-10-10 | minor | Day-2 refreshes after each batch of PRs (depth 58% → 79%) |
| 2026-10-09 | major | First roadmap |
