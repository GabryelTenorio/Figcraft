# FigCraft parity with Figma

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (spell check while editing text on Windows and macOS; menu breadth 201/204; the standalone server with share links to edit or view, wording only; live boolean editing and the vector tail: feature breadth ~79%; ready ~47%; last major: full re-measure against Figma desktop 126.10.7) · **Target:** Figma Design (desktop app 126.10.7, macOS)

The authoritative assessment of how close FigCraft is to Figma Design. [`ROADMAP.md`](../ROADMAP.md)
summarizes it; [`gaps.md`](gaps.md) is the ranked work list; the measured menu checklist is
[`parity-checklist.md`](parity-checklist.md). Deeper checklists:
[`ui-parity.md`](ui-parity.md), [`file-format-parity.md`](file-format-parity.md),
[`hardware-parity.md`](hardware-parity.md), [`localization-parity.md`](localization-parity.md),
[`auto-layout-parity.md`](auto-layout-parity.md), [`components-parity.md`](components-parity.md),
[`prototyping-parity.md`](prototyping-parity.md), [`dev-mode-parity.md`](dev-mode-parity.md),
[`collaboration-parity.md`](collaboration-parity.md).

## Headline

| Number | Value | Kind |
|---|---|---|
| Menu/command breadth | **98.5%** (201 / 204 catalogued menu items have a live command) | **measured** (`cargo xtask parity`, re-checked from source on 2026-10-10) |
| Feature breadth (does each Figma Design feature exist, weighted) | **~79%** (core editor areas 1–16: ~90%) | estimated from the area table below |
| Ready for real work | **~47%** | estimated (dimension table below: written weights × written per-dimension scores) |
| Essentials user | **~53%** | estimated ([below](#essentials-user): core features 77.8% × discounts 0.92 × 0.92 × 0.8) |
| Mainstream practitioner | **~43%** | estimated ([below](#mainstream-practitioner): weekly areas 66.7% × discounts 0.9 × 0.9 × 0.8) |
| Remaining to beta | **~200–365 h** one Opus 5.5 agent (~40–80 h wall-clock with 5 parallel agents) | estimated |
| Remaining to full parity | **~385–730 h** one agent (~95–165 h wall-clock with 5 parallel agents) | estimated |
| Stage | **alpha**: the alpha gate passes ([roadmap](roadmap.md#alpha-gate)); `.fig` import and multiplayer are beta gaps (see [ROADMAP](../ROADMAP.md#stage)) | |

The previous figures (ROADMAP of 2026-10-10, "feature depth ~79%", "full parity 115–195 h") were
self-graded against a 16-area rubric that left out, by design, multiplayer, plugins, cloud
libraries and AI features, and that counted no file-format blocker. Figma users rely on all of
those every day, so this re-measure weights them in. The core-editor depth figure drops from ~79%
to ~67% because nothing has yet been checked against real Figma files or side by side tool by
tool; presence was being graded as behaviour in places.

## Readiness by audience

| Audience | Ready % | Opus 5.5 agent wall-clock hours to ~95% | Work that dominates |
|---|---|---|---|
| Full target (ready for real work) | ~47% | ~370–695 h | multiplayer (45–90), localization (60–100), plugins (25–55), the interaction pass (40–60), `.fig` import (30–60), GPU canvas (30–60), AI (30–60) |
| Mainstream practitioner | ~43% | ~175–320 h | core editor depth incl. `.fig` import (88–173), the interaction pass (40–60), real-file corpus and platform QA (25–50), `.fig` export for handing work back (15–30) |
| Essentials user | ~53% | ~85–160 h | `.fig` import so files people send open (30–60), core tool polish (25–45), interaction pass on the core tools (15–25), stability QA (10–20), onboarding and starter templates (5–10) |

Hours are one agent working sequentially, calibrated from this repo's history (about 1 agent-hour
per 1,000 added lines, 1–2.5 h per feature PR, ~7 h per honest depth point in the polish tail; see
[how the hours are calibrated](#how-the-hours-are-calibrated)). Each audience's work is a subset of
the next. About 70–80% parallelizes across 4–6 agents on disjoint crates (roughly ÷4 in
wall-clock); the `.fig` reader, the multiplayer core and a plugin runtime each need one owner, and
native-speaker review, real-file licensing and screen-reader QA need people.

The full number is an additive weighted sum over the dimension table; mainstream and essentials use
multiplicative discounts, as the standard says.

## What was measured, and how

- **Target and version.** Figma desktop for macOS, `CFBundleShortVersionString` 126.10.7,
  installed at `/Applications/Figma.app`. FigCraft is measured as of `origin/main` d452da3
  (2026-10-10, PR #99).
- **The installed app, names and public listings only.** FigCraft is clean-room
  ([`AGENTS.md`](../AGENTS.md)): nothing inside the bundle's code or web assets was read. From
  `Info.plist`: one document type, `com.figma.document`, extensions `fig`, `jam`, `deck`
  (MIME `application/x-figma`), and the `figma://` URL scheme. The `.lproj` folders are
  Electron/Chromium locale packs (60+), not Figma's UI languages; the UI languages come from
  Figma's help center (see [`localization-parity.md`](localization-parity.md)). The bundle ships
  GPU, Plugin and Renderer helper processes (GPU rendering; a sandboxed plugin runtime).
- **The menu catalog.** `crates/engine/src/catalog.rs` holds Figma's main menu tree (202 items in
  File, Edit, View, Object, Text, Arrange, Vector, Plugins, Preferences, Libraries, Help), written
  from public documentation and black-box observation (local notes in the gitignored
  `plan/figma/03-menu-tree.md`). It leaves out the Widgets menu and dynamic plugin lists, so the
  98.5% overstates breadth against the full menu a little.
- **From source** (no build; 2026-10-10): 449 command ids registered with `spec!`, 963 `#[test]`
  functions in 177 files, ~108,800 lines of Rust in 13 crates + 3 apps + xtask (before #101–#103, which added the `plugin` and `collab` crates and ~7k lines), 140 key bindings
  and 13 gesture rows in `figcraft_engine::keymap`, 21 files in the SVG import corpus, 0
  translated UI strings (no localization layer exists).
- **Figma's feature set**: help center, release notes and Config 2025 announcements (Figma Draw,
  grid auto layout, slots, glass/noise/texture effects, progressive blur), known to the assessor up
  to mid-2026, plus web checks on 2026-10-10 (supported image/video types, UI languages, pen
  pressure in Figma Draw).
- **Side by side.** Look & feel was compared with Figma UI3 on the owner's Mac (screenshots kept
  local, never committed). No real Figma file has been opened in FigCraft (no `.fig` reader), and
  no rendered frame has been compared pixel for pixel against Figma's own export.

## Weights

Weights are the share of a typical product designer's working time in Figma Design, written down
so they can be argued with. Areas 1–16 keep the original rubric's weights (they sum to 94, not the 100 the rubric claimed);
areas 17–21 are added (sum 19) because Figma users depend on them. Percentages below divide by the
total, 113.
Out of scope as separate products: FigJam, Figma Slides, Sites, Make, Buzz, Weave (the `.jam` and
`.deck` document kinds belong to them).

## By feature area

Breadth: does it exist. Ready: depth, correctness and fidelity for real projects. Hours: one Opus
5.5 agent, sequential, tests included.

| # | Area | Weight | Breadth | Ready | Remaining | Hours |
|---|---|---|---|---|---|---|
| 1 | Canvas & navigation | 6 | 95% | 72% | Outline mode variants (`view.outlinesSelection`, `view.maskOutlines`), Display P3 on screen, QA at extreme zooms | 3–6 |
| 2 | Selection & transform | 8 | 95% | 68% | Handles, measuring and snapping checked tool by tool against Figma; rotated groups and mixed selections | 6–10 |
| 3 | Layers panel | 5 | 95% | 78% | Keyboard box selection, keyboard rename/context menu, long-list QA | 2–4 |
| 4 | Shapes & vector | 8 | 97% | 68% | Boolean groups hit-tested by their operands rather than their result; vector QA with real files (live boolean editing, arc fields, shape builder on strokes and open paths and one-sided widths landed) | 1–3 |
| 5 | Paint | 7 | 85% | 66% | Video and animated GIF fills, wide-gamut canvas, embedded image profiles, HEIC/TIFF images | 5–9 |
| 6 | Strokes | 3 | 90% | 72% | Stroke styles, brush strokes (see 20) | 2–4 |
| 7 | Effects | 4 | 100% | 72% | Glass and progressive blur tuned side by side | 3–6 |
| 8 | Frames, groups, sections, masks | 5 | 95% | 75% | Visual QA with real files | 2–4 |
| 9 | Auto layout & constraints | 9 | 95% | 68% | Grid `fr` rows around fixed children; behaviour against real Figma files ([auto-layout-parity](auto-layout-parity.md)) | 6–12 |
| 10 | Text | 8 | 88% | 66% | Spell check on Linux, FreeBSD and the web and in other languages (Windows and macOS use the system's checker), text on a path, CJK text QA, real-file text metrics | 5–10 |
| 11 | Components & variants | 9 | 95% | 68% | Real (not synthetic) libraries, `.fig` libraries, component playground ([components-parity](components-parity.md)) | 6–12 |
| 12 | Styles & variables | 5 | 85% | 70% | Extended collections, variables import/export, gradient stops and variant properties bound | 4–8 |
| 13 | Prototyping | 6 | 88% | 70% | Video triggers and media actions, gamepad triggers, real prototype QA ([prototyping-parity](prototyping-parity.md)) | 5–9 |
| 14 | Import/export | 6 | 65% | 35% | **`.fig` import** (the blocking gap), `.fig` export, HEIC/TIFF/video import, CMYK output intent ([file-format-parity](file-format-parity.md)) | 30–60 |
| 15 | Dev Mode / inspect | 2 | 80% | 62% | Code Connect, plugins in Dev Mode, editor integrations ([dev-mode-parity](dev-mode-parity.md)) | 4–8 |
| 16 | Comments, versions, find, misc | 3 | 80% | 58% | Comments are local only; branching; Open recent | 4–8 |
| 17 | Multiplayer & sharing | 8 | 15% | 5% | Core landed (`crates/collab`: ids, change sets, server ordering, convergence tested); live sessions (preview): WebSocket transport, hosting from the desktop app, joining from the app or the browser, invite links that open the app, cursors, selections and follow, per-person undo, offline resume; a self-hosted `figcraft-server` with persistence and share links to edit or view. Left: accounts and teams, cursor chat, TLS in the desktop app ([collaboration-parity](collaboration-parity.md)) | 45–90 |
| 18 | Plugins & widgets | 4 | 35% | 20% | Own sandboxed WASM plugins landed (#101: manifest, permissions, menu, Run last, Manage, SDK, 2 examples). Left: plugin UI at run time, settings storage, distribution, widgets; Figma's plugins are JavaScript and don't run (a JS engine would be an owner decision) ([plugins.md](plugins.md)) | 25–55 |
| 19 | Team libraries & organization | 2 | 40% | 25% | Local library files exist; no shared/cloud library, no library analytics | 10–20 |
| 20 | Figma Draw extras | 2 | 45% | 30% | Have: variable width, lasso, shape builder, noise/texture. Missing: brushes (stretch/scatter), pressure, text on a path, pattern fills, multi-edit | 10–18 |
| 21 | AI features | 3 | 0% | 0% | First draft, rename layers, image edit/remove background, visual search, Make: needs local models (owner decision) | 30–60 |
| | **Weighted (÷113)** | 113 | **~79%** | **~57%** | | **208–416** |
| | Core editor only (1–16, ÷94) | 94 | ~90% | ~67% | | 88–173 |

## By dimension (ready for real work)

| Dimension | Weight | Ready | Hours | Evidence | Doc |
|---|---|---|---|---|---|
| Features (depth) | 35 | 57% | 68–131 (areas 1–13, 15, 16, 20) | The table above; core editor ~67% | this file |
| UI/UX fidelity | 15 | 58% | 40–60 | Look & feel compared side by side with UI3 (~65% by the builders); tool-by-tool interaction (cursors, modifiers while dragging, hover states, popovers) not yet compared; accessibility names and keyboard navigation landed | [ui-parity](ui-parity.md) |
| File formats | 12 | 40% | 30–60 | No `.fig` read or write; REST JSON import, SVG in/out, PNG/JPG/WebP/PDF export solid | [file-format-parity](file-format-parity.md) |
| Collaboration | 10 | 12% | 55–110 | Local comments and version history; the multiplayer core (sync model, server ordering, client reconciliation, convergence tests) landed in #103, and live sessions (preview) co-edit a file hosted by a desktop app or kept on a self-hosted `figcraft-server` with edit and view links, joined from the app or the browser (`docs/collab-protocol.md`) | [collaboration-parity](collaboration-parity.md) |
| Stability | 8 | 55% | 20–40 | Never-crash lints, command guard with rollback, recovery copies, 963 tests incl. junk-parameter sweeps and a seeded retained-render walk; but two days old, one release (v0.2.0), no field reports, QA mostly on macOS | [gaps](gaps.md) |
| Performance | 6 | 60% | 30–60 | `cargo xtask bench`: 50k-layer page ~8 ms zoomed out, 10k-layer edits ~0.2 ms, 2,000-instance design system edits 4–64 ms; CPU (tiny-skia) canvas where Figma renders on the GPU; nothing timed against Figma on the same file | [hardware-parity](hardware-parity.md) |
| Hardware | 3 | 50% | 15–30 | Trackpad pinch and HiDPI yes; no pen pressure, no GPU canvas, no P3 on screen, one window | [hardware-parity](hardware-parity.md) |
| Localization | 3 | 10% | 60–100 | English only; Figma ships 7 more UI languages | [localization-parity](localization-parity.md) |
| Platforms | 3 | 75% | 10–20 | macOS, Windows (x64/x86/arm64), Linux, FreeBSD, web all packaged (v0.2.0); Figma also has iOS/Android apps; non-macOS QA thin | [hardware-parity](hardware-parity.md) |
| Ecosystem/plugins | 3 | 20% | 25–55 | Own WASM plugins with a Rust SDK and two examples (#101); no Figma plugin compatibility, no widgets, no distribution or community files; the CLI, control channel and MCP server are ahead of Figma for agents | [gaps](gaps.md) |
| AI features | 2 | 0% | 30–60 | None | [gaps](gaps.md) |
| **Ready for real work** | 100 | **~47%** | **~385–730** | | |

Dimension hours add up to the full-parity total without double counting: the feature-area rows
for import/export (14), multiplayer and team libraries (17, 19), plugins (18) and AI (21) are
counted under their dimensions.

## Essentials user

Someone who uses only the core of Figma: a casual or beginner user making a few screens, a social
graphic or a simple mockup. Same method, with the core features a casual user touches.

**1. Core features, weights and depth** (depth from the area table and checklists above):

| Feature | Weight | Depth | Notes |
|---|---|---|---|
| Create, open, save and reopen a file; pages | 10 | 85% | home screen, `.figcraft` round trips; Open recent missing |
| Pan and zoom | 10 | 80% | wheel, pinch, Space-drag, zoom shortcuts |
| Select, move, resize, rotate, align, group, duplicate | 15 | 75% | not yet compared tool by tool |
| Frames with device presets | 8 | 80% | |
| Rectangle, ellipse, line, arrow, corner radius | 8 | 80% | |
| Text: type, font, size, weight, colour, alignment | 12 | 70% | spell check on Windows and macOS only |
| Colour picker, fills and strokes; place and crop images | 10 | 72% | no HEIC images |
| Layers panel: rename, hide, lock, reorder | 7 | 80% | |
| Basic auto layout (add, gap, padding) | 6 | 70% | |
| Undo / redo | 5 | 90% | unlimited, one step per gesture |
| Export PNG, JPG, SVG, PDF | 6 | 85% | |
| Copy and paste, incl. images from other apps | 3 | 75% | |
| **Weighted** | 100 | **77.8%** | |

Left out: components and variables beyond using one, prototyping, Dev Mode, advanced vector and
type, and everything the mainstream number leaves out.

**2. Discounts a casual user actually hits:**

| Discount | Factor | Evidence |
|---|---|---|
| Launch and stability | ×0.92 | signed and notarized macOS build, installers for every platform (v0.2.0), never-crash rules and recovery copies; no field record yet |
| Discoverability and UI clarity | ×0.92 | the UI3 layout Figma users know, Actions palette, keyboard shortcuts panel; no onboarding, templates or Community files to start from |
| Opening files people send them | ×0.80 | what people send a Figma user is a Figma link or a `.fig`; neither opens. Images and SVG do |

77.8% × 0.92 × 0.92 × 0.80 = **~53%** (estimated). User evidence: none yet (see above).

## Mainstream practitioner

The same question for the typical professional Figma Design user: a product designer who designs
screens, maintains or uses a design system, prototypes and hands off to developers every week.
Computed per craftrules `standards/progress-docs.md`.

**1. Areas used weekly, with weights.** Feature areas 1–16 of the table above, with their weights
(sum 94). Left out as the standard says: plugins and widgets (18), AI (21), team and organization
libraries (19, cloud/team admin), Figma Draw extras (20, not weekly for a product designer),
localization beyond English, specialist hardware. Real-time multiplayer (17) is cloud collaboration
and is left out as an area, but its effect on working with Figma users is counted in discount 3.

**2. Average depth** ("Ready" column, weighted): **66.7%**.

**3. Discounts for what still stops real work:**

| Discount | Factor | Evidence |
|---|---|---|
| Interaction fidelity | ×0.90 | UI3 look compared side by side (~65% by the builders); no tool-by-tool comparison of cursors, modifier keys while dragging, snapping thresholds or popovers yet; Ctrl to suspend snapping on macOS missing ([ui-parity](ui-parity.md)) |
| Stability on real machines | ×0.90 | Never-crash lints, command guard with rollback, recovery copies and 963 tests, but no field record at all: the repository is private, v0.2.0 has had no outside users or issue reports, CI tests run on Linux only and hands-on QA is on macOS. Harsher than VectorCraft's ×0.92 because there is no outside evidence either way |
| File exchange with Figma users | ×0.80 | A Figma designer's work lives in shared Figma files. FigCraft can't open `.fig` or write anything Figma opens as a document; incoming work can come through the REST API JSON (one way, needs an access token), outgoing only as SVG, PDF, PNG or code. Teammates, reviewers and developers would all need FigCraft ([file-format-parity](file-format-parity.md), [collaboration-parity](collaboration-parity.md)) |

66.7% × 0.90 × 0.90 × 0.80 = **~43%** (estimated).

**4. User evidence (2026-10-10):** 0 issues, 0 praise or "switched from Figma" reports, 0 comments
from anyone but the maintainers (`gh issue list`, `gh api .../issues/comments`; the repository is
private, 1 star, PRs only from maintainers). No outside evidence either way; nothing raises the
number.

**Why it is lower than ready for real work (~47%), unusually.** The full number averages
dimensions additively, so the missing `.fig` exchange costs only the file-format dimension's 12%
weight, and FigCraft earns points the mainstream number doesn't count (platforms 75%: Linux,
FreeBSD and the web, where Figma has no desktop app). The mainstream number applies file exchange
as a multiplier on everything, because a Figma practitioner's whole working week is shared through
Figma files. Both are computed from written weights; neither was moved to match the other. The
mainstream number rises fastest with `.fig` import (the ×0.80 toward ×0.95: ~51%) and the
interaction pass.

## How the hours are calibrated

From this repository's own history (`git log`, `gh pr list`):

- The whole app was built in ~25 wall-clock hours: first commit 2026-10-09 15:38 UTC, PR #99
  merged 2026-10-10 16:53 UTC; ~49k lines on day 1 before PRs, then 99 PRs adding ~81k lines. The
  ROADMAP of that period records 4–6 agents in parallel, so ~110–150 agent-hours in all: about
  **1 agent-hour per ~1,000 added lines**, and a typical feature PR (1,000–2,500 lines, e.g.
  #84 arc handles, #92 PDF print options, #96 accessibility) took **1–2.5 agent-hours**.
- Between the ROADMAP revisions of 2026-10-09 22:19 UTC and 2026-10-10 16:53 UTC (~18.5 h wall,
  78 PRs, ~75–110 agent-hours) the self-graded depth went 58% → 79%: **~4–5 agent-hours per
  self-graded point**. This re-measure grades about 0.6 points for each of those, which puts an
  honest point nearer 7 agent-hours in the polish tail; new subsystems (a `.fig` reader,
  multiplayer, a plugin runtime) are estimated bottom-up instead, since nothing comparable has been
  built here yet.
- Parallelism: most of the work splits across 4–6 agents on disjoint crates (the history shows it
  working); multiplayer, `.fig` import and the plugin runtime each need one owner for their core.
  Needs a human: native-speaker review for every language, the plugin-runtime and AI-model
  decisions, real-world Figma files to test with (licensing), Windows/Linux/screen-reader QA.

## Area detail: what has landed and what is left

Carried over from the ROADMAP's "Where we're lacking" list (2026-10-10, before this re-measure).
Hours in this narrative are the builders' estimates; the tables above supersede them.

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
   flipped ellipses, any zoom), Design panel Arc fields (Start, Sweep, Ratio; bindable), live
   boolean editing (operands selected by double-click or Cmd/Ctrl-click; moving, resizing or
   editing an operand's points redraws the result during the drag with every operand outlined;
   per-operand operations from the boolean controls; only the operands from the edited one up are
   recombined), the shape builder on strokes and open paths (strokes as their outlined areas), and
   one-sided variable widths (Alt-drag a width handle; canvas, exports and outline stroke).
   *Left:* boolean groups are hit-tested by their operands rather than their result. 1–3 h.
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
   (Alt+/, Shift+/), outlines on Shift+Cmd+O, pixel grid on Cmd+', Shift+E / Shift+D and the other panel keys, Ctrl+Tab between files, B for the paint bucket in vector edit mode; the Keyboard
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
   test over real commands converges. Live sessions run over the network (hosting from the app,
   invite links, document sync, live cursors, selections, avatars, follow). Undo is per person in
   a session (`edit.undo` takes back only your own steps, leaving what others changed since;
   creations and deletions invert; a gesture or command group stays one step); refused edits roll
   back with a toast; a dropped connection keeps editing and resumes as the same client with what
   it missed (or a snapshot), then sends what was made offline. Randomized sessions with undo,
   redo and dropped connections converge, and a checker confirms no undo reverts someone else's
   change.
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
   distribution; for collaboration, the browser client and a standalone server. 18–45 h.
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

## Inventory: what exists today

Carried over from the ROADMAP's "Shipped so far" (2026-10-10). The test count there was stale: 963 `#[test]` functions as of this re-measure.

- **Architecture:** 15 crates with enforced layering (`cargo xtask layers`): geom, color, doc,
  layout, text, pathops, plugin, collab, render, format, svg, engine, ui-egui (+ mcp, pdf). Every action
  is a command (449 command ids); undo is unlimited via structural sharing (`Arc` node snapshots); gestures are
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
- **Tests:** 963 `#[test]` functions (2026-10-10) including junk-parameter sweeps over every command (and every vector-edit command while editing), importer fuzzing and render regressions.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Spell check while editing text: misspelled words underlined, suggestions, Ignore and Add to dictionary in the context menu, Text ▸ Spelling ▸ Check spelling (the system's checker on Windows and macOS, in the system's language; none on Linux, FreeBSD or the web yet). Text 85% → 88% breadth, 64% → 66% ready, 8–14 h → 5–10 h; menu breadth 200/203 → 201/204 (98.5%); gap 11 narrowed to the remaining platforms and languages (4–8 h → 2–5 h); weighted percentages unchanged, hour totals down 3–4 h (headline ranges unchanged within their rounding) |
| 2026-10-11 | minor | Collaboration wording after the standalone server: `figcraft-server` keeps many files, persisted as a snapshot plus a log of what changed (compacted, checksummed, damaged files skipped and reported), restarts lose nothing confirmed and apps resume where they were; the app publishes its file to it; share links per file let people edit or only view (the server refuses a viewer's edits), are revocable and stored as hashes; TLS comes from a reverse proxy (the desktop app has none of its own yet); percentages and hours unchanged until the next re-measure |
| 2026-10-11 | minor | Live boolean editing, Design panel arc fields, the shape builder on strokes and open paths and one-sided variable widths landed (gap 13): Shapes & vector breadth 92% → 97%, ready 66% → 68%, 8–14 h → 1–3 h; feature breadth ~78% → ~79%; ready for real work unchanged (~47%); full parity ~390–745 h → ~385–730 h |
| 2026-10-11 | minor | Wording only: live sessions (#104), then the browser joining them and `figcraft://` invite links opening the desktop app; percentages and hours unchanged until the next re-measure |
| 2026-10-10 | minor | Merged main after #101–#103: WASM plugins (plugins area 0% → 35% breadth, 20% ready; ecosystem 5% → 20%) and the multiplayer core without networking (multiplayer 5% → 15% breadth, 5% ready; collaboration 8% → 12%); menu breadth 97.5% → 98.5% (200/203); ready for real work ~46% → ~47%; full parity 420–790 h → 390–745 h; beta 210–380 h → 200–365 h. Fixed the stated weight sums (areas 1–16 sum to 94, total 113; percentages were already computed with those) |
| 2026-10-10 | minor | Readiness-by-audience table: hours to ~95% for full (~400–750 h), mainstream (~180–330 h) and essentials (~85–160 h). Full number confirmed as an additive weighted sum; unchanged |
| 2026-10-10 | minor | Added the essentials user number (~53%: core features 77.8% × 0.92 launch/stability × 0.92 discoverability × 0.8 opening files people send) |
| 2026-10-10 | minor | Added the mainstream practitioner number (~43%: weekly areas 66.5% × 0.9 interaction × 0.9 stability × 0.8 file exchange) and user-evidence count (none). Checked that ready for real work (~46%) is built from the written dimension weights and scores; unchanged |
| 2026-10-10 | minor | Stage checked against the alpha gate: stays alpha |
| 2026-10-10 | major | Created from the ROADMAP's parity rubric, honest assessment and area narrative. Full re-measure against Figma desktop 126.10.7: added multiplayer, plugins, team libraries, Figma Draw extras and AI as weighted areas; split breadth from ready-for-real-work; core depth re-graded ~79% → ~67%; hours calibrated from git history; full parity 115–195 h → 420–790 h |
