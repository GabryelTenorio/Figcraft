# FigCraft Roadmap

FigCraft is a clean-room, open-source, pure-Rust reimplementation of the Figma design workflow. It
runs on macOS, Windows, Linux, FreeBSD and the web (WASM), and agents can drive it completely through
the CLI, a JSON control channel and MCP.

This file tracks **how far we are and what's left**. Time estimates are wall-clock hours of
continuous Claude Opus 5.5 agent work (including builds and the CI gate), for **one agent** and for
**4–6 parallel agents** on disjoint crates. They are counted from the remaining work in the parity
table below and updated as work lands.

_Last updated: 2026-10-10 (day 2: vector editing, auto layout, text, components and assets, prototyping, interop, styles and variables, inspect code, panels polish)._

## Where we are

| Dimension | Status |
|---|---|
| Infrastructure (crates, command registry, undo, renderer, formats, control channel, tests) | **~70%** |
| Look & feel vs the incumbent's 2026 UI (measured side by side) | **~65%** |
| Menu/command breadth (`docs/parity.md`) | **97%** of the catalogued menu tree has a command |
| Feature depth (weighted rubric below) | **~58%** |
| Parity including interaction fidelity and hardening | **~35–40%** |
| Time to **feature parity** | **~130–210 h** one agent · **~35–60 h** with 4–6 agents |
| Time to **full parity** (feature parity + fidelity pass + hardening) | **~210–330 h** one agent · **~60–95 h** with 4–6 agents |

## Honest assessment (2026-10-09)

**In one line:** the architecture is complete end to end (document model → commands → renderer →
UI, CLI/MCP and control channel), and the everyday core (frames, shapes, text, auto layout,
components with overrides, variants, styles, variables, effects, booleans, prototyping basics,
SVG/PNG/JPG export) works. What's missing is depth: vector editing, rich text editing, the long tail
of auto-layout and component behaviours, real-file interop and a fidelity pass against the
incumbent.

**How much to trust the numbers:** scores are self-graded by the agents that built the features from
public documentation and black-box observation. Treat them as upper bounds. Look & feel is the only
dimension measured side by side.

### Where we're lacking (priority order)

1. **Vector edit mode**: *landed:* edit-mode toolbar (move, pen, bend, paint bucket, lasso, cut),
   pen that extends, branches and closes paths (vector networks), paint bucket fill/unfill of any
   closed area (faces with holes), point/segment selection, handle mirroring (none/angle/angle and
   length) with a Point panel (position, corner radius, mirroring), smooth/sharp points, join and
   merge, delete vs delete-and-heal, nudging, one undo step per gesture, eraser (⇧E). *Left:* variable
   width strokes, shape builder, transform box for several points, point snapping. 12–18 h.
2. **Text editing depth**: *landed:* OpenType features panel, small caps, lists, title case,
   truncation/max lines, font picker with previews and recents, word/line selection, IME commit,
   inline IME composition (underlined at the caret, reflowing, candidate window at the caret; the
   document only gets the committed text), variable fonts (axes and named instances read from the
   font, named instances as styles, Type settings ▸ Variable sliders, per-range axis values applied
   to shaping, outlines and PDF), range-aware Typography panel with "Mixed". Range fills in the Fill
   section. *Left:* axis hover preview, right-to-left/bidi text, visual QA. 6–10 h.
3. **Auto layout depth**: *landed:* on-canvas padding/gap handles (also wrapped-row and grid
   column/row gaps, grid tracks outlined), min/max width/height (UI, Hug/Fill/Fixed, hugging text
   wraps at its max), ignore auto layout (absolute position) with constraints, text baseline
   alignment, grid tracks (fixed/hug/`fr`), grid cells and spans, wrap row gap UI, wrap-aware
   drag-reorder (by row) and dragging into a grid cell. *Left:* nested hug/fill edge cases. 4–7 h.
4. **Components depth**: *landed:* component properties (edit/delete/bind) with the properties
   panel, add variant, instance swap UI, per-layer overrides with reset, restore main, instance
   structure protection; Assets panel with live thumbnails, search and drag-to-canvas (drops into
   the frame or auto layout slot under the pointer); exposed nested instances (their properties
   show on the main's instances); component descriptions (edited on the main, shown on
   instances); local libraries (publish a file's components, sets and styles with per-asset
   versions, enable library files, place library components and apply library styles as
   read-only linked copies, "Library updates available" with overrides kept on update).
   *Left:* nested override edge cases, slots, library variables. 7–11 h.
