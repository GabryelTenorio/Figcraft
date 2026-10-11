# UI and interaction parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-12 · **Change:** minor (layers and assets panels behaviour suite and fixes; vector multi-edit and the vector edit tail; canvas navigation, guides and grids behaviour suite and fixes; shapes and vector behaviour suite and fixes; paint and effects behaviour suite and fixes; frames, groups, sections and masks behaviour suite and fixes; layers panel keyboard: F2 rename, Shift+F10 context menu, F8 box selection; selection and transform behaviour suite and fixes; Display P3: gamut warning and soft-proof (true P3 blocked upstream); several windows; boolean hit testing on the result, group drags, frame outlines and slices; outlines for the selection, mask outlines; arc fields, live boolean editing, shape builder on strokes, one-sided widths) · **Target:** Figma Design UI3 (desktop app 126.10.7)

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
| Blend mode menus: the layer's (Design panel) and each paint's (colour picker) | done | Pass through and Normal are distinct (Normal isolates a group's or frame's children); paints have no pass-through; plus darker drawn exactly on the canvas |

## Layers and Assets panels

| Item | Status | Notes |
|---|---|---|
| Rows: click, Shift-click range, Cmd/Ctrl-click toggle; the selection follows on the canvas and a canvas selection is revealed | done | `layers_behaviour_tests` |
| Expand and collapse: Alt-click opens or closes everything inside; Collapse layers (Alt+L) keeps the way to the selection open | done | Alt-click pinned (the documentation is silent) |
| Drag to reorder and into frames, groups and auto layout: insertion line, spring-loaded open, several layers together, Escape cancels | done | several layers dropped together land together (they split apart before); instances and a layer's own children refuse drops |
| Eye and padlock: click, or drag across rows to set several in one step; layers in a locked frame show a faint padlock | done | the toggles no longer vanish under the pointer (clicks on them could miss) |
| Rename: double click, F2 / Cmd+R, Tab to the next row; several layers open Rename layers (match, regular expressions, current name and number codes, start value, preview) | done | numbered in stacking order, pinned |
| Find layers (filter by name) | done | not covered by the documentation |
| Pages: add, rename, drag to reorder, duplicate ("Copy of"), delete, dividers (a blank page named with a hyphen) | done | |
| Assets panel: folders from page, frames and slash names (fold away), search, thumbnails, drag onto the canvas or into frames and auto layout, Alt-drop swap, enabled libraries | done | |
| Hide when publishing from the Assets panel's context menu | missing | a leading `.` or `_` in the name only |

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
| Pixel grid snapping, pixel preview, 2x preview | done | moves, resizes and new shapes land on whole pixels; the pixel grid shows from 400% (`view_behaviour_tests`) |
| Numeric fields: arithmetic, scrubbing by dragging the label, mixed values | partial | check scrub speed and Shift/Alt steps |
| Transform box at small sizes | done | a press inside a layer smaller than its handles moves it, the nearest handle wins, handles answer just outside and are hidden under 16 px on screen; cursors follow each handle and the box's rotation |
| Locked layers | done | selectable from the layers panel; no handles, not dragged with the rest of a selection |
| Keyboard box selection | done | F8 (`select.area`): a box around the selection or in the middle of the view; arrows move it, Shift+arrows size it, Enter selects like a marquee (Shift+Enter toggles, Cmd/Ctrl+Enter nested); our addition for keyboard-only users |

## Tools

