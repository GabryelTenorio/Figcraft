# UI and interaction parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-11 · **Change:** minor (layers panel keyboard: F2 rename, Shift+F10 context menu, F8 box selection; selection and transform behaviour suite and fixes; Display P3: gamut warning and soft-proof (true P3 blocked upstream); several windows; boolean hit testing on the result, group drags, frame outlines and slices; outlines for the selection, mask outlines; arc fields, live boolean editing, shape builder on strokes, one-sided widths) · **Target:** Figma Design UI3 (desktop app 126.10.7)

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
| Context menus | done | canvas and layers; Shift+F10 opens them from the keyboard (at the focused layer row or below the canvas selection) and the menu takes the keyboard; the Menu key isn't delivered by the windowing layer (egui 0.36 has no key for it) |
| Main menu from the logo | done | generated from `catalog.rs`; popups sized to content |
| Keyboard shortcuts panel | done | docked, tabbed by group, searchable, per-platform keycaps |
| Several windows | partial | File ▸ New window, Open in new window on a tab, a tab dragged off the strip; screen-reader tree in the main window only; positions not restored |
| Multiplayer avatars, Share button | missing | waits for multiplayer |

## Selection and transform

| Item | Status | Notes |
|---|---|---|
| Click, Shift-click, marquee, Shift-marquee toggle | done | a marquee selects what it touches; from a top-level frame's background, the layers inside it; locked and hidden layers are skipped (`selection_behaviour_tests`) |
| Deep select (Cmd/Ctrl-click, Cmd/Ctrl-marquee) | done | |
| Select layer menu (every layer under the click) | done | |
| Enter / Shift+Enter (children / parent), Tab / Shift+Tab (siblings) | done | Tab renames the next layer while renaming |
| Resize handles: Shift keeps proportions, Alt from centre, constraints-aware | done | Alt shrinks too; past an edge flips (several layers mirror); groups and mixed rotations scale each layer along its own axes, keeping rotation and flip (no skew: children turned off a right angle are approximate); Cmd/Ctrl ignores a frame's constraints (`transform_behaviour_tests`) |
| Rotate just outside a corner; Shift in 15° steps | done | rotated handle box when layers share a rotation |
| Flip, rotate 90° | done | |
| Alt-drag duplicate | done | |
| Scale tool (K): handles about an anchor, live factor, panel | done | |
| Nudge 1 / Shift 10 (preference), Cmd+arrows resize | done | arrows reorder auto layout children |
| Smart guides: edges, centres, equal spacing (between two layers or continuing a row's gap, labelled), spacing labels, parent, layout grid, ruler guides | done | red, `snap.rs`; thresholds not yet compared with Figma's |
| Ctrl to suspend snapping while dragging (macOS) | missing | |
| Measure (Alt hover, Cmd+Alt nested, Shift+Alt padding/gaps) | done | labels never overlap |
| Pixel grid snapping, pixel preview, 2x preview | done | moves, resizes and new shapes land on whole pixels |
| Numeric fields: arithmetic, scrubbing by dragging the label, mixed values | partial | check scrub speed and Shift/Alt steps |
| Transform box at small sizes | done | a press inside a layer smaller than its handles moves it, the nearest handle wins, handles answer just outside and are hidden under 16 px on screen; cursors follow each handle and the box's rotation |
| Locked layers | done | selectable from the layers panel; no handles, not dragged with the rest of a selection |
| Keyboard box selection | done | F8 (`select.area`): a box around the selection or in the middle of the view; arrows move it, Shift+arrows size it, Enter selects like a marquee (Shift+Enter toggles, Cmd/Ctrl+Enter nested); our addition for keyboard-only users |

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
| Text: point, auto height, fixed; caret, words, lines, IME | done | carets step over whole emoji and accented letters; CJK words by script run (no dictionary); vertical trim in Type settings; no spell check |
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
| Layers panel from the keyboard: arrows, F2 / Cmd+R rename (the row gets the keyboard back), Shift+F10 context menu; 10k-layer lists | done | revealing a layer, expanding, collapsing and arrow keys timed in `cargo xtask bench` |
| Reduced motion | done | system setting or preference |
| WCAG AA contrast of secondary and accent text | done | |
| Tested with VoiceOver, NVDA, Narrator, Orca | missing | human QA |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | minor | Layers panel keyboard: F2 renames (also from a row with the keyboard, which gets it back), Shift+F10 opens the context menu at the focused row or below the canvas selection, F8 draws a selection box the arrows move and size; stroke weights bind to variables side by side |
| 2026-10-11 | minor | Selection and transform behaviour suite and fixes: Alt shrinks, groups and mixed rotations scale each layer along its own axes, several layers flip together, whole-pixel resizes, small layers move from inside, locked layers stay put, marquees from a top-level frame's background, equal spacing snapping, Cmd/Ctrl ignores constraints while resizing a frame |
| 2026-10-11 | minor | Gamut warnings: the colour picker flags Display P3 colours outside sRGB; View ▸ Soft-proof sRGB stripes them on the canvas |
| 2026-10-11 | minor | Several windows: File ▸ New window, Open in new window on a tab, a tab dragged off the strip; each window has its own tabs and panels and asks about unsaved changes when it closes |
| 2026-10-11 | minor | Canvas fixes: boolean groups clicked where their result is, a layer dragged inside a group stays in it, View ▸ Frame outlines and View ▸ Slices draw |
| 2026-10-11 | minor | Outline mode done: outlines for the selection (the selected layers draw as outlines, the rest normally) and mask outlines |
| 2026-10-11 | minor | Design panel arc fields, live boolean editing (operands on the canvas, live results, per-operand operations), shape builder on strokes and open paths, one-sided widths (Alt-drag a width handle) |
| 2026-10-10 | major | Created from the ROADMAP's look-and-feel and interaction items, the keymap and the canvas code |
