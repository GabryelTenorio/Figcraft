# Auto layout and constraints parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-10 · **Change:** major (created from the ROADMAP's auto layout items and `crates/layout`) · **Target:** Figma Design (desktop app 126.10.7)

Auto layout is the most used layout feature in Figma and the one where small differences (a
hugging frame one pixel off, text wrapping at another width) break real files. **Breadth ~95%,
ready ~68% (estimated)**: everything Figma documents exists, checked against the documented rules
with synthetic samples; nothing yet against real Figma files. Remaining: 6–12 h.

| Feature | Status | Evidence / notes |
|---|---|---|
| Horizontal, vertical, wrap | done | `crates/layout` |
| Grid auto layout: fixed / hug / `fr` tracks, cells, spans, gaps | done | `fr` rows of a fixed-height grid behave like `minmax(0, 1fr)` around fixed-size children; Figma's exact rule unconfirmed |
| Gap, auto gap, padding (per side), alignment 3×3, space between | done | |
| Text baseline alignment | done | |
| Hug / Fill / Fixed sizing with coherent rules (Fill turns a hugging parent Fixed) | done | `al_nesting_tests.rs` |
| Min/max width and height, redistribution | done | hugging text wraps at its max |
| Nested hug/fill settling in one pass (three levels, wrapping fill text, grid rows) | done | |
| Absolute position (ignore auto layout) with constraints | done | |
| Canvas stacking order (first on top / last on top) | done | |
| On-canvas padding and gap handles, incl. wrap row gap and grid gaps | done | |
| Drag to reorder (wrap-aware, by row), into grid cells, across nested frames | done | `al_drag_tests.rs` |
| Arrow keys move children one place (flows, rows, grid cells, into neighbours) | done | `al_keys_tests.rs` |
| Constraints: left, right, left and right, centre, scale; top/bottom likewise | done | |
| Layout grids: columns, rows, grid; stretch/left/centre/right; variables bound | done | |
| Suggest auto layout / "Add auto layout" on a selection | done | |
| Real Figma files laid out identically | **not checked** | needs `.fig` or REST import of real files ([gaps](gaps.md) #1, #3) |
| Performance: re-flow only where sizes or places changed | done | measured in `cargo xtask bench` (components at scale) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-10 | major | Created from the 2026-10-10 re-measure |
