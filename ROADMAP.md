# FigCraft Roadmap

FigCraft is a clean-room, open-source, pure-Rust reimplementation of the Figma design workflow. It
runs on macOS, Windows, Linux, FreeBSD and the web (WASM), and agents can drive it completely through
the CLI, a JSON control channel and MCP.

This file tracks **how far we are and what's left**. Time estimates are wall-clock hours of
continuous Claude Opus 5.5 agent work (including builds and the CI gate), for **one agent** and for
**4–6 parallel agents** on disjoint crates. They are counted from the remaining work in the parity
table below and updated as work lands.

_Last updated: 2026-10-10 (day 2: vector editing and the shape builder, auto layout nesting, bidi text, components, local libraries, variables editor and bindings, version previews and autosaves, prototype preview pane and device scrolling, REST prototypes and variables, export panel, Inspect properties/assets/HTML, canvas performance, panels polish)._

## Where we are

| Dimension | Status |
|---|---|
| Infrastructure (crates, command registry, undo, renderer, formats, control channel, tests) | **~70%** |
| Look & feel vs the incumbent's 2026 UI (measured side by side) | **~65%** |
| Menu/command breadth (`docs/parity.md`) | **97.5%** (196/201) of the catalogued menu tree has a command |
| Feature depth (weighted rubric below) | **~68%** |
| Parity including interaction fidelity and hardening | **~40–45%** |
| Time to **feature parity** | **~80–130 h** one agent · **~25–40 h** with 4–6 agents |
| Time to **full parity** (feature parity + fidelity pass + hardening) | **~160–250 h** one agent · **~45–75 h** with 4–6 agents |

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
   merge, delete vs delete-and-heal, nudging, one undo step per gesture, eraser (⇧E), shape builder
   (⇧M: the regions of overlapping shapes, click or drag to merge, Alt to remove, curves kept),
   variable width strokes (width profile presets with previews in the stroke settings, the Variable
   width tool ⇧W: drag the path to add a width point, slide points, drag their handles, Delete;
   smooth interpolation per subpath, dashes, caps and joins, canvas/PNG/SVG/PDF and outline stroke).
   *Left:* transform box for several points, point snapping, one-sided widths. 7–11 h.
2. **Text editing depth**: *landed:* OpenType features panel, small caps, lists, title case,
   truncation/max lines, font picker with previews and recents, word/line selection, IME commit,
   inline IME composition (underlined at the caret, reflowing, candidate window at the caret; the
   document only gets the committed text), variable fonts (axes and named instances read from the
   font, named instances as styles, Type settings ▸ Variable sliders, per-range axis values applied
   to shaping, outlines and PDF), range-aware Typography panel with "Mixed". Range fills in the Fill
   section. Right-to-left and mixed-direction text (Unicode bidi: Arabic shaping, mirrored brackets,
   caret, clicks and selection follow the runs; arrow keys move visually). *Left:* axis hover
   preview, right-to-left layer names in the panels, visual QA. 5–8 h.
3. **Auto layout depth**: *landed:* on-canvas padding/gap handles (also wrapped-row and grid
   column/row gaps, grid tracks outlined), min/max width/height (UI, Hug/Fill/Fixed, hugging text
   wraps at its max), ignore auto layout (absolute position) with constraints, text baseline
   alignment, grid tracks (fixed/hug/`fr`), grid cells and spans, wrap row gap UI, wrap-aware
   drag-reorder (by row) and dragging into a grid cell, nested hug/fill settling in one pass
   (wrapping fill text, three levels deep, grid rows), min/max redistribution, coherent
   Fill/Hug rules, drag across nested auto layout frames (deepest one wins, insertion line, out to
   the canvas or a plain frame). *Left:* visual QA of nested edge cases, arrow-key moves across
   nested frames. 2–4 h.
4. **Components depth**: *landed:* component properties (edit/delete/bind) with the properties
   panel, add variant, instance swap UI, per-layer overrides with reset, restore main, instance
   structure protection; Assets panel with live thumbnails, search and drag-to-canvas (drops into
   the frame or auto layout slot under the pointer); exposed nested instances (their properties
   show on the main's instances); component descriptions (edited on the main, shown on
   instances); local libraries (publish a file's components, sets and styles with per-asset
   versions, enable library files, place library components and apply library styles as
   read-only linked copies, "Library updates available" with overrides kept on update); slots
   (a frame of a main made a slot property with preferred components; instances fill it with any
   content of their own, kept across main updates, variant switches, copy/paste and detach, with
   the main's default content until edited; reset slot; slot badges in the layers panel and on the
   canvas; fill menu in the properties panel; drops from the Assets panel and the canvas land in
   slots); nested overrides kept when the main moves or swaps a nested instance, and on nested
   instances copied out of an instance (re-linked to their own main); canvas drags can no longer
   pull layers out of an instance; library variables (publishing includes the file's variable
   collections with their modes, aliases and scopes, versioned per variable and collection, without
   variables hidden from publishing; files using the library see its collections read-only under
   the library's name in the Variables editor and the variable pickers; binding, aliasing or setting
   a library collection's mode on a frame copies the variables in as read-only linked copies, so the
   file renders without the library; library components keep their variable bindings and modes;
   "Library updates available" covers changed variables and bound layers follow). Explicit variable
   modes in the Design panel: a frame's mode per collection, Auto (inherited) or its own.
   *Left:* library collections in the Design panel's mode picker. 1 h.
