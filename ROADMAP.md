# FigCraft Roadmap

FigCraft is a clean-room, open-source, pure-Rust reimplementation of the Figma design workflow. It
runs on macOS, Windows, Linux, FreeBSD and the web (WASM), and agents can drive it completely through
the CLI, a JSON control channel and MCP.

This file tracks **how far we are and what's left**. Time estimates are wall-clock hours of
continuous Claude Opus 5.5 agent work (including builds and the CI gate), for **one agent** and for
**4–6 parallel agents** on disjoint crates. They are counted from the remaining work in the parity
table below and updated as work lands.

_Last updated: 2026-10-10 (day 2, evening: measuring, scale tool handles and selection depth, effects fidelity (backdrop blur in clipping frames, exact shadow spread, glass, noise and texture), right-to-left names and the text style browser, comments polish, keyboard navigation in the layers panel, frame guides, 2x pixel preview, line-end previews)._

## Where we are

| Dimension | Status |
|---|---|
| Infrastructure (crates, command registry, undo, renderer, formats, control channel, tests) | **~72%** |
| Look & feel vs the incumbent's 2026 UI (measured side by side) | **~65%** |
| Menu/command breadth (`docs/parity.md`) | **97.5%** (197/202) of the catalogued menu tree has a command |
| Feature depth (weighted rubric below) | **~78%** |
| Parity including interaction fidelity and hardening | **~45–50%** |
| Time to **feature parity** | **~45–85 h** one agent · **~12–25 h** with 4–6 agents |
| Time to **full parity** (feature parity + fidelity pass + hardening) | **~115–195 h** one agent · **~32–55 h** with 4–6 agents |

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
   smooth interpolation per subpath, dashes, caps and joins, canvas/PNG/SVG/PDF and outline stroke),
   a transform box around several selected points (corners and edges scale them, Shift keeps
   proportions, Alt from the centre; just outside a corner rotates, Shift in 15° steps; drag inside
   to move; the Design panel's X/Y/W/H and flip/rotate act on the points), point snapping while
   dragging points or handles and with both pens (to the points and segment midpoints of this and
   other vectors, the usual layer targets and the pixel grid, with guides and a marker on the point
   landed on; Ctrl/Cmd turns it off), arc handles on the canvas (sweep, start and ratio with a
   live readout, Shift snapping, closing at the start, the other part from the gap; rotated and
   flipped ellipses, any zoom). *Left:* live boolean editing, one-sided widths, Design panel arc
   fields. 4–8 h.
2. **Text editing depth**: *landed:* OpenType features panel, small caps, lists, title case,
   truncation/max lines, font picker with previews and recents, word/line selection, IME commit,
   inline IME composition (underlined at the caret, reflowing, candidate window at the caret; the
   document only gets the committed text), variable fonts (axes and named instances read from the
   font, named instances as styles, Type settings ▸ Variable sliders, per-range axis values applied
   to shaping, outlines and PDF), range-aware Typography panel with "Mixed". Range fills in the Fill
   section. Right-to-left and mixed-direction text (Unicode bidi: Arabic shaping, mirrored brackets,
   caret, clicks and selection follow the runs; arrow keys move visually). Right-to-left and
   mixed-direction names in the panels (layers, pages, Assets, tabs, the file header, canvas frame
   titles: visual order, joined Arabic, the ellipsis at a right-to-left name's left end; rename fields
   whose caret, clicks, drags and arrow keys follow the shown text; system fonts for scripts the UI
   fonts lack). The text style browser in the Typography section (list and grid views in each
   style's own font, search, folders, this file's and enabled libraries' styles, apply, detach, edit
   the typography, create from the selected chars, hover to preview on the canvas, "Mixed" when the
   chars use several styles); text styles link chars, not just layers (editing what a style sets
   unlinks the chars edited). Live previews of a hovered named instance or font style and of a
   dragged axis slider (one undo step on release, Escape cancels). Visual QA: list markers keep the
   paragraph's font, renames start with the name selected and centred, Max lines shows while
   truncating. *Left:* list markers on the right of right-to-left paragraphs. 1 h.
3. **Auto layout and sections depth**: *landed:* on-canvas padding/gap handles (also wrapped-row
   and grid column/row gaps, grid tracks outlined), min/max width/height (UI, Hug/Fill/Fixed,
   hugging text wraps at its max), ignore auto layout (absolute position) with constraints, text
   baseline alignment, grid tracks (fixed/hug/`fr`), grid cells and spans, wrap row gap UI,
   wrap-aware drag-reorder (by row) and dragging into a grid cell, nested hug/fill settling in one
   pass (wrapping fill text, three levels deep, grid rows), min/max redistribution, coherent
   Fill/Hug rules, drag across nested auto layout frames (deepest one wins, insertion line, out to
   the canvas or a plain frame). Arrow keys move auto layout children one place (along the flow
   past hidden and absolute siblings, Up/Down between wrapped rows, to the neighbouring grid cell
   (automatically placed children swap, anchored ones take or swap cells), and at the end of a
   flow or across a row's axis into the neighbouring nested auto layout frame when the frame
   around flows that way; one undo step, none when nothing can move). Visual QA of nested
   hug/fill/fixed/min/max/wrap/grid samples against the documented rules (Fill turns a hugging
   parent Fixed, min/max redistribution, wrap inside fill, absolute children following
   constraints as hug frames grow). Sections: hide contents (name, outline and "Contents hidden"
   on the canvas; contents skipped by rendering, hit testing and labels; dimmed in the layers
   panel; Design panel checkbox and context menu), Wrap in new section, frames in sections get
   canvas labels and are top-level frames (flows start there, navigation, Export frames to PDF),
   "Navigate to" a section shows its frame shown last, else its first; ready for dev turns
   "Changed" when the contents are edited, resolved with a reason. *Left:* whether `fr` rows of a
   fixed-height grid should make room for fixed-size children (now `minmax(0, 1fr)`-like),
   section-level comparisons of changes. 1–2 h.
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
   modes in the Design panel: a frame's mode per collection, Auto (inherited) or its own, with
   enabled libraries' collections listed under the library's name (picking a mode copies the
   collection in). Instance boolean and text properties bound to boolean and string variables
   (following modes, renames, reset and detach); layout grid settings in the Design panel whose
   count, size, margin/offset and gutter bind to number variables; fills, strokes, effects and
   layout grids reordered by dragging their rows, their variable bindings following. *Left:* QA with larger real-world libraries. 1 h.
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
   coasts, a drag past the ends stretches and springs back); responsive presentation (the presented
   frame resized to the device's screen, or the window without a device, its constraints and auto
   layout responding live; in the Options menu and the Prototype panel, saved in the file); device
   colours (each model's colour variants, bodies drawn in code) and custom device sizes (a bare
   screen with width and height fields, imported from REST files); flow descriptions in the
   presenter's flow picker and under each flow; conditional actions (if / else if / else over
   variable expressions) and set-variable-mode actions run in the presenter (from REST files or
   commands). Conditionals are built in the interaction editor: an interaction's actions are a list
   (add, reorder, remove, open one to edit it), "Add action ▸ Conditional" adds an if whose
   condition comes from a builder (a variable from the variable picker, an operator, a value or
   another variable) or is typed as an expression (`count >= 3 and not done`, checked when entered,
   what's wrong shown under the field), each block with its own actions (the same editors,
   nested), plus "Else if" and "Else"; set variable values can be expressions (`count + 1`,
   `"Hi " + name`); every edit is an undo step (`proto.action.*`, `proto.conditional.*`). Hotspot
   hints: a click where nothing reacts flashes the screen's hotspots (an Options menu toggle, on by
   default); the presenter's Inspect panel shows the hovered layer's properties. *Left:* dragging
   actions to reorder them (a menu moves them now), QA against larger prototypes with
   conditionals. 2–4 h.
