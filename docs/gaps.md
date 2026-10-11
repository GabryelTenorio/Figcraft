# Where FigCraft falls short of Figma

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (spell check while editing text on Windows and macOS; menu breadth 201/204; the standalone server with share links to edit or view, wording only; live boolean editing and the vector tail: feature breadth ~79%) · **Target:** Figma Design (desktop app 126.10.7)

Every known shortfall against Figma, one entry each, ranked by how much it stops a Figma user from
doing real work in FigCraft. Agents: pick work from the top unless the owner says otherwise. The
numbers behind the ranking are in [`target-app-parity.md`](target-app-parity.md). Hours are one
Opus 5.5 agent working sequentially, tests included, calibrated as described there. "Estimated"
everywhere unless marked measured.

## Ranked

No alpha blockers: the alpha gate in [`roadmap.md`](roadmap.md#alpha-gate) passes. Beta blockers are marked.

| # | Gap | Kind | Hours | Doc |
|---|---|---|---|---|
| 1 | `.fig` files can't be opened (**beta blocker**) | File format | 30–60 | [file-format-parity](file-format-parity.md) |
| 2 | Multiplayer and sharing are a preview, without accounts (**beta blocker**) | Feature | 45–90 | [collaboration-parity](collaboration-parity.md) |
| 3 | Nothing checked against real Figma files | Fidelity | 15–30 | [target-app-parity](target-app-parity.md) |
| 4 | Interaction fidelity not compared tool by tool | UI/UX | 40–60 | [ui-parity](ui-parity.md) |
| 5 | No localization layer; English only | Localization | 60–100 | [localization-parity](localization-parity.md) |
| 6 | Plugins: no run-time UI, distribution or widgets; Figma's plugins don't run | Ecosystem | 25–55 | [target-app-parity](target-app-parity.md) |
| 7 | CPU canvas where Figma uses the GPU | Hardware/performance | 30–60 | [hardware-parity](hardware-parity.md) |
| 8 | Platform QA is mostly macOS | Stability/platforms | 10–20 | [hardware-parity](hardware-parity.md) |
| 9 | Figma Draw: brushes, pressure, text on a path, pattern fills | Feature | 10–18 | [target-app-parity](target-app-parity.md) |
| 10 | Shared (team) libraries | Feature | 10–20 | [components-parity](components-parity.md) |
| 11 | Spell check on Linux, FreeBSD and the web; one language | Feature | 2–5 | [target-app-parity](target-app-parity.md) |
| 12 | Video and animated GIF fills; HEIC/TIFF images | File format | 6–12 | [file-format-parity](file-format-parity.md) |
| 13 | Boolean groups hit-tested by their operands (the rest of the vector tail landed) | Feature | 1–3 | [target-app-parity](target-app-parity.md) |
| 14 | Variables: extended collections, import/export | Feature | 4–8 | [components-parity](components-parity.md) |
| 15 | Prototyping: media triggers and actions, gamepad | Feature | 3–6 | [prototyping-parity](prototyping-parity.md) |
| 16 | Dev Mode: Code Connect, editor integrations | Feature | 4–8 | [dev-mode-parity](dev-mode-parity.md) |
| 17 | Branching and merging | Feature | 8–15 | [collaboration-parity](collaboration-parity.md) |
| 18 | Display P3 on screen; one window only | Hardware | 6–12 | [hardware-parity](hardware-parity.md) |
| 19 | AI features | AI | 30–60 | [target-app-parity](target-app-parity.md) |
| 20 | Mobile apps (view, mirror, comment) | Platforms | 20–40 | [hardware-parity](hardware-parity.md) |
| 21 | Three menu items without a command | Feature | 2–4 | [parity-checklist](parity-checklist.md) |
| 22 | Screen readers never run; browser has no accessibility tree | UI/UX | 4–8 | [ui-parity](ui-parity.md) |
| 23 | CMYK output intent for print PDFs | File format | 2–4 | [file-format-parity](file-format-parity.md) |

## Entries

### 1. `.fig` files can't be opened
- **Missing:** a reader for Figma's native document (`com.figma.document`, extensions `fig`, `jam`,
  `deck` in the installed app's `Info.plist`). Figma's files live in the cloud; "Save local copy"
  writes a `.fig`. Today the only way in is the REST API file JSON (`file.open` / `file.import`),
  which needs an access token and a separate download of images and variables.
- **Evidence:** no `.fig` code anywhere in `crates/format` (searched 2026-10-10); ROADMAP listed
  ".fig import (best effort)" as left.
- **Impact:** blocks beta by definition: a designer can't bring existing work across. The single
  biggest gap.
- **Estimate:** 30–60 h for a best-effort reader written from public format knowledge (a
  compressed binary schema plus a message per node; mapping onto the existing REST importer's
  model), with a corpus of synthetic `.fig` files saved from the installed Figma. Export to `.fig`
  is a further 15–30 h and lower priority.

### 2. Multiplayer and sharing are a preview, without accounts
- **Missing:** accounts, teams and projects, per-person permissions, spotlight, cursor chat, audio,
  shared comments and version history, TLS in the desktop app. Comments and versions are local to
  the file.
- **Evidence:** the core landed in #103 (`crates/collab`: client-scoped ids, change sets, server
  ordering and validation, client reconciliation, binary messages, randomized convergence tests over
  real commands), and live sessions run over the network (`crates/collab-net`: hosting from the
  app, invite links that open the app, document sync between apps, live cursors, selections,
  text carets, avatars and follow, and the browser build joins) with per-person undo, refused
  edits rolled back with a reason, reconnects that resume and send what was made offline, and
  styles, variables and comments that several people create at once all kept; a standalone
  `figcraft-server` keeps many files (snapshot plus log, compacted; restarts lose nothing
  confirmed), takes files published from the app and hands out share links that let people edit
  or only view (`docs/server.md`). But there are no accounts (a link is the access), and the
  desktop app speaks only plain `ws://` (TLS comes from a reverse proxy, which only the browser
  can use so far) (`docs/collab-protocol.md`).