5. **Prototyping depth**: *landed:* on-canvas connections with drag-to-connect, interaction editor,
   flows, smart animate, overlays, scroll/fixed/sticky, change-to and set-variable in the presenter.
   Custom bezier and custom spring easings with editor fields; device frames (phone, tablet,
   laptop, desktop bodies drawn in code) around the presented frame; overlays animate out with
   their opening transition reversed; several selected layers edit their interactions together
   (or replace mixed ones); scrolling inside device frames (the frame sized to the device's screen,
   clipped or scrolling by wheel and drag, fixed layers pinned to the screen by their constraints,
   nested "Scroll to", preserve scroll position); a resizable preview pane over the canvas that
   plays the selected frame and follows edits live; device rotation (phones and tablets turn to
   landscape: saved in the file, imported from REST files, drawn turned in the presenter and the
   pane); presentation options (fit, fit width, fill, 100%, with Z to cycle them and R to
   restart, the device frame shown or hidden, kept between presentations; what overflows the
   window pans); scroll momentum (drags fling and slow down the same at any frame rate, the wheel
   coasts, a drag past the ends stretches and springs back). *Left:* responsive presentation
   (frames resized to the window), device colours and custom device sizes. 1–2 h.
6. **Interop**: *landed:* image crop handles; REST API file JSON import (`file.open` / `file.import`
   / CLI: pages, layer tree, geometry, paints, strokes, effects, text runs, vectors from geometry
   paths, auto layout, constraints, grids, components, sets, instances with overrides, component
   properties, styles; image fills from an `images` folder or placeholders; warnings for the rest),
   now with prototyping (interactions with their triggers, actions, transitions and easing, flows,
   scrolling, the prototype device) and variables (the local variables JSON passed along, next to
   the file or embedded: collections, modes, aliases, scopes, layer bindings and explicit modes).
   Export panel: a row per export setting (typed or preset sizes like `2x` / `512w` / `512h`, PNG,
   JPG, WebP, SVG and PDF, suffix), a preview, and "Export N layers" (`file.exportLayers`) in the
   panel and the Export dialog. Paste from other apps: images, SVG and text (the newest clipboard
   content wins over our own copy). *Left:* .fig import (best effort), PDF export (in progress),
   REST import of conditional and variable-mode prototype actions. 14–26 h.
7. **Look & feel and interaction-fidelity pass**: tool by tool, panel by panel against the incumbent
   (cursors, modifier keys, hover states, menus, popovers). 40–60 h.
8. **Collaboration (local):** comments landed (pins, threads, resolve, panel). Version history
   landed (named versions saved in the `.figcraft` file; restore, rename, duplicate, delete; panel;
   view a version on the canvas read-only before restoring it; autosave checkpoints every 30
   minutes, folded in the panel). Multiplayer and plugins are not started. 20–50 h.
9. **Hardening and performance**: *landed:* `cargo xtask bench` (synthetic 1k/10k/50k-layer
   pages: engine, render and headless UI-frame timings). The canvas keeps its last image and
   re-renders only what a pan uncovers or an edit touches: moving one layer at 10k layers re-renders
   in ~0.2 ms instead of ~8 ms (0.8 vs 24 ms at 50k zoomed out), a 40 px pan in ~0.7 ms instead of
   ~7.5 ms. The layers panel lays out only the rows in view (every screen expanded at 50k: 39 →
   0.6 ms a frame); text layouts are shared instead of re-formatted per lookup; overflowing text is
   culled by its glyphs. At 10k: full 1440×900 renders 7–15 ms, single-layer edits ~10 µs, hit
   tests ~26 µs. Zoomed out (50% and below) the canvas draws levels of detail: layers under 2 px
   land as their averaged colour, text under 4 px is greeked into bars, small rectangles and clip
   layers skip the path pipeline (exports and closer zooms stay exact): a whole 50k-layer page
   renders in ~8 ms instead of ~50 ms. `.figcraft` at 50k layers saves in ~0.09 s instead of ~1 s
   and loads in ~0.16 s instead of ~0.86 s (JSON streamed to and from the document without a
   value tree, layer fields at their defaults left out: 63 → 22 MB of JSON, old files still
   open). *Left:* a real-world file corpus, Windows/Linux/browser QA, accessibility. 30–50 h.