6. **Interop**: *landed:* image crop handles; REST API file JSON import (`file.open` / `file.import`
   / CLI: pages, layer tree, geometry, paints, strokes, effects, text runs, vectors from geometry
   paths, auto layout, constraints, grids, components, sets, instances with overrides, component
   properties, styles; image fills from an `images` folder or placeholders; warnings for the rest),
   now with prototyping (interactions with their triggers, actions, transitions and easing, flows,
   scrolling, the prototype device; conditional actions (if / else if / else over expressions),
   set variable with variable references and expressions, set variable mode, all run by the
   presenter) and variables (the local variables JSON passed along, next to the file or embedded:
   collections, modes, aliases, scopes, layer bindings and explicit modes).
   Export panel: a row per export setting (typed or preset sizes like `2x` / `512w` / `512h`, PNG,
   JPG, WebP, SVG and PDF, suffix), a preview, and "Export N layers" (`file.exportLayers`) in the
   panel and the Export dialog. Paste from other apps: images, SVG and text (the newest clipboard
   content wins over our own copy). PDF export: one page per frame in points (Export frames to
   PDF, the Export panel, `figcraft-cli export -o x.pdf`), vectors, gradients and strokes as
   vectors, real selectable text in subset fonts with clickable hyperlinks, images embedded once,
   opacity and blend modes, clip content, alpha/luminance/vector masks, shadows and layer and
   background blurs as images; checked by rendering the PDFs back against PNG exports.
   *Left:* .fig import (best effort), variable mode lookups in prototype expressions, PDF print
   options (bleed, CMYK). 10–20 h.
