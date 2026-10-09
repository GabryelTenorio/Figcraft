# FigCraft Roadmap

FigCraft is a clean-room, open-source, pure-Rust reimplementation of the Figma design workflow. It
runs on macOS, Windows, Linux, FreeBSD and the web (WASM), and agents can drive it completely through
the CLI, a JSON control channel and MCP.

This file tracks **how far we are and what's left**. Time estimates are wall-clock hours of
continuous Claude Opus 5.5 agent work (including builds and the CI gate), for **one agent** and for
**4–6 parallel agents** on disjoint crates. They are counted from the remaining work in the parity
table below and updated as work lands.

_Last updated: 2026-10-10 (vector edit mode tools, auto layout depth, layers panel depth)._

## Where we are

| Dimension | Status |
|---|---|
| Infrastructure (crates, command registry, undo, renderer, formats, control channel, tests) | **~70%** |
| Look & feel vs the incumbent's 2026 UI (measured side by side) | **~65%** |
| Menu/command breadth (`docs/parity.md`) | **96%** of the catalogued menu tree has a command |
| Feature depth (weighted rubric below) | **~40%** |
| Parity including interaction fidelity and hardening | **~25–30%** |
| Time to **feature parity** | **~260–380 h** one agent · **~70–110 h** with 4–6 agents |
| Time to **full parity** (feature parity + fidelity pass + hardening) | **~420–600 h** one agent · **~110–170 h** with 4–6 agents |

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
   merge, delete vs delete-and-heal, nudging, one undo step per gesture. *Left:* eraser, variable
   width strokes, shape builder, transform box for several points, point snapping. 12–18 h.
2. **Text editing depth**: *landed:* OpenType features panel, small caps, lists, title case,
   truncation/max lines, font picker with previews and recents, word/line selection, IME commit.
   *Left:* inline IME preedit, variable font axes, visual QA. 8–12 h.
3. **Auto layout depth**: *landed:* on-canvas padding/gap handles (also wrapped-row and grid
   column/row gaps, grid tracks outlined), min/max width/height (UI, Hug/Fill/Fixed, hugging text
   wraps at its max), ignore auto layout (absolute position) with constraints, text baseline
   alignment, grid tracks (fixed/hug/`fr`), grid cells and spans, wrap row gap UI. *Left:*
   grid/wrap-aware drag-reorder and drag-to-cell, nested hug/fill edge cases. 6–10 h.
4. **Components depth**: *landed:* component properties (edit/delete/bind) with the properties
   panel, add variant, instance swap UI, per-layer overrides with reset, restore main, instance
   structure protection; Assets panel with live thumbnails, search and drag-to-canvas (drops into
   the frame or auto layout slot under the pointer). *Left:* nested instance overrides depth, slots,
   local libraries (publish/use across files), component descriptions UI. 10–14 h.
5. **Prototyping depth**: *landed:* on-canvas connections with drag-to-connect, interaction editor,
   flows, smart animate, overlays, scroll/fixed/sticky, change-to and set-variable in the presenter.
   *Left:* overlay close animation, springs, device frames, multi-selection editing. 6–10 h.
6. **Interop**: .fig import (best effort), PDF export (in progress), image crop handles (landed),
   paste from other apps, Figma REST JSON import. 25–40 h.
7. **Look & feel and interaction-fidelity pass**: tool by tool, panel by panel against the incumbent
   (cursors, modifier keys, hover states, menus, popovers). 40–60 h.
8. **Collaboration (local):** comments landed (pins, threads, resolve, panel). Version history UI,
   multiplayer and plugins are not started. 30–60 h.
9. **Hardening and performance**: real-world file corpus, perf budgets at 50k layers (tile cache,
   incremental re-render), Windows/Linux/browser QA, accessibility. 40–60 h.

### Where we're going

- **Next:** text depth (2) and auto layout depth (3) in parallel with the fidelity pass (7); the
  rest of vector editing (1).
- **Then:** auto layout/components/prototyping depth (3–5), interop (6).
- **Alpha:** release builds on every push to `release` (packaging in progress), demo files, docs.

## Shipped so far

- **Architecture:** 13 crates with enforced layering (`cargo xtask layers`): geom, color, doc,
  layout, text, pathops, render, format, svg, engine, ui-egui (+ mcp, pdf in progress). Every action
  is a command (~250); undo is unlimited via structural sharing (`Arc` node snapshots); gestures are
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
- **Tests:** ~120 automated tests including a junk-parameter sweep over every command.

## Parity estimate (weighted rubric)

Scores by behaviour, 0–100%. Hours are what remains (one agent).

| Area | Weight | Score | Remaining work | Hours |
|---|---|---|---|---|
| Canvas & navigation | 6 | 60% | rulers/guides dragging, pixel preview, minimap-free perf at scale | 8–12 |
| Selection & transform | 8 | 60% | rotated multi-select handles, scale tool polish, measure hover polish | 10–15 |
| Layers panel | 5 | 70% | drag polish (auto-scroll while dragging, drop into collapsed groups), keyboard row navigation | 4–6 |
| Shapes & vector | 8 | 55% | eraser, variable width, shape builder, multi-point transform box, point snapping, live boolean editing, arc handles on canvas | 15–22 |
| Paint | 7 | 65% | colour profiles (Display P3), paint style picker polish, image adjustment presets | 6–10 |
| Strokes | 3 | 55% | arrowhead set, dash/gap UI, per-side UI, stroke styles | 5–8 |
| Effects | 4 | 60% | effect styles UI, glass/texture fidelity, shadow spread on non-rects exactness | 6–10 |
| Frames, groups, sections, masks | 5 | 55% | section tooling, mask types (luminance/outline), frame presets UI | 6–10 |
| Auto layout & constraints | 9 | 60% | grid/wrap-aware drag-reorder, drag to grid cell, nested hug/fill edge cases | 6–10 |
| Text | 8 | 35% | in-text mouse selection, styled ranges UI, lists, OpenType, variable fonts | 20–30 |
| Components & variants | 9 | 55% | nested overrides depth, libraries, slots, descriptions | 12–18 |
| Styles & variables | 5 | 45% | style picker everywhere, variable binding UI, aliasing UI, scoping | 10–16 |
| Prototyping | 6 | 30% | noodle editing, overlays, scrolling, smart animate | 20–30 |
| Import/export | 6 | 45% | PDF (in progress), .fig import, export panel polish | 20–35 |
| Dev mode / inspect | 2 | 35% | measurements on canvas, code variants (Swift/Compose), assets | 6–10 |
| Comments, versions, misc | 3 | 25% | comments, version history UI, plugins (WASM) | 15–25 |

Weighted feature depth ≈ **50%**.

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
