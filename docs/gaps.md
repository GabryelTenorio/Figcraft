# Where FigCraft falls short of Figma

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (`.fig` import, best effort: gap 1 down to its tail; Windows QA pass: everyday workflows on Windows 11, eight fixes incl. unsaved work kept after a crash or a forced quit; Display P3: gamut warning and soft-proof (true P3 blocked upstream); several windows; localization layer with a draft Spanish catalog; hosting figcraft-server: wss in the desktop app, server releases, deployment guide; plugin panels and settings; canvas fixes: boolean hit testing on the result, group drags, frame outlines and slices; code connect; stretch and scatter brushes, pen pressure; text on a path and pattern fills; team libraries on the server and swap library; branches: create, update from main, review and merge; animated GIF fills and TIFF images; Open recent, outlines for the selection and mask outlines: menu breadth 100%; extended collections, variables import/export, gradient stop and variant bindings: gap 14 down to its tail) · **Target:** Figma Design (desktop app 126.10.7)

Every known shortfall against Figma, one entry each, ranked by how much it stops a Figma user from
doing real work in FigCraft. Agents: pick work from the top unless the owner says otherwise. The
numbers behind the ranking are in [`target-app-parity.md`](target-app-parity.md). Hours are one
Opus 5.5 agent working sequentially, tests included, calibrated as described there. "Estimated"
everywhere unless marked measured.

## Ranked