7. **Look & feel and interaction-fidelity pass**: *landed:* keyboard shortcuts audited against the
   public shortcut reference: one keymap (`figcraft_engine::keymap`) drives the menus, the canvas,
   the control channel and MCP, with macOS and Windows/Linux keys where they differ (mask, bring to
   front/back, distribute, tidy up, layout grids, pixel preview), Text-only and canvas-only bindings
   (Cmd/Ctrl+K: link while typing, Actions on the canvas), Cmd/Ctrl+/ for Actions, `+`/`-` zoom,
   Shift+0/1/2 from any layout, Cmd/Ctrl+0, hold Z for the zoom tool (Alt zooms out, drag an area),
   two quick digits for an exact opacity, boolean keys (Alt+Shift+U/S/I/E), remove fill/stroke
   (Alt+/, Shift+/), outlines on Shift+Cmd+O, pixel grid on Cmd+', Shift+E / Shift+D / Shift+Cmd+   panel keys, Ctrl+Tab between files, B for the paint bucket in vector edit mode; the Keyboard
   shortcuts panel docks at the bottom with a tab per group and search, keycaps per platform.
   *Left:* tool by tool, panel by panel against the incumbent (cursors, modifier keys while
   dragging such as Ctrl to ignore snapping on macOS, hover states, menus, popovers); multiplayer
   cursor keys wait for multiplayer. 40–60 h.
8. **Collaboration (local):** comments landed: pins pinned to frames and layers that move with them
   (to other pages too) and stay where they were when the layer is deleted, your pins dragged onto
   another frame, crowded pins clustered when zoomed out (a click zooms them apart), Shift+C to show
   or hide them; threads where you edit and delete your own messages, emoji reactions, @mentions
   with a picker of the names used in the file (your name from Preferences), read and unread state,
   resolve, and "Copy link" (`figcraft://comment/<id>`: pasting or `comment.open` goes to the page,
   zooms to the frame and opens the thread); the Comments panel with search (text, names, frame),
   filters (open, resolved, yours, unread), sorting by date or page, each thread's frame and page,
   and Up/Down to step through threads; comments on the presented frame (Comments button or
   Shift+C: read and reply). All of it through `comment.*` commands. Version history
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

- **Next:** the fidelity pass (7) and hardening (9: a real-file corpus, platform QA); effects:
  glass and progressive blur tuned side by side; the last
  text details (right-to-left list markers) and auto layout details (grid `fr` rows around
  fixed-size children).
