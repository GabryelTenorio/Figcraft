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
| Feature depth (weighted rubric below) | **~79%** |
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
   truncating. Right-to-left paragraphs mirror lists and indents: bullets and numbers (".1") on
   the right, wrapped lines hanging from the right, the paragraph indent on the right, trailing
   spaces hanging outside the line (canvas, PNG, PDF and SVG). *Left:* visual QA. 0.5 h.
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
   "Changed" when the contents are edited, resolved with a reason; Compare changes shows what a
   changed frame or section looked like when it was marked next to how it is now, with the
   changed layers and their property changes. *Left:* whether `fr` rows of a fixed-height grid
   should make room for fixed-size children (now `minmax(0, 1fr)`-like). 1–2 h.
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
   layout grids reordered by dragging their rows, their variable bindings following. Components at
   scale, QA'd with a synthetic design system (about 300 components: buttons, inputs, cards, navs
   and badges as sets of size × state × type × theme variants, boolean/text/instance swap/slot
   properties, instances nested four deep, text and paint styles, light/dark variables bound across
   it) used 2,000 times with overrides at every level: main edits keep every override and match a
   full rebuild exactly; values from variables (a theme switch, a bound corner radius) are no longer
   recorded as overrides; detaching a variable on an instance layer is an override that survives main
   edits; exposed nested instances stay exposed and swapped nested instances keep their layer names
   through main edits; layers a swapped nested instance's main adds keep their ids and overrides.
   Instance swap properties have preferred values (offered first). Alt-drop from the Assets panel
   onto an instance swaps it (same-set instance preferred, highlighted while dragging); instances
   pasted into other files follow their mains (library copies shared, published components used
   from their library, other mains left behind as a restorable missing main). The Assets panel lays
   out only the tiles in view and renders thumbnails as they show (cached, re-rendered only for
   changed components). *Left:* QA with real-world (not synthetic) libraries. 1 h.
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
   (add, reorder by dragging a row's grip, also inside conditional blocks, or from its menu, remove,
   open one to edit it; one undo step per move), "Add action ▸ Conditional" adds an if whose
   condition comes from a builder (a variable from the variable picker, an operator, a value or
   another variable) or is typed as an expression (`count >= 3 and not done`, checked when entered,
   what's wrong shown under the field), each block with its own actions (the same editors,
   nested), plus "Else if" and "Else"; set variable values can be expressions (`count + 1`,
   `"Hi " + name`); expressions read a variable's value in a given mode of its collection
   (`accent[Dark]`, also from REST files' variable mode lookups) and the mode a collection is in
   where the interaction runs (`mode(Theme) == "Dark"`, following set variable mode actions); every
   edit is an undo step (`proto.action.*`, `proto.conditional.*`). Hotspot hints: a click where
   nothing reacts flashes the screen's hotspots (an Options menu toggle, on by default); the
   presenter's Inspect panel shows the hovered layer's properties. Overlays opened through commands
   close on a click outside unless told not to, as in the editor. *Left:* QA against larger
   real-world prototypes. 1–2 h.
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
   content wins over our own copy; pasted SVG lands centred in the view). SVG import is editable and
   checked against a corpus of synthetic files (expected pixels worked out by hand, layer structure,
   export and re-import): groups named from their ids with transforms baked into the layers;
   rectangles (corner radius), ellipses and lines kept as those shapes; gradients with exact
   handles, `reflect`/`repeat` and focal points; patterns as image fills; clip paths and masks as
   vector, luminance and alpha masks; shadow and blur filter chains as drop shadows, inner shadows
   and layer blurs; blend modes; text as text layers with runs, anchors and stacked lines; images
   and nested SVG images; import warnings for what's left out; linked files never read; hostile
   input fuzzed. SVG export keeps alpha and luminance masks. PDF export: one page per frame in points (Export frames to
   PDF, the Export panel, `figcraft-cli export -o x.pdf`), vectors, gradients and strokes as
   vectors, real selectable text in subset fonts with clickable hyperlinks, images embedded once,
   opacity and blend modes, clip content, alpha/luminance/vector masks, shadows and layer and
   background blurs as images; checked by rendering the PDFs back against PNG exports. PDF print
   options in the Export panel (per PDF export setting), the Export dialog, `file.export`,
   `file.exportLayers`, `file.exportFramesPdf` and `figcraft-cli export`: a bleed showing what lies
   past the frame (its fills reach out, its children are clipped at the bleed), crop and bleed
   marks outside it, MediaBox / BleedBox / TrimBox, and CMYK colour (a documented, deterministic
   naive conversion; images and rasterised effects too).
   *Left:* .fig import (best effort), a CMYK output intent (no CMYK ICC profile can ship in the
   repository), SVG filters other than shadows and blurs (left out with a warning). 8–16 h.
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
   minutes, folded in the panel). Real-time collaboration is designed (`docs/collab-protocol.md`:
   WebSocket sessions the app can host itself, server-ordered per-property changes, live cursors)
   and its core landed (`crates/collab`, no networking yet): client-scoped node ids, every saved
   layer field and the document-level data (styles, variables, comments, images…) as synced
   properties, diffs after a command that cost the size of the edit, fractional sibling positions,
   a session server that orders and validates batches (cycles, deleted layers, id spaces, sizes)
   and a client that shows its own edits at once and replays them over others'; derived data
   (auto layout, instances, bindings) is repaired by the author after concurrent edits; binary
   messages (a moved layer is 43 bytes, a cursor 23) with a JSON form; a randomized multi-client
   test over real commands converges.
   Plugins landed: sandboxed WebAssembly plugins (a metered wasmi
   interpreter with a memory cap and no WASI, also in the browser) whose API is the command
   registry; each run is one undo step and is rolled back when it fails (error, trap, endless loop,
   memory); a `figcraft-plugin.json` manifest declares menu commands with parameters and read or
   write access with an optional command allowlist, and files, other documents, the undo history,
   dialogs and other plugins are always off limits. Plugins ▸ Development ▸ Import plugin from
   manifest… (kept in the preferences, the module re-read on every run), the installed plugins in
   the Plugins menu with a parameters form drawn from the manifest, Run last plugin (⌥⌘P), Manage
   plugins, and `plugin.*` commands for scripts, the CLI and MCP; a Rust SDK and two example
   plugins (Tidy names, Swatch grid) in `plugins/`, built by `cargo xtask plugins`. *Left:*
   plugin-defined UI at run time (forms or panels a plugin returns), plugin settings storage,
   distribution; for collaboration, the WebSocket transport, hosting and joining from the app,
   live cursors, per-user undo, reconnect, the browser client and a standalone server. 18–45 h.
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
   open). The retained canvas is held to a full render: a seeded random walk (edits, undo/redo,
   pans and zooms over nested, rotated and clipping frames, groups, masks, booleans, auto layout,
   effects, text, instances, variables and styles) checks every step, and found clip edges on a
   pixel boundary letting a column through in some renders. Drags show live on the canvas (every
   finished render is shown, not only the newest one asked for), and a button held still no
   longer rebuilds the preview and re-renders every frame.
   Components at scale (`cargo xtask bench`, synthetic design system of about 300
   components used by 2,000 instances): edits of mains reach their copies directly instead of
   rebuilding every instance (value, size and layer added/removed edits; other edits rebuild each
   affected instance once, inner mains first), bound variables no longer touch unchanged layers on
   every edit, auto layout re-flows only where sizes or places changed: recolouring or resizing a
   main's layer used ~900 times 188/226 → 4/18 ms, adding a layer to it 430 → 21 ms, a label style
   in all 72 button variants (1,800 copies) 450 → 57 ms, switching a variant, reset, detach, push
   and paste ~450 → under 3 ms, accepting a library update 460 → 64 ms; the Assets panel with 2,000
   assets 3.4 → 1.3 ms a frame. Accessibility: the custom-painted
   controls name themselves for screen readers (AccessKit on desktop: role, name, value and state
   for tools, layer rows with their type, level, selection and hidden/locked state, fields,
   dropdowns, tabs, menu items, and the canvas, which describes the selection); keyboard navigation:
   F6 / Shift+F6 move between the toolbar, layers, canvas and properties, Tab / Shift+Tab move
   within a region with a focus ring, Enter or Space activate, the arrows go along the toolbar and
   through the layer rows, menus and tool flyouts opened from the keyboard take it and give it back,
   Escape returns to the canvas (where Tab selects the next layer again instead of wandering into
   the tab strip); reduced motion follows the system's setting or a preference; secondary and
   accent text meet WCAG AA in both themes.
   An idle window draws no frames: it redrew ~240 times a second
   (the window title was re-sent every frame, and each viewport command asks for another), ~33% of
   a core in a debug build, and now sits at 0 frames and ~0% CPU; the toast, the autosave
   checkpoint and the presenter's "after delay" timers wake it at their deadline, the interaction
   editor's curve preview plays when it shows, changes or is hovered, and a screenshot keeps frames
   coming only until it arrives (`ui.inspect` `framesDrawn` and `repaintCauses` show what asked for
   frames; headless tests hold it). *Left:* a real-world file corpus, Windows/Linux/browser
   QA, runs with real screen readers (NVDA, Narrator, VoiceOver, Orca), the AccessKit tree in the
   browser (eframe's web backend doesn't send it yet), keyboard box selection, keyboard rename and
   context menus in the layers panel. 28–46 h.