- **Impact:** Figma's defining feature; most teams use it every day. Solo designers can work
  without it.
- **Estimate:** 45–90 h: accounts, teams and per-person permissions, TLS in the desktop app,
  shared comments and versions (numbers from before the server landed; re-measured next).

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

### 5. No localization layer; English only
- **Missing:** externalized strings and catalogs. Figma ships English, Japanese, Korean, French,
  German, Spanish (Spain and Latin America) and Portuguese (Brazil).
- **Evidence (measured from source):** ~800 distinct capitalised UI string literals in
  `crates/ui-egui/src` and 435 command labels, all inline; no catalog files.
- **Impact:** keeps out non-English designers, including Figma's large Japanese and Korean user
  bases.
- **Estimate:** 15–25 h for the layer and string extraction, then 3–5 h per language plus native
  review; 60–100 h for the 11 key languages. Detail in [`localization-parity.md`](localization-parity.md).

### 6. Plugins: no run-time UI, distribution or widgets; Figma's plugins don't run
- **Landed (#101):** sandboxed WebAssembly plugins (wasmi, metered, memory cap, no WASI, also in
  the browser) acting through the command registry, one undo step per run, manifest permissions,
  Plugins menu, Run last plugin, Manage plugins, `plugin.*` commands, a Rust SDK and two examples
  ([`plugins.md`](plugins.md)).
- **Missing:** plugin-defined UI at run time (forms or panels a plugin returns), settings storage,
  distribution (a plugin directory), widgets, Community files. Figma's own plugins are JavaScript
  against Figma's plugin API and don't run.
- **Impact:** teams that depend on specific Figma plugins (icons, content, accessibility checks)
  can't bring them. FigCraft's CLI, control channel and MCP server cover agent automation better
  than Figma does.
- **Estimate:** 25–55 h for the missing pieces of FigCraft's own API. Running Figma's plugins
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

### 8. Platform QA is mostly macOS
- **Missing:** regular runs on Windows, Linux and the browser; real screen readers.
- **Evidence:** CI tests on Linux only (`.github/workflows/ci.yml`); release builds exist for every
  platform (v0.2.0) but no issue has been filed yet and the builders checked on macOS.
- **Impact:** unknown bugs on the platforms where FigCraft is most attractive (Linux has no Figma
  desktop app).
- **Estimate:** 10–20 h, partly human (hardware).

### 9. Figma Draw: brushes, pressure, text on a path, pattern fills
- **Missing:** stretch and scatter brushes, pen pressure for the pencil and brushes, text on a
  path, pattern fills, multi-edit.
- **Evidence:** `PaintKind` has solid, gradient and image only (`crates/color/src/lib.rs`); no
  pressure input anywhere; SVG text on a path imports as outlines.
- **Impact:** illustration-flavoured work moved into Figma Draw in 2025.
- **Estimate:** 10–18 h.

### 10. Shared (team) libraries
- **Missing:** libraries shared across people and files without copying them around; library
  analytics; swapping a whole library.
- **Evidence:** local library files work (publish, enable, updates with overrides kept), see
  [`components-parity.md`](components-parity.md).
- **Impact:** design-system teams. Builds on multiplayer's server.
- **Estimate:** 10–20 h after gap 2.

### 11. Spell check on Linux, FreeBSD and the web; one language
- **Have:** misspelled words in the text being edited get a wavy underline; right-click one for
  suggestions, Ignore (until the app quits) and Add to dictionary (saved in preferences); Text ▸
  Spelling ▸ Check spelling, on by default (`crates/engine/src/spelling.rs`). The desktop app uses
  the system's checker on Windows and macOS, in the system's language.
- **Missing:** a checker on Linux and FreeBSD (hunspell or enchant) and in the browser; choosing the
  language (Text ▸ Spelling ▸ Language); text in scripts written without spaces (CJK, Thai) isn't
  checked; Add to dictionary doesn't reach the system's dictionary.
- **Estimate:** 2–5 h.