- **Then:** interop depth (6: .fig best effort), plugins and multiplayer (8; comments and version
  history are in).
- **Alpha:** release builds on every push to `release` (packaging in progress), demo files, docs.

## Shipped so far

- **Architecture:** 13 crates with enforced layering (`cargo xtask layers`): geom, color, doc,
  layout, text, pathops, render, format, svg, engine, ui-egui (+ mcp, pdf). Every action
  is a command (~330); undo is unlimited via structural sharing (`Arc` node snapshots); gestures are
  one undo step; `command.batch` runs several commands as one step.
- **Document model:** pages, frames, groups, sections, rectangles, ellipses (arcs/rings), polygons,
  stars, lines/arrows, vector networks, text with styled runs, boolean groups, components, component
  sets (variants), instances with per-layer overrides, slices; fills (solid, linear/radial/angular/
  diamond gradients, images with fill/fit/crop/tile + adjustments), multiple strokes with
  inside/center/outside alignment, per-side weights, dashes, caps/arrowheads, joins; effects (drop
  and inner shadow with spread that offsets the outline exactly: ellipses, stars, vectors, text;
  inner shadows over the fills and under the children and strokes; layer blur; background blur
  that sees the whole backdrop, inside clipping frames, faded groups and masks too; both blurs
  uniform or progressive (the radius ramps between two points, dragged on the canvas); noise in
  mono, duo and multi colour with grain size, density and blend mode; texture with clip to shape;
  glass with light angle and intensity, refraction, depth, dispersion, frost and splay; the same in
  PNG, PDF and SVG exports, SVG embedding renders of noise grains, texture, progressive and
  background blur and glass); corner radii with
  smoothing; constraints; auto layout (horizontal, vertical, wrap, grid; hug/fill/fixed; min/max;
  absolute children); layout grids; export settings; prototype interactions, flows and transitions;
  paint/text/effect/grid styles; variables with collections, modes and aliases.
- **Renderer:** CPU (tiny-skia) scene renderer used for canvas, export and the web: isolated layers
  for opacity/blend/effects, clip content, masks, booleans (linesweeper), text (harfrust shaping,
  skrifa outlines, font fallback), aligned strokes with caching.
- **Engine:** pointer tools (move/marquee/deep select/resize/rotate/Alt-duplicate with smart-guide
  snapping, frame/section/slice/shapes/line/arrow with Shift/Alt, text, pen, pencil, hand,
  eyedropper), Cmd/Ctrl-click and Cmd/Ctrl-marquee into nested layers, Shift-marquee toggling,
  Select layer (every layer under the click), measuring (Alt-hover distances from the selection to
  layers, parents and ruler guides, Cmd+Alt for nested layers, extension lines, one box inside the
  other, overlaps, live distances to the neighbours while Alt-dragging, Shift+Alt for auto layout
  padding and gaps; labels that never overlap at any zoom), text editing (caret, selection, words,
  lines), auto layout reorder by dragging, drop into frames, align/distribute/tidy, z-order,
  flip/rotate, the scale tool (panel, and canvas handles that scale everything about its anchor
  point or from the centre with Alt, with a live factor, one undo step per drag), booleans, flatten,
  outline stroke, components (create, instances, sync with overrides, reset, detach, push changes,
  variants, properties, slots), styles and variables, prototype commands, find/replace, batch rename,
  copy/paste (internal + SVG/text), file new/open/save/import/export (native `.figcraft`, SVG
  in/out, PNG/JPG/WebP), version snapshots.