### Where we're going

- **Next:** the fidelity pass (7) and hardening (9: a real-file corpus, platform QA); effects:
  glass and progressive blur tuned side by side; the last
  text details (right-to-left list markers) and auto layout details (grid `fr` rows around
  fixed-size children).
- **Then:** interop depth (6: .fig best effort), multiplayer (8; comments, version history and
  plugins are in).
- **Alpha:** release builds on every push to `release` (packaging in progress), demo files, docs.

## Shipped so far

- **Architecture:** 15 crates with enforced layering (`cargo xtask layers`): geom, color, doc,
  layout, text, pathops, plugin, collab, render, format, svg, engine, ui-egui (+ mcp, pdf). Every action
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
  with pinned properties and categories drawn as coloured callouts on the canvas (filtered by category),
  a ready-for-dev list, a focus view on one frame with previous / next, Compare changes since a frame
  was marked ready (before / after side by side or overlaid, property-level diffs), CSS/HTML/React/SwiftUI/Compose
  code where components take their properties as parameters and instances are calls), colour picker
  (HSB square, hue/alpha, hex, gradient stops, image fills), main menu generated from the catalog,
  context menu, Actions palette (⌘K), keyboard shortcuts from one keymap per platform (a tabbed,
  searchable Keyboard shortcuts panel), keyboard navigation (F6 between regions, Tab within them, a
  focus ring) and screen-reader names for the custom controls (AccessKit), presentation player with
  transitions, light and dark themes, home screen, About/Preferences/shortcuts/variables dialogs.