### Where we're going

- **Next:** the fidelity pass (7) and hardening (9: a real-file corpus, platform QA); responsive
  presentation.
- **Then:** interop depth (6: PDF export, .fig best effort), plugins and collaboration (8).
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
  variants, properties, slots), styles and variables, prototype commands, find/replace, batch rename,
  copy/paste (internal + SVG/text), file new/open/save/import/export (native `.figcraft`, SVG
  in/out, PNG/JPG/WebP), version snapshots.
- **UI (egui, swappable):** the incumbent's 2026 layout: tab strip that is also the window's title
  bar (frameless on Windows/Linux with drawn window buttons, drag, double-click maximise and edge
  resizing; macOS window buttons inset into it), icon rail, file header, pages and
  layers tree (rename, visibility/lock, drag to reorder/reparent), Assets panel, floating bottom
  toolbar with flyouts and mode switcher, Design / Prototype / Inspect (CSS) panels, colour picker
  (HSB square, hue/alpha, hex, gradient stops, image fills), main menu generated from the catalog,
  context menu, Actions palette (⌘K), keyboard shortcuts from the catalog, presentation player with
  transitions, light and dark themes, home screen, About/Preferences/shortcuts/variables dialogs.
- **Automation:** desktop control channel (`--control <port>`: execute commands, inspect document/UI,
  synthetic pointer/keys/typing, menu invocation, screenshots, renders).
- **Tests:** ~390 automated tests including junk-parameter sweeps over every command (and every vector-edit command while editing), importer fuzzing and render regressions.

## Parity estimate (weighted rubric)

Scores by behaviour, 0–100%. Hours are what remains (one agent).

| Area | Weight | Score | Remaining work | Hours |
|---|---|---|---|---|
| Canvas & navigation | 6 | 72% | frame guides, 2x pixel preview | 3–5 |
| Selection & transform | 8 | 68% | measure hover polish, scale tool on-canvas handles | 5–8 |
| Layers panel | 5 | 79% | keyboard row navigation | 1–2 |
| Shapes & vector | 8 | 66% | multi-point transform box, point snapping, live boolean editing, arc handles on canvas, shape builder on strokes and open paths, one-sided widths | 7–11 |
| Paint | 7 | 78% | a wide-gamut canvas (egui draws sRGB, so Display P3 colours outside sRGB clip on screen; exports keep them), images' own embedded colour profiles (images are taken as sRGB) | 1–3 |
| Strokes | 3 | 76% | stroke styles, arrowhead previews in the menus | 2–3 |
| Effects | 4 | 70% | glass/texture fidelity, shadow spread on non-rects exactness | 4–7 |
| Frames, groups, sections, masks | 5 | 75% | hiding section contents UI, section-level prototype/dev affordances | 2–4 |
| Auto layout & constraints | 9 | 72% | visual QA of nested edge cases, arrow-key moves across nested auto layout frames | 2–4 |
| Text | 8 | 67% | right-to-left layer names in the panels, text style picker depth, axis hover preview | 5–8 |
| Components & variants | 9 | 71% | library variables | 2–4 |
| Styles & variables | 5 | 82% | bindings that follow effects when they are reordered, grid and component property bindings | 1–2 |
| Prototyping | 6 | 76% | device rotation, presentation scaling options, scroll momentum | 1–3 |
| Import/export | 6 | 58% | PDF (in progress), .fig import, REST import of conditional and variable-mode prototype actions | 13–25 |
| Dev mode / inspect | 2 | 78% | component code with properties as parameters, visual QA | 2–3 |
| Comments, versions, misc | 3 | 45% | comment polish, plugins (WASM) | 9–15 |

Weighted feature depth ≈ **68%** (self-graded, from the table).

## Milestones

| Milestone | Status |
|---|---|
| M0 Foundation: workspace, model, renderer, engine, UI shell, app, control channel | done (2026-10-09) |
| M0.9 CLI + MCP, web build, release packaging, PDF export | in progress |
| M1 Core editing completeness (tools, panels, menus, clipboard, files) | ~70% |
| M2 Layout depth | ~72% |
| M3 Components & styles depth | ~62% |
| M4 Vector & text depth | ~60% |
| M5 Prototype & present | ~72% |
| M6 Interop & dev mode | ~55% |
| M7 Release (signed builds for all platforms, README screenshots) | started |