| Tool | Status | Notes |
|---|---|---|
| Move, hand (Space), zoom (hold Z, Alt out, drag area) | done | |
| Frame (presets panel, swap orientation), section, slice | done | |
| Rectangle, ellipse (arc handles and Design panel arc fields), polygon, star, line, arrow; Shift/Alt | done | Alt draws lines from the centre too (`shapes_behaviour_tests`) |
| Corner radius handles (inside each corner of a rectangle or frame, a polygon's or star's top point; Alt: one corner), star ratio handle; Count and Ratio fields | done | handles hide under 48 px on screen; no count handle on the canvas (the Count field instead); corner smoothing rounds rectangles and frames only |
| Pen: vector networks, branch, close, bend; pencil | done | pen pressure on Windows and from browser styluses only; outside edit mode the Pen closes on its first point, connects to its other points and continues a selected open path from an end point (`vector_multi_behaviour_tests`) |
| Vector edit mode: bend, paint bucket, lasso, cut, eraser, transform box, point snapping, mirroring | done | paint bucket drags across areas; cut drags divide a shape into layers; delete and heal follows the curve; rounded points are drawn; Shift-click deselects a point at a corner of the box (`vector_behaviour_tests`); several vectors edited at once, Shift-click handles to drag them together, a cap and join per point, rounded points beside curves (`vector_multi_behaviour_tests`). Not yet: the transform box around points of several layers |
| Shape builder (Shift+M) on fills, strokes and open paths; variable width (Shift+W), one side with Alt | done | |
| Boolean groups: clicked where the result is, double-click or Cmd/Ctrl-click into operands, live results while dragging or editing points, per-operand operations | done | |
| Text: point, auto height, fixed; caret, words, lines, IME | done | carets step over whole emoji and accented letters; CJK words by script run (no dictionary); vertical trim in Type settings; no spell check |
| Eyedropper (I) | done | picks the colour seen under the pointer (opaque) into the selection's fill and the recent colours, one undo step (`fx_behaviour_tests`) |
| Comment (C), Shift+C to show/hide | done | local only |
| Text on a path (Figma Draw) | done | put on from the Design panel or `text.onPath`, not by clicking a path with the Text tool; the path is edited by resizing only |
| Stretch and scatter brushes (Figma Draw) | done | eight presets drawn in code; the Pencil's brush is set by `tool.pencilBrush`, not from the toolbar |
| Image crop handles | done | |

## Canvas