5. **Prototyping depth**: *landed:* on-canvas connections with drag-to-connect, interaction editor,
   flows, smart animate, overlays, scroll/fixed/sticky, change-to and set-variable in the presenter.
   Custom bezier and custom spring easings with editor fields; device frames (phone, tablet,
   laptop, desktop bodies drawn in code) around the presented frame; overlays animate out with
   their opening transition reversed; several selected layers edit their interactions together
   (or replace mixed ones). *Left:* scrolling inside device frames. 3–5 h.
6. **Interop**: *landed:* image crop handles; REST API file JSON import (`file.open` / `file.import`
   / CLI: pages, layer tree, geometry, paints, strokes, effects, text runs, vectors from geometry
   paths, auto layout, constraints, grids, components, sets, instances with overrides, component
   properties, styles; image fills from an `images` folder or placeholders; warnings for the rest).
   Paste from other apps: images, SVG and text (the newest clipboard content wins over our own
   copy). *Left:* .fig import (best effort), PDF export (in progress), REST import
   of prototype interactions and variables. 20–32 h.
7. **Look & feel and interaction-fidelity pass**: tool by tool, panel by panel against the incumbent
   (cursors, modifier keys, hover states, menus, popovers). 40–60 h.
8. **Collaboration (local):** comments landed (pins, threads, resolve, panel). Version history
   landed (named versions saved in the `.figcraft` file; restore, rename, duplicate, delete; panel);
   previews on the canvas and autosave checkpoints are missing. Multiplayer and plugins are not
   started. 25–55 h.
9. **Hardening and performance**: *landed:* `cargo xtask bench` (synthetic 1k/10k/50k-layer
   pages: engine, render and headless UI-frame timings). The canvas keeps its last image and
   re-renders only what a pan uncovers or an edit touches: moving one layer at 10k layers re-renders
   in ~0.2 ms instead of ~8 ms (0.8 vs 24 ms at 50k zoomed out), a 40 px pan in ~0.7 ms instead of
   ~7.5 ms. The layers panel lays out only the rows in view (every screen expanded at 50k: 39 →
   0.6 ms a frame); text layouts are shared instead of re-formatted per lookup; overflowing text is
   culled by its glyphs. At 10k: full 1440×900 renders 7–15 ms, single-layer edits ~10 µs, hit
   tests ~26 µs. *Left:* zoomed-out renders at 50k (~40 ms: tiles or levels of detail), save/load
   at 50k (~0.7 s each), a real-world file corpus, Windows/Linux/browser QA, accessibility. 40–60 h.

### Where we're going

- **Next:** hardening and performance (9) with the fidelity pass (7); the rest of vector editing
  (eraser, shape builder, variable width) and components (nested overrides, libraries).
- **Then:** interop depth (6: .fig best effort, paste from other apps), version history and
  collaboration (8).
- **Alpha:** release builds on every push to `release` (packaging in progress), demo files, docs.

## Shipped so far

- **Architecture:** 13 crates with enforced layering (`cargo xtask layers`): geom, color, doc,
  layout, text, pathops, render, format, svg, engine, ui-egui (+ mcp, pdf in progress). Every action
  is a command (~330); undo is unlimited via structural sharing (`Arc` node snapshots); gestures are
  one undo step; `command.batch` runs several commands as one step.
- **Document model:** pages, frames, groups, sections, rectangles, ellipses (arcs/rings), polygons,
  stars, lines/arrows, vector networks, text with styled runs, boolean groups, components, component
  sets (variants), instances with per-layer overrides, slices; fills (solid, linear/radial/angular/
  diamond gradients, images with fill/fit/crop/tile + adjustments), multiple strokes with
  inside/center/outside alignment, per-side weights, dashes, caps/arrowheads, joins; effects (drop
  and inner shadow with spread, layer blur, background blur, noise, texture); corner radii with
  smoothing; constraints; auto layout (horizontal, vertical, wrap, grid; hug/fill/fixed; min/max;
  absolute children); layout grids; export settings; prototype interactions, flows and transitions;
  paint/text/effect/grid styles; variables with collections, modes and aliases.
- **Renderer:** CPU (tiny-skia) scene renderer used for canvas, export and the web: isolated layers
  for opacity/blend/effects, clip content, masks, booleans (linesweeper), text (harfrust shaping,
  skrifa outlines, font fallback), aligned strokes with caching.
