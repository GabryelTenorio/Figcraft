# Components, variants, styles and variables parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (extended collections, variables import/export, gradient stops and variant properties bound to variables) · **Target:** Figma Design (desktop app 126.10.7)

Design systems are why teams adopt Figma. **Components: breadth ~95%, ready ~68%. Styles and
variables: breadth ~92%, ready ~72% (estimated).** QA'd with a synthetic design system of ~300
components used 2,000 times (`crates/engine/src/synthetic_ds.rs`, `components_scale_tests.rs`);
not with real libraries. Remaining: 7–14 h here, plus shared libraries (10–20 h after
multiplayer).

## Components and variants

| Feature | Status | Notes |
|---|---|---|
| Create component, instances, detach, reset, go to main, restore main, push changes | done | |
| Per-layer overrides with reset; kept across main edits, nested swaps, copy/paste | done | main edits match a full rebuild exactly at 2,000 instances |
| Component sets / variants, add variant, switch | done | |
| Properties: boolean, text, instance swap (preferred values), variant | done | |
| Slots (preferred components, fill menu, kept across updates) | done | Figma 2025 feature |
| Exposed nested instances | done | |
| Descriptions | done | |
| Instance swap: panel, Alt-drop from Assets | done | |
| Assets panel: thumbnails, search, drag into frames and auto layout slots | done | virtualized |
| Instances pasted into other files follow their mains | done | missing main restorable |
| Component playground (try properties without placing) | missing | 1–2 h |
| Real-world libraries QA | **not checked** | [gaps](gaps.md) #3 |

## Libraries

| Feature | Status | Notes |
|---|---|---|
| Publish a file's components, sets, styles and variables (per-asset versions) | done | as local library files |
| Enable a library in a file; read-only linked copies | done | |
| Library updates available; overrides kept | done | |
| Shared team/organization libraries, library analytics, swap library | missing | [gaps](gaps.md) #10 |

## Styles and variables

| Feature | Status | Notes |
|---|---|---|
| Paint, text, effect, grid styles; style picker; text styles on character ranges | done | |
| Variables: colour, number, string, boolean; collections, modes, aliases, scopes, groups | done | |
| Bindings: fills, strokes, effects, opacity, padding, gaps, sizes, radius, font size, line height, letter spacing, text, visibility, layout grids, instance properties | done | |
| Explicit modes per frame, library collections' modes | done | |
| Gradient stops and variant properties bound to variables | done | a stop's binding follows it when earlier stops are removed |
| Extended collections | done | library collections; overrides per mode with reset; follow library updates; modes inherited. New library variables arrive with the next other update ([gaps](gaps.md) #14) |
| Variables import/export (JSON) | done | the REST local variables shape (round-trips; merges by collection, mode and variable name); CSS custom properties export |
| Expressions in prototype set-variable actions | done | see [prototyping-parity](prototyping-parity.md) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Extended collections, variables import/export (JSON, CSS) and gradient stop and variant property bindings landed: styles and variables breadth ~85% → ~92%, ready ~70% → ~72%; remaining here 10–20 h → 7–14 h |
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