- **Automation:** desktop control channel (`--control <port>`: execute commands, inspect document/UI,
  synthetic pointer/keys/typing, menu invocation, screenshots, renders).
- **Tests:** ~390 automated tests including junk-parameter sweeps over every command (and every vector-edit command while editing), importer fuzzing and render regressions.

## Parity estimate (weighted rubric)

Scores by behaviour, 0–100%. Hours are what remains (one agent).

| Area | Weight | Score | Remaining work | Hours |
|---|---|---|---|---|
| Canvas & navigation | 6 | 82% | visual QA at extreme zooms | 0.5–1 |
| Selection & transform | 8 | 78% | measuring and transform handles checked side by side, rotated groups and mixed selections QA | 2–4 |
| Layers panel | 5 | 88% | visual QA of long lists and deep trees | 0.5–1 |
| Shapes & vector | 8 | 76% | live boolean editing, Design panel arc fields, shape builder on strokes and open paths, one-sided widths | 4–8 |
| Paint | 7 | 78% | a wide-gamut canvas (egui draws sRGB, so Display P3 colours outside sRGB clip on screen; exports keep them), images' own embedded colour profiles (images are taken as sRGB) | 1–3 |
| Strokes | 3 | 82% | stroke styles | 1–2 |
| Effects | 4 | 84% | glass and progressive blur tuned side by side; SVG draws noise, texture, progressive and background blur as embedded images | 2–4 |
| Frames, groups, sections, masks | 5 | 82% | visual QA | 0.5–1 |
| Auto layout & constraints | 9 | 76% | grid `fr` rows around fixed-size children, QA with real-world files | 1–2 |
| Text | 8 | 79% | visual QA | 0.5–1 |
| Components & variants | 9 | 79% | QA with real-world (not synthetic) libraries | 1 |
| Styles & variables | 5 | 89% | variant properties and gradient stops bound to variables, REST import of bindings past the first paint and on layout grids, a colour picker for layout grid colours | 1 |
| Prototyping | 6 | 91% | QA against larger real-world prototypes | 1–2 |
| Import/export | 6 | 71% | .fig import (best effort), SVG filters beyond shadows and blurs and rotated repeating patterns, a CMYK output intent for print PDFs (needs a CMYK ICC profile) | 8–16 |
| Dev mode / inspect | 2 | 91% | visual QA of the compare and focus views, comparing against an older named version (not only the snapshot taken when a frame first changes) | 1–2 |
| Comments, versions, misc | 3 | 62% | multiplayer; plugin-defined UI at run time, plugin settings storage | 6–11 |

Weighted feature depth ≈ **79%** (self-graded, from the table).

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
| M9 Real-time collaboration (WebSocket sessions hosted from the app, then a self-hosted server; live cursors; see `docs/collab-protocol.md`) | core done (ids, change sets, server ordering, convergence); networking next |