No alpha blockers: the alpha gate in [`roadmap.md`](roadmap.md#alpha-gate) passes. Beta blockers are marked.

| # | Gap | Kind | Hours | Doc |
|---|---|---|---|---|
| 1 | `.fig` files: best-effort import landed; editable vector networks, more layer kinds, real-file checks and export left (**beta blocker** until checked on real files, gap 3) | File format | 8–15 | [file-format-parity](file-format-parity.md) |
| 2 | Multiplayer and sharing are a preview, without accounts (**beta blocker**) | Feature | 45–90 | [collaboration-parity](collaboration-parity.md) |
| 3 | Nothing checked against real Figma files | Fidelity | 15–30 | [target-app-parity](target-app-parity.md) |
| 4 | Interaction fidelity not compared tool by tool | UI/UX | 40–60 | [ui-parity](ui-parity.md) |
| 5 | Localization: English and a Spanish draft only (the layer landed 2026-10-11) | Localization | 45–75 | [localization-parity](localization-parity.md) |
| 6 | Plugins: no distribution or widgets; Figma's plugins don't run | Ecosystem | 15–40 | [target-app-parity](target-app-parity.md) |
| 7 | CPU canvas where Figma uses the GPU | Hardware/performance | 30–60 | [hardware-parity](hardware-parity.md) |
| 8 | Platform QA: Linux and the browser left (Windows pass done 2026-10-11) | Stability/platforms | 10–20 | [hardware-parity](hardware-parity.md) |
| 9 | Figma Draw: multi-edit, pen pressure on macOS/Linux tablets (brushes, pressure, text on a path and pattern fills landed) | Feature | 2–5 | [target-app-parity](target-app-parity.md) |
| 10 | Team libraries: no team-wide lists or accounts; analytics are counts | Feature | 4–8 | [components-parity](components-parity.md) |
| 11 | Spell check | Feature | 4–8 | [target-app-parity](target-app-parity.md) |
| 12 | Video fills; HEIC images | File format | 5–10 | [file-format-parity](file-format-parity.md) |
| 13 | Vector tail: none left (boolean hit testing on the result landed 2026-10-11) | Feature | — | [target-app-parity](target-app-parity.md) |
| 14 | Variables: extended collections' tail | Feature | 1–2 | [components-parity](components-parity.md) |
| 15 | Prototyping: media triggers and actions, gamepad | Feature | 3–6 | [prototyping-parity](prototyping-parity.md) |
| 16 | Dev Mode: dev resources links, editor extension (code connect and Dev Mode plugins landed) | Feature | 1–3 | [dev-mode-parity](dev-mode-parity.md) |
| 17 | Branching: per-branch version history, finer merges of list properties (branches landed 2026-10-11) | Feature | 1–3 | [collaboration-parity](collaboration-parity.md) |
| 18 | Display P3 on screen: blocked upstream (egui-wgpu picks an 8-bit sRGB surface); gamut warning and soft-proof sRGB landed, as did several windows | Hardware | 4–8 | [hardware-parity](hardware-parity.md) |
| 19 | AI features | AI | 30–60 | [target-app-parity](target-app-parity.md) |
| 20 | Mobile apps (view, mirror, comment) | Platforms | 20–40 | [hardware-parity](hardware-parity.md) |
| 21 | Menu items without a command: none left (landed 2026-10-11) | Feature | — | [parity-checklist](parity-checklist.md) |
| 22 | Screen readers never run; browser has no accessibility tree | UI/UX | 4–8 | [ui-parity](ui-parity.md) |
| 23 | CMYK output intent for print PDFs | File format | 2–4 | [file-format-parity](file-format-parity.md) |

## Entries

### 1. `.fig` files: the import's tail
- **Landed (2026-10-11):** `.fig` files open (best effort): the zip, its deflate/zstd chunks and the kiwi schema are decoded generically and mapped by field name onto the REST importer, so pages, the layer tree, geometry, paints (styles resolved), strokes, effects, text with styled runs, vectors (as outlines), auto layout (grid included), constraints, components, sets, instances (expanded from their main with overrides, derived layout and property values), styles, variables with modes and bindings, prototyping and image fills from the archive come across; anything else is a warning (`crates/format/src/fig`). `file.open`,
  `file.import`, drag and drop, the CLI and MCP's `open_file` read `.fig`; the file is never saved
  over.
- **Missing:** vectors come in as their outlines (fill or stroke geometry), not editable vector
  networks with per-vertex caps; sticky notes, connectors, tables, slides and other non-design layer
  kinds are skipped; set-variable and conditional prototype actions, variables bound to fields other
  than the common ones, non-flexible grid tracks and video fills aren't read; text lays out again with
  local fonts. Nothing has been checked against real-world files yet (gap 3). Export to `.fig` is
  not started.
- **Evidence:** fixtures generated in code (`crates/format/src/fig/tests.rs`: kiwi round trips,
  every mapped feature, truncation, bit flips and junk never panic); `crates/engine/src/fig_import_tests.rs`.
- **Impact:** existing work can be brought across, but only real files will show how much of it.
- **Estimate:** 8–15 h for the read tail (vector networks, the skipped layer kinds that matter,
  real-file fixes); export to `.fig` a further 15–30 h and lower priority.

### 2. Multiplayer and sharing are a preview, without accounts
- **Missing:** accounts, teams and projects, per-person permissions, spotlight, cursor chat, audio,
  shared comments and version history. Comments and versions are local to the file.
- **Evidence:** the core landed in #103 (`crates/collab`: client-scoped ids, change sets, server
  ordering and validation, client reconciliation, binary messages, randomized convergence tests over
  real commands), and live sessions run over the network (`crates/collab-net`: hosting from the
  app, invite links that open the app, document sync between apps, live cursors, selections,
  text carets, avatars and follow, and the browser build joins) with per-person undo, refused
  edits rolled back with a reason, reconnects that resume and send what was made offline, and
  styles, variables and comments that several people create at once all kept; a standalone
  `figcraft-server` keeps many files (snapshot plus log, compacted; restarts lose nothing
  confirmed), takes files published from the app and hands out share links that let people edit
  or only view; the desktop app and the browser reach it over `wss://` through a TLS reverse
  proxy, and releases ship it with a deployment guide (`docs/server.md`). But there are no
  accounts (a link is the access) (`docs/collab-protocol.md`).
- **Impact:** Figma's defining feature; most teams use it every day. Solo designers can work
  without it.
- **Estimate:** 45–90 h: accounts, teams and per-person permissions, shared comments and versions
  (numbers from before the server landed; re-measured next).