- **Engine:** pointer tools (move/marquee/deep select/resize/rotate/Alt-duplicate with smart-guide
  snapping, frame/section/slice/shapes/line/arrow with Shift/Alt, text, pen, pencil, hand,
  eyedropper), text editing (caret, selection, words, lines), auto layout reorder by dragging,
  drop into frames, align/distribute/tidy, z-order, flip/rotate, scale tool, booleans, flatten,
  outline stroke, components (create, instances, sync with overrides, reset, detach, push changes,
  variants, properties), styles and variables, prototype commands, find/replace, batch rename,
  copy/paste (internal + SVG/text), file new/open/save/import/export (native `.figcraft`, SVG
  in/out, PNG/JPG/WebP), version snapshots.
- **UI (egui, swappable):** the incumbent's 2026 layout: tab strip, icon rail, file header, pages and
  layers tree (rename, visibility/lock, drag to reorder/reparent), Assets panel, floating bottom
  toolbar with flyouts and mode switcher, Design / Prototype / Inspect (CSS) panels, colour picker
  (HSB square, hue/alpha, hex, gradient stops, image fills), main menu generated from the catalog,
  context menu, Actions palette (⌘K), keyboard shortcuts from the catalog, presentation player with
  transitions, light and dark themes, home screen, About/Preferences/shortcuts/variables dialogs.
- **Automation:** desktop control channel (`--control <port>`: execute commands, inspect document/UI,
  synthetic pointer/keys/typing, menu invocation, screenshots, renders).
- **Tests:** ~260 automated tests including junk-parameter sweeps over every command (and every vector-edit command while editing), importer fuzzing and render regressions.

## Parity estimate (weighted rubric)

Scores by behaviour, 0–100%. Hours are what remains (one agent).

| Area | Weight | Score | Remaining work | Hours |
|---|---|---|---|---|
| Canvas & navigation | 6 | 65% | pixel preview, frame guides, perf at scale | 6–10 |
| Selection & transform | 8 | 64% | scale tool polish, measure hover polish | 8–12 |
| Layers panel | 5 | 70% | drag polish (auto-scroll while dragging, drop into collapsed groups), keyboard row navigation | 4–6 |
| Shapes & vector | 8 | 60% | variable width, shape builder, multi-point transform box, point snapping, live boolean editing, arc handles on canvas | 12–18 |
| Paint | 7 | 65% | colour profiles (Display P3), paint style picker polish, image adjustment presets | 6–10 |
| Strokes | 3 | 70% | stroke styles, variable width, arrowhead previews in the menus | 3–5 |
| Effects | 4 | 70% | glass/texture fidelity, shadow spread on non-rects exactness | 4–7 |
| Frames, groups, sections, masks | 5 | 75% | hiding section contents UI, section-level prototype/dev affordances | 2–4 |
| Auto layout & constraints | 9 | 65% | nested hug/fill edge cases, drag-reorder across nested auto layout frames | 4–7 |
| Text | 8 | 65% | visual caret movement in right-to-left runs, right-to-left layer names in the panels, text style picker depth, axis hover preview | 6–9 |
| Components & variants | 9 | 65% | slots, nested override edge cases, library variables | 7–11 |
| Styles & variables | 5 | 72% | binding effects, line height and letter spacing, variable groups in the editor sidebar | 3–5 |
| Prototyping | 6 | 70% | scrolling inside device frames, prototype preview pane in the editor | 3–5 |
| Import/export | 6 | 50% | PDF (in progress), .fig import, export panel polish, REST import of prototypes/variables | 18–32 |
| Dev mode / inspect | 2 | 55% | asset export list in Inspect, redlines in the Inspect tab, code for whole frames as components | 4–6 |
| Comments, versions, misc | 3 | 40% | autosave checkpoints, comment polish, plugins (WASM) | 10–17 |

Weighted feature depth ≈ **58%** (self-graded; the table's arithmetic gives ~59%).

## Milestones

| Milestone | Status |
|---|---|
| M0 Foundation: workspace, model, renderer, engine, UI shell, app, control channel | done (2026-10-09) |
| M0.9 CLI + MCP, web build, release packaging, PDF export | in progress |
| M1 Core editing completeness (tools, panels, menus, clipboard, files) | ~60% |
| M2 Layout depth | ~60% |
| M3 Components & styles depth | ~40% |
| M4 Vector & text depth | ~45% |
| M5 Prototype & present | ~30% |
| M6 Interop & dev mode | ~35% |
| M7 Release (signed builds for all platforms, README screenshots) | started |