- **UI (egui, swappable):** the incumbent's 2026 layout: tab strip that is also the window's title
  bar (frameless on Windows/Linux with drawn window buttons, drag, double-click maximise and edge
  resizing; macOS window buttons inset into it), icon rail, file header, pages and
  layers tree (rename, visibility/lock, drag to reorder/reparent), Assets panel, floating bottom
  toolbar with flyouts and mode switcher, Design / Prototype / Inspect panels (Dev Mode: properties with their styles and variables, annotations
  with pinned properties drawn as callouts on the canvas, a ready-for-dev list, CSS/HTML/React/SwiftUI/Compose
  code where components take their properties as parameters and instances are calls), colour picker
  (HSB square, hue/alpha, hex, gradient stops, image fills), main menu generated from the catalog,
  context menu, Actions palette (⌘K), keyboard shortcuts from one keymap per platform (a tabbed,
  searchable Keyboard shortcuts panel), presentation player with
  transitions, light and dark themes, home screen, About/Preferences/shortcuts/variables dialogs.
- **Automation:** desktop control channel (`--control <port>`: execute commands, inspect document/UI,
  synthetic pointer/keys/typing, menu invocation, screenshots, renders).
- **Tests:** ~390 automated tests including junk-parameter sweeps over every command (and every vector-edit command while editing), importer fuzzing and render regressions.

## Parity estimate (weighted rubric)

Scores by behaviour, 0–100%. Hours are what remains (one agent).

| Area | Weight | Score | Remaining work | Hours |
|---|---|---|---|---|
| Canvas & navigation | 6 | 80% | visual QA at extreme zooms | 1 |
| Selection & transform | 8 | 78% | measuring and transform handles checked side by side, rotated groups and mixed selections QA | 2–4 |
| Layers panel | 5 | 88% | visual QA of long lists and deep trees | 0.5–1 |
| Shapes & vector | 8 | 76% | live boolean editing, Design panel arc fields, shape builder on strokes and open paths, one-sided widths | 4–8 |
| Paint | 7 | 78% | a wide-gamut canvas (egui draws sRGB, so Display P3 colours outside sRGB clip on screen; exports keep them), images' own embedded colour profiles (images are taken as sRGB) | 1–3 |
| Strokes | 3 | 82% | stroke styles | 1–2 |
| Effects | 4 | 84% | glass and progressive blur tuned side by side; SVG draws noise, texture, progressive and background blur as embedded images | 2–4 |
| Frames, groups, sections, masks | 5 | 80% | comparing what changed in ready-for-dev frames and sections, visual QA | 1–2 |
| Auto layout & constraints | 9 | 76% | grid `fr` rows around fixed-size children, QA with real-world files | 1–2 |
| Text | 8 | 78% | list markers on the right of right-to-left paragraphs, visual QA | 1–2 |
| Components & variants | 9 | 76% | QA with larger real-world libraries | 1 |
| Styles & variables | 5 | 89% | variant properties and gradient stops bound to variables, REST import of bindings past the first paint and on layout grids, a colour picker for layout grid colours | 1 |
| Prototyping | 6 | 89% | dragging actions to reorder them, QA against larger prototypes with conditionals | 2–4 |
| Import/export | 6 | 66% | .fig import (best effort), variable-mode lookups in prototype expressions, PDF print options (bleed, CMYK) | 10–20 |
| Dev mode / inspect | 2 | 84% | changes since ready for dev (compare view), focus view on one frame, annotation categories, visual QA | 1–2 |
| Comments, versions, misc | 3 | 55% | plugins (WASM), multiplayer | 8–14 |

Weighted feature depth ≈ **78%** (self-graded, from the table).

## Milestones

| Milestone | Status |
|---|---|
| M0 Foundation: workspace, model, renderer, engine, UI shell, app, control channel | done (2026-10-09) |
| M0.9 CLI + MCP, web build, release packaging, PDF export | CLI, MCP, web build and PDF export done; release packaging in progress |
| M1 Core editing completeness (tools, panels, menus, clipboard, files) | ~70% |
| M2 Layout depth | ~76% |
| M3 Components & styles depth | ~70% |
| M4 Vector & text depth | ~66% |
| M5 Prototype & present | ~82% |
| M6 Interop & dev mode | ~62% |
| M7 Release (signed builds for all platforms, README screenshots) | started |