### 3. Nothing checked against real Figma files
- **Missing:** a corpus of real documents (design systems, app screens, prototypes) imported and
  compared with Figma's own PNG/SVG/PDF exports.
- **Evidence:** every corpus in the repo is synthetic (`crates/svg/tests/corpus`, the synthetic
  design system in `crates/engine/src/synthetic_ds.rs`). Scores were self-graded by the agents that
  built the features.
- **Impact:** unknown fidelity in auto layout sizing, text metrics, effects and component
  resolution on real files; the numbers in this repo can't be trusted beyond "estimated" until it
  exists.
- **Estimate:** 15–30 h once `.fig` or REST import of real files is available; needs files whose
  licence allows use (owner to supply or approve; never committed if not redistributable, see
  craftrules `standards/test-corpora.md`).

### 4. Interaction fidelity not compared tool by tool
- **Missing:** a side-by-side pass of every tool and panel: cursors, modifier keys while dragging
  (e.g. Ctrl to ignore snapping on macOS), hover states, popovers, menus, snapping thresholds.
- **Evidence:** ROADMAP item 7 ("Left: tool by tool, panel by panel against the incumbent").
- **Impact:** Figma users notice the feel within minutes; it decides whether they stay.
- **Estimate:** 40–60 h. Detail in [`ui-parity.md`](ui-parity.md).

### 5. Localization: English and a Spanish draft only
- **Missing:** Figma's other UI languages (Japanese, Korean, French, German, Portuguese (Brazil)), a
  native review of the Spanish draft, localized engine messages and number formatting.
- **Evidence (measured from source):** the layer landed 2026-10-11: catalogs in
  `crates/ui-egui/locales/` (1,280 UI keys plus 501 command labels), Preferences ▸ Language,
  `pref.language`; `en.toml` is complete and `es.toml` is a complete machine draft. Engine error
  messages and Dev Mode values stay English.
- **Impact:** keeps out non-English designers, including Figma's large Japanese and Korean user
  bases.
- **Estimate:** 3–7 h per language plus native review, 4–6 h for engine messages and numbers;
  45–75 h for the 11 key languages. Detail in [`localization-parity.md`](localization-parity.md);
  how to add one in [`translating.md`](translating.md).

### 6. Plugins: no distribution or widgets; Figma's plugins don't run
- **Landed (#101):** sandboxed WebAssembly plugins (wasmi, metered, memory cap, no WASI, also in
  the browser) acting through the command registry, one undo step per run, manifest permissions,
  Plugins menu, Run last plugin, Manage plugins, `plugin.*` commands, a Rust SDK and two examples
  ([`plugins.md`](plugins.md)). Plugin panels and settings (ABI v2, 2026-10-11): a plugin shows a
  panel of fields and buttons drawn with FigCraft's widgets and is called back for each button or
  changed value (one undo step per callback, same sandbox limits), driven headless with
  `plugin.ui` / `plugin.event` / `plugin.close`; per-plugin settings kept with the preferences.
- **Missing:** distribution (a plugin directory), widgets, Community files; panels are one at a
  time with no colour picker or images. Figma's own plugins are JavaScript against Figma's plugin
  API and don't run.
- **Impact:** teams that depend on specific Figma plugins (icons, content, accessibility checks)
  can't bring them. FigCraft's CLI, control channel and MCP server cover agent automation better
  than Figma does.
- **Estimate:** 15–40 h for the missing pieces of FigCraft's own API. Running Figma's plugins
  needs a JS engine and an API shim: **owner decision needed**.

### 7. CPU canvas where Figma uses the GPU
- **Missing:** GPU rendering of the canvas. FigCraft renders with tiny-skia on the CPU (banded,
  parallel, retained) and shows the image through wgpu.
- **Evidence (measured, `cargo xtask bench`):** a 50k-layer page ~8 ms zoomed out with levels of
  detail, single-layer edits at 10k layers ~0.2 ms; full 1440×900 renders 7–15 ms at 10k layers.
  Heavy effects (glass, background blur, noise) and 4K/5K canvases at 2x are where CPU time grows.
