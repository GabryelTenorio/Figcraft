# UI and interaction parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (gamut warnings and soft-proof sRGB; boolean hit testing on the result, group drags, frame outlines and slices; outlines for the selection, mask outlines; arc fields, live boolean editing, shape builder on strokes, one-sided widths) · **Target:** Figma Design UI3 (desktop app 126.10.7)

Fine canvas control is what Figma users feel first: handles, snapping, nudging, modifier keys,
drag behaviour, numeric precision. This checklist covers the tools, gestures and panels. Status:
**done** (built and checked against Figma's documented behaviour or by eye), **partial**, **missing**.
Nothing here has been compared tool by tool against the running Figma yet; that pass is the main
remaining work (40–60 h, [gaps](gaps.md) #4).

**Summary (estimated):** UI/UX fidelity ~58%. Layout and look of UI3 compared side by side (~65%);
interaction feel not yet compared.

## Window and panels

| Item | Status | Notes |
|---|---|---|
| UI3 layout: tab strip as title bar, icon rail, file header, pages + layers, floating bottom toolbar with flyouts, Design / Prototype / Inspect panels | done | frameless window on Windows/Linux with drawn buttons |
| Light and dark themes | done | colours from `theme::Tokens` |
| Minimize UI (Cmd+\\) | done | |
| Resizable panels | partial | check widths and snap points against Figma |
| Actions palette (Cmd+K, Cmd+/) | done | commands; no assets search parity |
| Context menus | done | canvas and layers; keyboard access to the layers context menu missing |
| Main menu from the logo | done | generated from `catalog.rs`; popups sized to content |
| Keyboard shortcuts panel | done | docked, tabbed by group, searchable, per-platform keycaps |
| Multiplayer avatars, Share button | missing | waits for multiplayer |

## Selection and transform

| Item | Status | Notes |
|---|---|---|
| Click, Shift-click, marquee, Shift-marquee toggle | done | |
| Deep select (Cmd/Ctrl-click, Cmd/Ctrl-marquee) | done | |
| Select layer menu (every layer under the click) | done | |
| Enter / Shift+Enter (children / parent), Tab / Shift+Tab (siblings) | done | Tab renames the next layer while renaming |
| Resize handles: Shift keeps proportions, Alt from centre, constraints-aware | done | |
| Rotate just outside a corner; Shift in 15° steps | done | rotated handle box when layers share a rotation |
| Flip, rotate 90° | done | |
| Alt-drag duplicate | done | |
| Scale tool (K): handles about an anchor, live factor, panel | done | |
| Nudge 1 / Shift 10 (preference), Cmd+arrows resize | done | arrows reorder auto layout children |
| Smart guides: edges, centres, spacing labels, parent, layout grid, ruler guides | done | red, `snap.rs`; thresholds not yet compared with Figma's |
| Ctrl to suspend snapping while dragging (macOS) | missing | |
| Measure (Alt hover, Cmd+Alt nested, Shift+Alt padding/gaps) | done | labels never overlap |
| Pixel grid snapping, pixel preview, 2x preview | done | |
| Numeric fields: arithmetic, scrubbing by dragging the label, mixed values | partial | check scrub speed and Shift/Alt steps |
| Keyboard box selection | missing | |

## Tools

| Tool | Status | Notes |
|---|---|---|
| Move, hand (Space), zoom (hold Z, Alt out, drag area) | done | |
| Frame (presets panel, swap orientation), section, slice | done | |
| Rectangle, ellipse (arc handles and Design panel arc fields), polygon, star, line, arrow; Shift/Alt | done | |
| Pen: vector networks, branch, close, bend; pencil | done | pen pressure on Windows and from browser styluses only |
| Vector edit mode: bend, paint bucket, lasso, cut, eraser, transform box, point snapping, mirroring | done | |
| Shape builder (Shift+M) on fills, strokes and open paths; variable width (Shift+W), one side with Alt | done | |
| Boolean groups: clicked where the result is, double-click or Cmd/Ctrl-click into operands, live results while dragging or editing points, per-operand operations | done | |
| Text: point, auto height, fixed; caret, words, lines, IME | done | no spell check |
| Eyedropper (I) | done | |
| Comment (C), Shift+C to show/hide | done | local only |
| Text on a path (Figma Draw) | done | put on from the Design panel or `text.onPath`, not by clicking a path with the Text tool; the path is edited by resizing only |
| Stretch and scatter brushes (Figma Draw) | done | eight presets drawn in code; the Pencil's brush is set by `tool.pencilBrush`, not from the toolbar |
| Image crop handles | done | |

## Canvas

| Item | Status | Notes |
|---|---|---|
| Pan/zoom: wheel, pinch, Space-drag, Shift+0/1/2, Cmd+0, `+`/`-` | done | |
| Rulers and guides (drag out, move, remove; rotated frames; frame guides) | done | |
| Frame labels (rotated, right-to-left), size badge | done | |
| Outline mode | done | Outlines, outlines for the selection, mask outlines, frame outlines |
| Slices shown on the canvas (View ▸ Slices) | done | dashed rectangles |
| Dragging a layer inside a group keeps it in the group | done | frames and auto layout take other drops |
| Layout grids (Ctrl+G) | done | |
| Gamut warnings for Display P3 files | done | colour picker note; View ▸ Soft-proof sRGB stripes what sRGB can't show (the canvas itself is sRGB) |
| Gradient and padding/gap handles on the canvas | done | |
| Hover outline, selection colours, handle sizes | done | measured from UI3 screenshots |
| Cursors per tool and modifier | partial | not compared |
| Multiplayer cursors, cursor chat, spotlight | missing | |

## Accessibility (ours vs Figma's)

| Item | Status | Notes |
|---|---|---|
| Screen-reader names, roles, values, states (AccessKit) | done on desktop | browser: eframe doesn't send the tree yet |
| Keyboard navigation: F6 regions, Tab within, focus ring | done | |
| Reduced motion | done | system setting or preference |
| WCAG AA contrast of secondary and accent text | done | |
| Tested with VoiceOver, NVDA, Narrator, Orca | missing | human QA |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Gamut warnings: the colour picker flags Display P3 colours outside sRGB; View ▸ Soft-proof sRGB stripes them on the canvas |
| 2026-10-11 | minor | Canvas fixes: boolean groups clicked where their result is, a layer dragged inside a group stays in it, View ▸ Frame outlines and View ▸ Slices draw |
| 2026-10-11 | minor | Outline mode done: outlines for the selection (the selected layers draw as outlines, the rest normally) and mask outlines |
| 2026-10-11 | minor | Design panel arc fields, live boolean editing (operands on the canvas, live results, per-operand operations), shape builder on strokes and open paths, one-sided widths (Alt-drag a width handle) |
| 2026-10-10 | major | Created from the ROADMAP's look-and-feel and interaction items, the keymap and the canvas code |