| Item | Status | Notes |
|---|---|---|
| Pan/zoom: wheel, pinch, Space-drag, Shift+0/1/2, Cmd+0, `+`/`-` | done | Shift-wheel pans sideways, pinch and the zoom tool keep the point under the pointer; zoom steps, limits (2%–25,600%) and frame order pinned where the documentation is silent (`view_behaviour_tests`) |
| Rulers and guides (drag out, move, remove; rotated frames; frame guides) | done | guides belong to the page or a top-level frame (a frame moved into another keeps its guides but they stop applying); a click selects one for Delete, the arrows or the context menu's Remove guide; Alt-drag drops a copy; Alt while dragging measures to the layers in the frame it crosses; ruler numbering checked by eye only |
| Frame labels (rotated, right-to-left), size badge | done | |
| Outline mode | partial | Outlines, outlines for the selection, mask outlines, frame outlines; shapes are clicked on their outlines, reaching layers behind; the Include hidden layers and Include object bounds options are missing |
| Slices shown on the canvas (View ▸ Slices) | done | dashed rectangles |
| Dragging a layer inside a group keeps it in the group | done | frames and auto layout take other drops |
| Frames and sections drawn around layers take those fully inside; sections never drop into frames | done | not in auto layout frames |
| Resizing a section leaves its contents in place | done | |
| Masks: what a mask hides doesn't take clicks; a mask group's box is the mask's | done | |
| Layout grids (Ctrl+G) | done | columns, rows and grid; stretch, left, centre, right; Auto count; several per frame; styles; not shown or snapped on a rotated frame; a nested frame's grids snap but only top-level frames draw them |
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
| 2026-10-12 | minor | Layers and Assets panels behaviour suite and fixes: several layers dropped together land together, Collapse layers keeps the selection's rows open, dragging across eyes or padlocks sets several layers in one step, the toggles stay under the pointer, Rename layers dialog for several layers, pages dragged to reorder, dividers, "Copy of" duplicates, Assets folders by page, frame and slash names |
| 2026-10-12 | minor | Vector multi-edit and the vector edit tail, with a behaviour suite written from the public documentation (5 tests, about 30 table rows and steps, and a junk-parameter sweep of every vector command in multi-edit mode): several vector layers are edited at once (Enter, double-click or `vector.edit.begin {ids}`; a press on another layer's point, handle or segment makes it the active layer; Shift-click, marquee and lasso select across layers; dragging, the arrows, Delete, delete and heal, mirroring, smoothing, corner radius and point caps and joins reach every layer's selected points in one undo step; the pen, bend, paint bucket, cut and eraser work in the active layer); several Bezier handles are selected with Shift-click and dragged together (`vector.edit.moveHandles`); a sharp point next to a curved segment rounds (the curve is cut where the arc meets it and keeps its shape); a point can have its own stroke cap and join (`vector.edit.strokeCap`, `vector.edit.strokeJoin`, the Point section of the Design panel; canvas, PNG, SVG and PDF, outline stroke); the Pen outside edit mode connects to one of its own points and ends the path, and continues a selected open path from an end point. Left: corner smoothing beyond rectangles, the transform box around points of several layers, per-point caps and joins on dashed, variable-width and brush strokes |
| 2026-10-11 | minor | Canvas navigation, rulers, guides, layout grids, outline mode and measuring behaviour suite and fixes: the pixel grid shows from 400% (it waited for 800%); guides belong to the page or a top-level frame; a guide can be selected, deleted, nudged and removed from the context menu, Alt-dragged into a copy, and measured from with Alt while dragging; Auto column and row counts; layout grids hidden on rotated frames; outline mode clicks reach layers behind. Outline mode back to partial (two documented options missing) |
| 2026-10-11 | minor | Shapes and vector behaviour suite and fixes: corner radius and star ratio handles on the canvas, Count and Ratio fields, Alt draws lines from the centre, paint bucket and cut drags, delete and heal along the curve, rounded vector points drawn, Shift-click deselects a point at a box corner, the miter field shows degrees, individual corners only for rectangles and frames |
| 2026-10-11 | minor | Paint and effects behaviour suite: the blend menu's Normal sets Normal (it set pass-through), a blend menu for each paint in the colour picker, the eyedropper's rules covered by tests |
| 2026-10-11 | minor | Frames, groups, sections and masks: drawing a frame or section around layers takes them, sections stay out of frames, section resizing leaves contents in place, masked-out areas don't take clicks, a mask group's box follows the mask, X/Y of layers in groups are measured in the frame |
| 2026-10-11 | minor | Layers panel keyboard: F2 renames (also from a row with the keyboard, which gets it back), Shift+F10 opens the context menu at the focused row or below the canvas selection, F8 draws a selection box the arrows move and size; stroke weights bind to variables side by side |
| 2026-10-11 | minor | Selection and transform behaviour suite and fixes: Alt shrinks, groups and mixed rotations scale each layer along its own axes, several layers flip together, whole-pixel resizes, small layers move from inside, locked layers stay put, marquees from a top-level frame's background, equal spacing snapping, Cmd/Ctrl ignores constraints while resizing a frame |
| 2026-10-11 | minor | Gamut warnings: the colour picker flags Display P3 colours outside sRGB; View ▸ Soft-proof sRGB stripes them on the canvas |
| 2026-10-11 | minor | Several windows: File ▸ New window, Open in new window on a tab, a tab dragged off the strip; each window has its own tabs and panels and asks about unsaved changes when it closes |
| 2026-10-11 | minor | Canvas fixes: boolean groups clicked where their result is, a layer dragged inside a group stays in it, View ▸ Frame outlines and View ▸ Slices draw |
| 2026-10-11 | minor | Outline mode done: outlines for the selection (the selected layers draw as outlines, the rest normally) and mask outlines |
| 2026-10-11 | minor | Design panel arc fields, live boolean editing (operands on the canvas, live results, per-operand operations), shape builder on strokes and open paths, one-sided widths (Alt-drag a width handle) |
| 2026-10-10 | major | Created from the ROADMAP's look-and-feel and interaction items, the keymap and the canvas code |