- **Impact:** large files and effect-heavy designs; battery on laptops.
- **Estimate:** 30–60 h for a GPU path (vello or wgpu shaders) held to the CPU renderer as the
  oracle.

### 8. Platform QA: Linux and the browser left
- **Missing:** regular runs on Linux and the browser; real screen readers; a repeat of the Windows
  pass on other machines (scale factors above 100%, pen tablets, file drops from Explorer).
- **Evidence:** CI tests on Linux only (`.github/workflows/ci.yml`); release builds exist for every
  platform (v0.2.0) but no issue has been filed yet. Windows: QA pass done 2026-10-11, findings in
  PR (Windows 11, the desktop app driven over the control channel, the same workflow through MCP):
  eight bugs fixed, among them unsaved changes lost when the app was killed or crashed,
  Ctrl+arrows and Ctrl+Backspace acting on the whole line in text, scrubbed fields snapping back
  and skewed linear gradients.
- **Impact:** unknown bugs on the platforms where FigCraft is most attractive (Linux has no Figma
  desktop app).
- **Estimate:** 10–20 h, partly human (hardware).

### 9. Figma Draw: multi-edit and tablet pressure
- **Missing:** multi-edit; pen pressure on macOS and Linux tablets (winit reports none there).
- **Landed:** text on a path (`text.onPath`, `text.pathOptions`, `text.removeFromPath`; the
  Design panel's Typography section; canvas, PNG, SVG and PDF as outlines; SVG `<textPath>` imports
  as text on a path), pattern fills (`PaintKind::Pattern`, `fill.pattern`, the colour picker's
  Pattern tab; SVG `<pattern>`, PDF tiling patterns), stretch and scatter brushes (`stroke.brush`,
  eight presets drawn in code; canvas, PNG, SVG, PDF and outline stroke) and pen pressure for the
  Pencil (Windows pens and browser styluses; recorded as the stroke's width profile, which brushes
  follow).
- **Evidence:** no pressure input anywhere; strokes have uniform and variable widths only.
- **Impact:** illustration-flavoured work moved into Figma Draw in 2025.
- **Estimate:** 5–10 h.

### 10. Team libraries: no team-wide lists or accounts; analytics are counts
- **Missing:** libraries listed for a whole team or organization (each file enables a library by
  its link), accounts and per-person library permissions, analytics beyond instance counts per
  component and file (insertions and detaches over time), update notices pushed by the server
  (apps ask every 30 seconds while the Assets panel shows).
