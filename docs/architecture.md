# FigCraft architecture

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created from the code as of PR #99; replaces the local-only plan notes) · **Target:** Figma Design

How FigCraft is built today. Engine first, a thin swappable UI, everything a command; a Figma-shaped
data model (a scene graph of frames with auto layout, components and variables). ~108,800 lines of
Rust (2026-10-10).

## Crates and layers

`cargo xtask layers` enforces the layering (`xtask/src/layers.rs`): a crate may depend only on
lower layers. Nothing below L6 depends on egui, eframe, rfd or winit, so the UI crate can be
replaced.

| Layer | Crate | Lines | Role |
|---|---|---|---|
| L0 | `figcraft-geom` | ~0.5k | Affine transforms, shape path builders (rounded and smoothed corners, polygons, stars, arcs) on kurbo |
| L0 | `figcraft-color` | ~1.3k | Colours (HSB/HSL, hex/CSS, Display P3), paints (solid, gradients, image), blend modes |
| L1 | `figcraft-doc` | ~8.5k | Document model: pages, node tree in a persistent map (`imbl`, stable ids), properties, components, styles, variables, prototype data, comments, versions. Serde everywhere |
| L2 | `figcraft-layout` | ~1.8k | Auto layout (horizontal, vertical, wrap, grid; hug/fill/fixed; min/max), constraints, layout grid geometry |
| L2 | `figcraft-text` | ~3.1k | Font database (optional embedded craft-fonts + system scan), shaping (harfrust), bidi, line layout, glyph outlines (skrifa), variable fonts |
| L2 | `figcraft-pathops` | ~1.1k | Booleans (linesweeper), outline stroke with alignment, arrowheads |
| L3 | `figcraft-render` | ~5.4k | Scene → pixels on tiny-skia (CPU): isolated layers for opacity/blend/effects, clips, masks, shadows, blurs (uniform, progressive, background), noise, texture, glass, text; banded in parallel; retained rendering of only what changed; levels of detail when zoomed out |
| L3 | `figcraft-format` | ~5.2k | Native `.figcraft` (zip of `document.json` + images; streamed JSON), local library files, versions, REST API file JSON import (`rest/`) |
| L4 | `figcraft-svg` | ~3.7k | SVG export and import (usvg) |
| L4 | `figcraft-pdf` | ~2.3k | PDF export (krilla) with print options |
| L5 | `figcraft-engine` | ~39.3k | Sessions, undo history (`Arc` snapshots), the command registry (`cmd/*`, `spec!`), pointer tools and vector edit mode, snapping and measuring, components sync, libraries, prototype runtime, Dev Mode code generation, the menu catalog (`catalog.rs`) and the keymap (`keymap.rs`), a panic guard |
| L6 | `figcraft-ui-egui` | ~31.9k | The UI3-style frontend: canvas (render worker), toolbar, layers, Design/Prototype/Inspect panels, colour picker, menus, dialogs, presenter, comments, accessibility (AccessKit), the control channel server |
| L6 | `figcraft-mcp` | ~2.4k | MCP server (JSON-RPC over stdio), headless or bridged to a running app |
| apps | `figcraft`, `figcraft-cli`, `figcraft-web` | ~1.9k | Desktop app (eframe + wgpu), command line, browser build (WASM, WebGPU/WebGL2) |
| tools | `xtask` | | `ci`, `layers`, `assets`, `parity`, `version`, `icon`, `wasm`, `bench` |

## Data model

A `Document` holds pages, a persistent map of nodes (frames, groups, sections, rectangles,
ellipses, polygons, stars, lines, vector networks, text with styled runs, boolean groups,
components, component sets, instances, slices), styles, variable collections, comments, versions
and images by hash. Instances keep their main's id and per-layer overrides; the engine keeps their
materialised children in sync when a main changes (directly for value, size and add/remove edits,
by a rebuild otherwise). Snapshots are cheap (`Arc` and structural sharing), which gives unlimited
undo with one step per gesture.

## Commands: one path for every surface

Every user-visible behaviour is a command in `crates/engine/src/cmd/*` (449 ids on 2026-10-10)
with an id, label, parameter docs and undo flag. The menus (generated from the catalog), keyboard
shortcuts, Actions palette, CLI (`figcraft-cli run`), JSON control channel (`--control <port>`) and
MCP server all dispatch the same ids, so agents can do anything a person can. Commands never open
dialogs themselves; they request UI, and programmatic callers drop those requests.
`cargo xtask parity` measures the catalog's coverage ([`parity-checklist.md`](parity-checklist.md)).

## Rendering

The CPU renderer is the single source of pixels for the canvas, exports and the browser. The
desktop canvas renders on a worker thread at the device pixel ratio and keeps its last image,
re-rendering only the region a pan uncovers or an edit touches; overlays (selection, handles,
guides, labels) are drawn by egui on top. A seeded random walk checks the retained canvas against
full renders. SVG and PDF export walk the same scene and embed rasters where those formats can't
express an effect.

## File I/O

`.figcraft` is the native format (atomic saves, autosave checkpoints, recovery copies after a
crash). Imports: REST API file JSON with variables and images, SVG, raster images, clipboard
content. Exports: PNG, JPG, WebP, SVG, PDF, code. There is no `.fig` reader yet
([`file-format-parity.md`](file-format-parity.md)).

## Robustness

Workspace lints deny `unwrap`, `expect`, `panic!` and friends in shipped code; `unsafe_code` is
forbidden. `figcraft_engine::guard` turns a panicking command into an error and rolls the document
back; the desktop app also guards each UI frame. Junk-parameter sweeps run every command with bad
input.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created from the code as of PR #99 |