### 12. Video and animated GIF fills; HEIC/TIFF images
- **Missing:** Figma takes JPG, PNG, HEIC, WebP, GIF and TIFF images and MP4, MOV and WebM video
  (help center, checked 2026-10-10). FigCraft decodes PNG, JPEG, WebP, GIF (first frame) and BMP.
- **Estimate:** 6–12 h (video playback in the presenter is most of it).

### 13. Boolean groups hit-tested by their operands
- **Landed (2026-10-11):** live boolean editing (operands selected by double-click or Cmd/Ctrl-click;
  moving, resizing or editing an operand's points redraws the result during the drag, with every
  operand outlined; per-operand operations from the boolean controls and `node.set`; a drag frame
  recombines only the operands from the edited one up), Design panel Arc fields (Start, Sweep,
  Ratio; Mixed across ellipses; bindable), the shape builder on strokes and open paths (strokes take
  part as their outlined areas), one-sided variable widths (Alt-drag a width handle; canvas, PNG,
  SVG, PDF and outline stroke).
- **Missing:** a click is tested against a boolean group's operands, not its result, so a click in
  a subtracted hole still picks the group.
- **Estimate:** 1–3 h.

### 14. Variables: extended collections, import/export
- **Missing:** extended collections (a library collection extended with overrides), variables
  import/export, variant properties and gradient stops bound to variables.
- **Estimate:** 4–8 h.

### 15. Prototyping: media triggers and actions, gamepad
- **Missing:** video triggers (on media hit/end) and play/pause/mute/seek actions; gamepad input.
  The REST importer warns and skips them (`crates/format/src/rest/proto.rs`).
- **Estimate:** 3–6 h after video fills (gap 12).

### 16. Dev Mode: Code Connect, editor integrations
- **Missing:** mapping components to real code (Code Connect), plugins in Dev Mode, VS Code
  extension. FigCraft's MCP server already serves design data to agents.
- **Estimate:** 4–8 h.

### 17. Branching and merging
- **Missing:** branches of a file, review and merge.
- **Estimate:** 8–15 h (versions and Compare changes are a base).

### 18. Display P3 on screen; one window only
- **Missing:** wide-gamut colour on the canvas (egui draws sRGB, so P3 colours outside sRGB clip on
  screen; exports keep them); several windows (Figma's desktop app opens files in separate
  windows).
- **Estimate:** 6–12 h.

### 19. AI features
- **Missing:** Figma AI (first draft, rename layers, remove background, image generation and
  editing, visual search, Make).
- **Estimate:** 30–60 h with openly licensed local models. **Owner decision needed.**

### 20. Mobile apps
- **Missing:** Figma's iOS and Android apps view and comment on files and mirror prototypes.
  FigCraft's web build runs in mobile browsers but is not designed for touch.
- **Estimate:** 20–40 h for a touch-first viewer and mirror.

### 21. Three menu items without a command (measured)
- `file.openRecent`, `view.outlinesSelection`, `view.maskOutlines`
  ([`parity-checklist.md`](parity-checklist.md)); the plugin pair landed in #101.
- **Estimate:** 2–4 h for the first three.

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
| 2026-10-11 | minor | Spell check while editing text: misspelled words underlined, suggestions, Ignore and Add to dictionary in the context menu, Text ▸ Spelling ▸ Check spelling (the system's checker on Windows and macOS, in the system's language; none on Linux, FreeBSD or the web yet). Text 85% → 88% breadth, 64% → 66% ready, 8–14 h → 5–10 h; menu breadth 200/203 → 201/204 (98.5%); gap 11 narrowed to the remaining platforms and languages (4–8 h → 2–5 h); weighted percentages unchanged, hour totals down 3–4 h (headline ranges unchanged within their rounding) |
| 2026-10-11 | minor | Collaboration wording after the standalone server: `figcraft-server` keeps many files, persisted as a snapshot plus a log of what changed (compacted, checksummed, damaged files skipped and reported), restarts lose nothing confirmed and apps resume where they were; the app publishes its file to it; share links per file let people edit or only view (the server refuses a viewer's edits), are revocable and stored as hashes; TLS comes from a reverse proxy (the desktop app has none of its own yet); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Live boolean editing, Design panel arc fields, the shape builder on strokes and open paths and one-sided variable widths landed (gap 13): Shapes & vector breadth 92% → 97%, ready 66% → 68%, 8–14 h → 1–3 h; feature breadth ~78% → ~79%; ready for real work unchanged (~47%); full parity ~390–745 h → ~385–730 h |
| 2026-10-11 | minor | Collaboration wording after the collaboration polish: styles, variables, collections, modes and comments created in a live session take ids from their creator's id space (concurrent creations no longer overwrite each other; the server checks it), others' text carets and selections show on the canvas, and typing in a layer someone else is typing in says so; percentages unchanged |
| 2026-10-11 | minor | Wording only: live sessions (#104), then the browser joining them and `figcraft://` invite links opening the desktop app; percentages and hours unchanged until the next re-measure |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | minor | Marked beta blockers; no alpha blockers (the alpha gate passes) |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure against Figma desktop 126.10.7 (replaces the gap list that lived in ROADMAP.md) |