- **Evidence:** a file's library is published to `figcraft-server` under the file and enabled in
  other files by a share link, with per-asset versions, updates with overrides kept, usage counts
  and swapping a whole library by names ([`server.md`](server.md#team-libraries),
  [`components-parity.md`](components-parity.md)).
- **Impact:** design-system teams.
- **Estimate:** 4–8 h, the accounts part after gap 2.

### 11. Spell check
- **Missing:** spelling underline and suggestions while editing text.
- **Evidence:** no spell-check code in the repo.
- **Estimate:** 4–8 h with a permissively licensed dictionary and checker (provenance per
  `ATTRIBUTION.md`).

### 12. Video fills; HEIC images
- **Landed (2026-10-11):** animated GIF fills (the file kept as is, the first frame on the canvas
  and in exports, played in the presenter and the preview) and TIFF images.
- **Missing:** Figma takes JPG, PNG, HEIC, WebP, GIF and TIFF images and MP4, MOV and WebM video
  (help center, checked 2026-10-10). FigCraft decodes PNG, JPEG, WebP, GIF, TIFF and BMP; HEIC and
  video need decoders that are C libraries (libheif, FFmpeg) or each platform's media framework.
- **Estimate:** 5–10 h (video playback in the presenter is most of it; it can reuse the GIF clock).

### 13. Vector tail: none left
- **Landed (2026-10-11):** boolean groups are clicked where their result is (a click in a
  subtracted hole reaches what's behind; with the group or an operand selected its operands are
  hit, so each can still be picked; "Select layer" lists the group only where its result is), live
  boolean editing (operands selected by double-click or Cmd/Ctrl-click;
  moving, resizing or editing an operand's points redraws the result during the drag, with every
  operand outlined; per-operand operations from the boolean controls and `node.set`; a drag frame
  recombines only the operands from the edited one up), Design panel Arc fields (Start, Sweep,
  Ratio; Mixed across ellipses; bindable), the shape builder on strokes and open paths (strokes take
  part as their outlined areas), one-sided variable widths (Alt-drag a width handle; canvas, PNG,
  SVG, PDF and outline stroke).
- **Estimate:** none left (vector QA with real files is part of gap 3).

### 14. Variables: extended collections' tail
- **Landed:** extended collections (`library.extendCollection`: a library collection's modes and
  variables in a collection of this file, values overridden per mode with reset, following library
  updates, variables of its own), variables import/export (`variable.export` / `variable.import`:
  the REST local variables JSON, merged by name; CSS custom properties), gradient stops and variant
  properties bound to variables.
- **Missing:** variables a library adds to an extended collection arrive only with the library's
  next other update (they don't count as an update of their own); an extended collection can't be
  extended again; checked with synthetic files only.
- **Estimate:** 1–2 h.

### 15. Prototyping: media triggers and actions, gamepad
- **Missing:** video triggers (on media hit/end) and play/pause/mute/seek actions; gamepad input.
  The REST importer warns and skips them (`crates/format/src/rest/proto.rs`).
- **Estimate:** 3–6 h after video fills (gap 12).

### 16. Dev Mode: dev resources links, editor extension
- **Landed (2026-10-11):** code connect: a component or set links to its code component per
  language (React, SwiftUI, Compose): an import path, the name and a template whose `{property}`
  and `{property ? "a" : "b"}` placeholders take the instance's values, escaped per language and
  never executed. `inspect.code`, `inspect.componentCode` and the Inspect panel show that snippet
  instead of generated code (inside generated layer trees too); the panel's Connect to code edits
  it with a live preview. Links are stored on the component (saved, synced, undoable) and travel
  as `figcraft.connect.json` (`codeConnect.export` / `codeConnect.import`, validated and capped)
  so code owners can version them. Plugins whose manifest asks only to read run from the Inspect
  tab.
- **Missing:** dev resources links on layers; import lines for connected instances inside a
  generated layer tree (only the selected layer's own import is shown); publishing mappings from
  the code repository with a CLI. An editor extension is out of scope: the MCP server already
  serves design data, code and the connected snippets to agents and editors.
- **Estimate:** 1–3 h.

### 17. Branching and merging
- **Landed (2026-10-11):** branches of a file (`branch.*`): File ▸ Create branch… (or the Branches
  list in version history) copies the file into a branch tab that remembers the file as it was (its
  base) and makes its ids in its own id space; edits on the branch are saved with the main file.
  Review and merge (the banner on a branch tab) shows the main file and the branch side by side or
  as an overlay, what changed on each side since the base (layers with their property changes as
  Inspect shows them; styles, variables, comments) and the conflicts: the same property changed on
  both sides, or a layer deleted on one side while the other edited it or what's inside it. Each
  conflict is resolved for the branch or main; nothing conflicting merges until it is. Merging is
  one undo step on the main file and archives the branch; Update from main brings main's changes
  into the branch the same way; branches can be archived without merging. Three-way merge over the
  collaboration change sets (`figcraft_collab::merge`), fuzz-tested with random real commands.
- **Missing:** a branch's own version history; a list property (fills, text runs…) changed on both
  sides conflicts as a whole; branches of a file in a live session are refused.
- **Estimate:** 1–3 h.

### 18. Display P3 on screen (several windows landed)
- **Landed (2026-10-11):** several windows: File ▸ New window, a tab's Open in new window or a tab
  dragged off the strip; each window has its own tabs, panels and canvas and draws only when it
  needs to; closing one asks about unsaved changes; `window.new`, `window.list`, `window.focus` and
  `window` on any control request.
- **Missing:** wide-gamut colour on the canvas (egui draws sRGB, so P3 colours outside sRGB clip on
  screen; exports keep them); a screen-reader tree in windows other than the main one (eframe 0.36
  gives only the main window one); window positions restored at the next start.
- **Estimate:** 4–8 h.

### 19. AI features
- **Missing:** Figma AI (first draft, rename layers, remove background, image generation and
  editing, visual search, Make).
- **Estimate:** 30–60 h with openly licensed local models. **Owner decision needed.**

### 20. Mobile apps
- **Missing:** Figma's iOS and Android apps view and comment on files and mirror prototypes.
  FigCraft's web build runs in mobile browsers but is not designed for touch.
- **Estimate:** 20–40 h for a touch-first viewer and mirror.

### 21. Menu items without a command (measured): none left
- **Landed (2026-10-11):** `file.openRecent` (File ▸ Open recent: recent files kept in the
  preferences, at most 20, greyed when the file is gone and dropped when picked, Clear recent),
  `view.outlinesSelection` (outline mode for the selected layers only) and `view.maskOutlines`;
  the plugin pair landed in #101. Every item in the menu catalog has a command
  ([`parity-checklist.md`](parity-checklist.md)).
- **Estimate:** none left.

### 22. Screen readers never run; browser has no accessibility tree
- **Evidence:** AccessKit names and keyboard navigation landed (#96), checked by tests only;
  eframe's web backend doesn't send the tree yet.
- **Estimate:** 4–8 h plus human QA with VoiceOver, NVDA, Narrator and Orca.

### 23. CMYK output intent for print PDFs
- **Evidence:** CMYK conversion is a documented naive formula; no CMYK ICC profile can ship in the
  repository (licensing).
- **Estimate:** 2–4 h once an openly licensed profile is found.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | `.fig` import, best effort (gap 1): `.fig` files open (best effort): the zip, its deflate/zstd chunks and the kiwi schema are decoded generically and mapped by field name onto the REST importer, so pages, the layer tree, geometry, paints (styles resolved), strokes, effects, text with styled runs, vectors (as outlines), auto layout (grid included), constraints, components, sets, instances (expanded from their main with overrides, derived layout and property values), styles, variables with modes and bindings, prototyping and image fills from the archive come across; anything else is a warning. Gap 1 30–60 h → 8–15 h (vector networks, other layer kinds, real-file checks; export 15–30 h on top, as before) |
| 2026-10-11 | minor | Display P3 honest minimum (gap 18): the colour picker of a Display P3 file warns when a colour is outside sRGB (Outside sRGB, with what the canvas and exports do), and View ▸ Soft-proof sRGB (`view.proofSrgb`) stripes the canvas pixels sRGB can't show. Why the canvas still shows sRGB: wgpu 30 can present Display P3 or extended-range sRGB surfaces (Metal, DX12, Vulkan, WebGPU canvases), but eframe/egui-wgpu 0.36 configure an 8-bit sRGB surface with no option for the format or colour space, and egui's renderer writes sRGB-encoded UI colours; showing P3 needs those two upstream (or a patched egui-wgpu). Hardware 13–27 h → 12–26 h; gap 18 6–12 h → 5–11 h; percentages, feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~363–697 h → ~362–696 h |
| 2026-10-11 | minor | Several windows (gap 18): File ▸ New window, a tab's Open in new window or a tab dragged off the tab strip opens files in another window with its own tabs, panels and canvas, sharing the files, preferences, libraries and clipboard; each window draws only when it needs to (idle: ~0% CPU), closing one closes its files after asking about unsaved changes (the main window too, which takes over another window instead of quitting; the last window quits), and the control channel and MCP open, list, focus and capture windows (`window.new`, `window.list`, `window.focus`, `window` on any request); the screen-reader tree covers the main window only (eframe 0.36), so with a screen reader running files stay tabs; one window in the browser. Hardware 55% → 60%, 13–27 h → 10–21 h; gap 18 6–12 h → 4–8 h (P3 on screen left); feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~363–697 h → ~360–691 h |
| 2026-10-11 | minor | Localization layer landed (gap 5): per-language catalogs baked in at build time with English text as keys and fallback, placeholders and plurals, command labels by id, Preferences ▸ Language (System follows the OS or the browser) applied live and `pref.language`; English catalog and a complete Spanish draft awaiting native review. Localization 10% → 20%, 60–100 h → 45–75 h; ready for real work unchanged (~47%); full parity ~363–697 h → ~348–672 h |
| 2026-10-11 | minor | Hosting `figcraft-server`: the desktop app and `figcraft-cli` open `wss://` links (rustls with Mozilla's root certificates, always verified; `FIGCRAFT_CA_FILE` adds a private authority), releases ship the server for Linux x86_64 and aarch64, it stops gracefully on SIGTERM or Ctrl+C, and `docs/server.md` has a deployment guide (systemd unit, Caddy, nginx, Docker); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Plugin panels and settings landed (gap 6, ABI v2; v1 plugins unchanged): a plugin shows a panel of notes, fields (text, number, checkbox, menu, list, colour) and buttons drawn with FigCraft's widgets; a button or a changed value calls it back with every value (same limits per callback, one undo step per callback that edits, rolled back on failure), its module staying loaded while the panel is open; `plugin.ui` / `plugin.event` / `plugin.close` drive panels headless; per-plugin settings (64 KB) kept with the preferences; the Swatch grid example gained a palette panel that remembers the last palette. Plugins & widgets breadth 35% → 45%, ready 20% → 28%, 25–55 h → 15–40 h; ecosystem 20% → 28%; feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~373–712 h → ~363–697 h |
| 2026-10-11 | minor | Canvas fixes (gap 13 closed): boolean groups are clicked where their result is (a click in a subtracted hole reaches what's behind; with the group or an operand selected its operands are hit, so each can still be picked), a layer dragged inside a group stays in the group (frames and auto layout still take other drops), View ▸ Frame outlines and View ▸ Slices draw on the canvas, and the idle tests wait for the render thread instead of counting frames. Shapes & vector 1–3 h → 1–2 h (vector QA with real files left); percentages, feature breadth (~79%), ready for real work (~47%) and full parity hours unchanged at this rounding |
| 2026-10-11 | minor | Code connect and Dev Mode plugins (gap 16): a component or set links to its code component per language (import path, name, a template with `{property}` and `{property ? "a" : "b"}` placeholders); `inspect.code`, `inspect.componentCode` and the Inspect panel show that snippet with the instance's values (also inside generated layer trees), edited in Connect to code with a live preview; links are saved, synced and undoable, and travel as `figcraft.connect.json` (`codeConnect.export` / `codeConnect.import`). Plugins that only read run from the Inspect tab. Dev Mode breadth 80% → 90%, ready 62% → 64%, 4–8 h → 1–3 h; feature breadth (~79%) and ready for real work (~47%) unchanged; full parity ~378–722 h → ~375–717 h |
| 2026-10-11 | minor | Stretch and scatter brushes and pen pressure landed (gap 9): eight brushes drawn in code (four stretched along the path, four scattered with spacing, size, rotation and scatter jitter from a seed) in the stroke settings, drawn everywhere strokes are and flattened by outline stroke; the Pencil records pen pressure as the stroke's width profile (Windows pens, browser styluses; macOS and Linux tablets report none) and can draw with a brush; Figma Draw extras breadth 45% → 65%, ready 30% → 45%, 10–18 h → 5–9 h; Strokes 2–4 h → 1–3 h; Hardware 50% → 55%, 15–30 h → 13–27 h; feature breadth (~79%) and ready for real work (~47%) unchanged at this rounding; full parity ~378–722 h → ~373–712 h |
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
| 2026-10-10 | minor | Marked beta blockers; no alpha blockers (the alpha gate passes) |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure against Figma desktop 126.10.7 (replaces the gap list that lived in ROADMAP.md) |
