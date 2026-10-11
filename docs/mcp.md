# FigCraft MCP server

FigCraft ships a [Model Context Protocol](https://modelcontextprotocol.io) server so AI agents can
design with it: create and edit layers, run any menu command, inspect the result as JSON and look at
it as a PNG. It speaks newline-delimited JSON-RPC 2.0 over stdio (MCP protocol `2025-06-18`;
`2025-03-26` and `2024-11-05` clients are accepted too).

The server lives in `crates/mcp` (`figcraft-mcp`) and is started by the CLI:

| Command | Backend |
|---|---|
| `figcraft-cli mcp` | **Headless**: an in-process engine session with one untitled document. No window; renders with the CPU renderer. |
| `figcraft-cli mcp --attach <port>` | **Attached**: drives a running FigCraft app through its control channel (`figcraft --control <port>`). The user sees every change live. |

`--attach` also takes `host:port`. The control channel only listens on 127.0.0.1.

## Configure a client

Build the CLI first (`cargo build --release -p figcraft-cli`) and use the absolute path to
`target/release/figcraft-cli`.

### Claude Code

```sh
# headless
claude mcp add figcraft -- /path/to/figcraft-cli mcp
# drive the running app (start it with: figcraft --control 7979)
claude mcp add figcraft-app -- /path/to/figcraft-cli mcp --attach 7979
```

Or in a project's `.mcp.json`:

```json
{
  "mcpServers": {
    "figcraft": { "command": "/path/to/figcraft-cli", "args": ["mcp"] }
  }
}
```

### Claude Desktop

`claude_desktop_config.json` (Settings > Developer > Edit Config):

```json
{
  "mcpServers": {
    "figcraft": { "command": "/path/to/figcraft-cli", "args": ["mcp"] },
    "figcraft-app": { "command": "/path/to/figcraft-cli", "args": ["mcp", "--attach", "7979"] }
  }
}
```

## Conventions agents must know

- Coordinates are **page units** (1 unit = 1 px at 100% zoom), x right, **y down**.
- `create_node` and `set_properties` take `x`/`y` **relative to the parent** (the page when there is
  no parent, so they are page coordinates then). `inspect_document` reports parent-relative `x`/`y`
  plus `absoluteBounds` in page coordinates.
- Children of an auto-layout frame are positioned by the layout: set `layout`, `gap`, `padding`,
  `mainAlign`, `crossAlign`, `sizingH`/`sizingV` instead of x/y. More auto layout props:
  - `sizingH` / `sizingV`: `fixed`, `hug` (auto layout frames and text) or `fill` (children in an
    auto layout flow). A child can't fill an axis its parent hugs: setting `fill` turns the parent
    `fixed` on that axis, and a parent set to `hug` turns the children that filled it `fixed`. A
    layer moved out of auto layout (or whose frame loses it) keeps its size as `fixed`. Fill
    children share the space like CSS flexbox, within their min/max.
  - `minWidth` / `maxWidth` / `minHeight` / `maxHeight` (px, `null` removes) bound Hug, Fill and
    Fixed sizes; a hugging text wider than its max width wraps.
  - `absolute: true` takes a child out of the flow (it stays where it is and follows its
    `constraints` when the frame resizes; Fill becomes Fixed).
  - `crossAlign: "baseline"` lines up the first text baselines in horizontal layouts (other layers
    align to the top; nested auto layout frames use their first child's baseline).
  - `wrap` with `crossGap` (alias `rowGap`) for the gap between wrapped rows.
  - Grid (`layout: "grid"`): `gridColumns`, `gridRows` (minimum; rows grow), `columnGap` /
    `rowGap`, track sizes `columnTracks` / `rowTracks` as `[120, "1fr", "auto"]` (px fixed, `fr`
    fill share, `auto` hug) or the `layout.setTrack {axis, index, sizing, value?}` command; on a
    child, `gridColumn` / `gridRow` (0-based anchor cell, `null` = placed automatically) and
    `gridColumnSpan` / `gridRowSpan`. `inspect` reports a grid child's resolved `gridCell`.
- **Every menu action is a command** (`list_commands`), and most commands act on `ids` when given,
  else on the selection. New layers become the selection.
- Colours are `"#RRGGBB"` or `"#RRGGBBAA"`. Inside fills, strokes and effects passed through the MCP
  tools, `color` may also be a hex string and gradients can be written
  `{"type": "linearGradient", "stops": ["#7048E8", "#1098AD"]}`; the server converts them to the
  engine's paint JSON.
- **Colour profile**: colour values are in the file's profile, sRGB (default) or Display P3
  (`inspect_document` reports `colorProfile`). `file.colorProfile {profile: srgb|displayP3,
  convert?}` changes it: `convert` (default true) converts every colour value so colours keep their
  appearance; `false` keeps the values and reinterprets them. Images are treated as sRGB and managed
  into the file's space. PNG exports are tagged (`sRGB` chunk, or a Display P3 ICC profile plus
  `cICP`), Display P3 JPG and WebP embed the ICC profile, and SVG and PDF export Display P3 files
  converted to sRGB (out-of-gamut colours clip).
- **Image adjustments**: image paints carry `filters` (exposure, contrast, saturation, temperature,
  tint, highlights, shadows; each -1..1). `image.adjust {ids?, target?: fill|stroke, index?,
  preset?, filters?}` applies a built-in preset (`vivid`, `muted`, `warm`, `cool`, `blackAndWhite`,
  `fade`, `reset`) or one saved in the file, with any `filters` values on top.
  `image.savePreset {name, filters?}` saves the current (or given) adjustments in the file,
  `image.deletePreset {name}` removes one and `image.presets` lists both kinds.
- Text props (`fontFamily`, `fontStyle`, `fontSize`, `lineHeight`, `letterSpacing`, `textDecoration`,
  `textCase`, `textFills`, `fontVariations`…) apply to the whole layer, or to chars `start`..`end`
  when given (`text.style {id, start, end, …}` does the same). Styled ranges come back as
  `styledRuns`. Variable fonts: `fontVariations` (alias `variationSettings`) sets axis values in
  user units, e.g. `{"wght": 550, "wdth": 85}` (`null` = back to what the style name implies);
  `text.setVariations {variations, replace?}` merges instead. `text.fonts {family, style?, axes: true}`
  lists a font's axes and named instances (named instances are also valid `fontStyle` values).
  OpenType features: `features {"liga": false, "tnum": true}`.
- **Text styles** link chars, not just layers: `style.apply {style, ids?, start?, end?}` with a text
  style links chars `start`..`end` (while text is edited, the selected chars; else whole layers), and
  `style.detach {which: "text", start?, end?}` unlinks them (values stay). A layer's `styles.text` is
  the style all its chars share (none when they differ); `get_node` lists the linked chars as
  `textStyleRanges` (`[start, end, style id]`).
  Changing what a text style sets (font, size, line height, letter spacing, decoration, case, axis
  values) on linked chars unlinks them; `style.update` moves every linked char. `style.create
  {type: "text", name}` takes the selected chars while editing; a `value` may use the `node.set`
  spellings (`{"fontSize": 32, "lineHeight": "120%", "letterSpacing": "2%"}`).
- **Live previews**: `text.preview {command, params}` shows what `style.apply`,
  `library.applyStyle`, `text.setVariations` or `text.style` would do on the canvas without an undo
  step (in place of an earlier preview); `text.endPreview` puts the document back. Any other command
  ends the preview first, so running the command for real commits it as one undo step.
- Every mutation is one undo step (`batch` makes several one step). A failing command leaves the
  document as it was.
- Programmatic calls never open dialogs.
- Verify your work: `inspect_document` / `get_node` for numbers, `screenshot` for pixels.

## Tools

| Tool | What it does |
|---|---|
| `list_commands {filter?}` | Every command: id, label, menu, shortcut, undoable, params doc. |
| `run_command {command, params}` | Run any command (e.g. `align.left`, `layout.add`, `component.create`, `instance.place` (drop an instance at a page point into the frame or auto layout slot under it), `component.exposeInstance` / `instance.exposed` (nested instance properties on instances), `component.makeSlot` / `instance.slotInsert` / `instance.resetSlot` (slots, below), `inspect.code` (CSS, HTML + CSS for a whole frame, React, SwiftUI or Jetpack Compose for a layer; components take their properties as parameters and instances are calls, see Dev Mode below), `inspect.properties` (layout, auto layout, typography, fills, strokes and effects with their style and variable names), `inspect.assets` (layers under the selection with export settings), `style.create`, `variable.bind`, `variable.update` (rename, describe, scope), `variable.set` with `{alias}`, `variable.renameMode` / `variable.removeMode`, `variable.deleteCollection`, `proto.interaction.add`). |
| `batch {commands: [{command, params}], label?}` | Several commands as one undo step (`command.batch`); all-or-nothing. |
| `create_node {type, x, y, width, height, parent, index, name, text, path, props}` | Create a frame, rectangle, ellipse, polygon, star, line, arrow, text, vector, section, component or slice; returns `{id, node}`. |
| `set_properties {ids, props}` | `node.set` on layers (geometry, fills, strokes, effects, corner radius, auto layout, sizing, constraints, masks (`isMask`, `maskType: alpha\|vector\|luminance`), handoff status (`devStatus: none\|readyForDev\|completed`; marking again clears "Changed"), a section's `sectionContentsHidden`, text…); returns the updated layers. |
| `inspect_document {depth, allPages}` | The document as JSON (layer tree, styles, selection, history). |
| `get_node {id, depth}` | One layer as JSON. |
| `select {ids}` | Replace the selection (`[]` deselects). |
| `export {ids, format, scale \| size, path?, bleed?, cropMarks?, bleedMarks?, cmyk?}` | png, jpg, svg, webp, pdf; `size` is `2x`, `512w` or `512h`. PDF: frames one page each (sized in points), anything else together on one page; print options: `bleed` (points or a length like `"3mm"`, `"0.125in"`), crop and bleed marks, CMYK colour (see Exports below), also taken by `file.export`, `file.exportLayers`, `file.exportFramesPdf` and stored per export setting by `export.add` / `export.update`; `run_command` `file.exportFramesPdf {page?, path?}` writes every top-level frame of a page. Without `path`, images come back as MCP image content, SVG as text. Layers' own export settings: `run_command` with `export.add` / `export.update {index, size?, suffix?, format?}` / `export.remove` and `file.exportLayers {ids?, dir?}` (every layer once per setting, named `<layer><suffix>.<format>`). |
| `screenshot {ids?, scale?, maxSize?, window?}` | PNG of the page (with background) or of layers, scaled to fit `maxSize` (default 1600). `window: true` captures the app window when attached. |
| `open_file {path, variables?}` / `save_file {path?}` | Open `.figcraft`, `.json` (FigCraft JSON or REST API file JSON with its variables, see below), `.svg` (see Importing SVG below) or an image; save as `.figcraft`. |
| `undo` / `redo` | Step through history. |
| `pointer {kind, x, y, to?, steps?, tool?, shift, alt, cmd}` | Mouse gestures in page coordinates with the current tool: `click`, `double`, `drag`, `down`, `move`, `up`, `hover`. |
| `key {key, shift, alt, cmd}` | A shortcut or key as if typed on the canvas (every default shortcut, tool keys, Escape, Delete, arrows, number keys for opacity; while editing text, the text shortcuts and then text editing keys). Keys are names like `A`, `0`, `Enter`, `ArrowLeft`, `=`, `/`, or egui's (`Plus`, `Questionmark`); `cmd` is Cmd on macOS and Ctrl elsewhere. |
| `type_text {text, id?}` | Type into the text being edited, or replace the contents of text layer `id`. |
| `inspect_ui {}` | Tool, view and status (and panels/timings when attached). |

### Sections, handoff and arrow keys

`run_command` with `object.wrapInSection {ids?, name?}` (Wrap in new section: layers on the page or
in a section, with some room around them), `section.toggleContents {ids?, hidden?}` (a section
hiding its contents shows only its name and outline; its layers aren't drawn or hit, and the layers
panel dims them), `object.resizeToFit`. A frame or section marked `devStatus: readyForDev` turns
"Changed" (`devChanged: true` in `get_node`) when a layer in it is edited, added or removed;
`dev.resolveChange {ids?, reason?}` returns it to Ready for dev and keeps the reason (`devNote`).
Frames in sections are top-level frames (flows can start there; `proto.flow.add`); a "Navigate to"
whose destination is a section shows the frame of it shown last in the presentation, else its
first.

`transform.nudge {dx, dy}` (arrow keys) moves auto layout children one place instead of nudging:
along the flow, to the previous/next row of a wrapping layout, to the neighbouring grid cell, and,
at the end of a flow or across a row's axis, into the neighbouring nested auto layout frame when the
frame around flows that way (a list of rows: Down moves an item into the row below).

### Keyboard shortcuts

The default shortcuts live in one table (`figcraft_engine::keymap`), grouped like the public shortcut
reference (Essential, Tools, View, Zoom, Text, Shape, Selection, Edit, Transform, Arrange,
Components, Prototype) with the macOS keys and, where they differ, the Windows/Linux keys (for
example Use as mask is ⌃⌘M on macOS and Ctrl+Alt+M elsewhere; Bring to front ⌥⌘] and
Ctrl+Shift+]). `list_commands` reports each command's first key for the platform the server runs
on; `key` resolves keys the way the canvas does. Commands behind keys:

- `object.opacityKeys {digits, ids?}`: the opacity number keys. One digit is tens of percent (`"0"`
  is 100%); two digits are the exact percentage (`"45"` is 45%, `"05"` is 5%), what typing two
  digits in quick succession does on the canvas.
- `view.zoomAt {x, y, out?, width?, height?}`: the zoom tool (hold Z on the canvas; Alt zooms out).
  A point zooms one step in (or out) keeping that page point in place; an area fills the view.
- `view.designPrototype` (`Shift+E`), `view.devMode` (`Shift+D`) and `view.leftSidebar`
  (`Shift+Cmd+\`) switch the right panel's tab or hide the left sidebar in the app (no-ops headless).
- `doc.nextTab` / `doc.previousTab` (`Ctrl+Tab` / `Ctrl+Shift+Tab`): the next or previous open file.
- `view.nextRegion` / `view.previousRegion` (`F6` / `Shift+F6`, live while editing text too): move
  the keyboard between the app's regions (toolbar, layers, canvas, properties). In the app, Tab and
  Shift+Tab then move between the region's controls, Enter or Space activate one, the arrows move
  along the toolbar and through the layer rows, and Escape returns to the canvas (no-ops headless).
- `pref.reduceMotion {on?}`: reduced motion (the interface's own animations off; the system's
  accessibility setting turns it on too). Prototype transitions still play.

### Editing vectors

Vector edit mode is driven with `run_command` (or `pointer`/`key` for gestures):
`vector.edit.begin {id}` (converts shapes to vectors), `vector.edit.state` (page-space overlay, the
local network, each point's handle mirroring and the width points), `vector.edit.tool {tool: move|pen|bend|paint|lasso|cut|eraser|width}`,
`vector.edit.select {vertices?, segments?, add?}`, `vector.edit.selectAll`, `vector.edit.moveVertices`,
`vector.edit.nudge`, `vector.edit.position {x?, y?}`, `vector.edit.setHandle`, `vector.edit.mirroring
{mode: none|angle|angleAndLength}`, `vector.edit.smooth {smooth?}`, `vector.edit.bend`,
`vector.edit.addVertex`, `vector.edit.connect {a, b}`, `vector.edit.pen {x, y, dx?, dy?}` (continues
from the pen's open end; clicking a point connects or branches), `vector.edit.penEnd`,
`vector.edit.paint {x, y}` (paint bucket: fills or unfills the closed area under a page point),
`vector.edit.cut {segment, t} | {vertex}`, `vector.edit.erase {points, radius?}` (the eraser),
`vector.edit.lasso {points}`, `vector.edit.delete`,
`vector.deleteHeal`, `vector.join` / `vector.smoothJoin` (join the selected points while editing),
`vector.edit.cornerRadius`, `vector.edit.end`. Point indices refer to `network.vertices` in
`vector.edit.state`; local coordinates are the layer's own.

The selected points' transform box: `vector.edit.bounds` (`{x, y, width, height, corners,
points}`: the box in the vector's axes, `x`/`y` in the parent's coordinates like the Position
fields), `vector.edit.resize {width?, height?, center?}` (keeps the top-left unless `center`),
`vector.edit.rotate {angle, cx?, cy?}` (degrees counter-clockwise about the box centre or a page
pivot), `vector.edit.transform {matrix: [a, b, c, d, e, f], vertices?}` (any page-space affine).
While points are selected, `object.flipH` / `object.flipV` and the 90°/180° rotations act on them.
On the canvas (`pointer`), dragging points, handles and pen clicks snap to other points and segment
midpoints (this vector's and other vectors'), the usual layer targets and the pixel grid; `cmd`
held while dragging turns the object snapping off.

Variable width strokes: `stroke.widthProfile {ids?, preset?: uniform|wedge|taper|eye|rounded|pinch,
points?: [[t, scale], ...]}` (also `node.set {widthProfile}`; `t` runs 0–1 along each subpath by
length, `scale` multiplies the stroke weight, widths in between are interpolated smoothly; no
preset or points = uniform). While editing, the Variable width tool (`width`, ⇧W) drags width
points: `vector.edit.widthPoint {t, width? (px) | scale?}` adds one, `{index, t?, width? | scale?}`
moves or resizes it, `{action: "remove", index}` deletes it. Rendering, PNG/SVG/PDF export and
`vector.outlineStroke` follow the profile.

One-sided widths: a width point can be wider on one side of the path than the other (left and
right looking along the path, y down). Points are `[t, left, right]` (or `{t, left, right}`) where
each side's scale is what `scale` would be with both sides that wide, so `[t, s]` equals
`[t, s, s]`; `get_node` reports one-sided points that way. `vector.edit.widthPoint {index, side:
left|right, width? | scale?}` changes one side (`width`: the distance from the centre line to that
edge; with `t` it adds a point keeping the other side as it is); a width without `side` makes the
point symmetric again. With `pointer`, `alt: true` while dragging a handle (or the path) changes
that side only; `vector.edit.state` lists `widthPoints` with both handle positions (`sides`:
right, left) and `oneSided`.

### Prototypes

Interactions are `proto.interaction.add` / `set` / `remove` / `clear` (a navigate action can keep
the scroll position with `preserveScroll: true`). Variable actions: `{type: "setVariable",
variable, value}` sets a value, or with `expr` an expression computed when it runs;
`{type: "setVariableMode", collection, mode}` switches a collection's mode for the prototype (ids or
names; modes chosen on layers inside the screen still win); `{type: "conditional", blocks:
[{condition?, actions}]}` runs the first block whose condition holds (a block without one is the
"else"). Expressions are `{value: {type: number|string|boolean|color|alias, value}}` (`alias` reads
a variable) or `{op: {op, args}}` with `add` (text joins when a string is involved), `subtract`,
`multiply`, `divide`, `equals`, `notEqual`, `less`, `lessOrEqual`, `greater`, `greaterOrEqual`,
`and`, `or`, `not` and `negate`; `{inMode: {variable, mode}}` reads a variable's value in one mode
of its collection whichever mode is chosen, and `{mode: {collection}}` gives the name of the mode a
collection is in where the interaction runs (as text; it follows set variable mode actions).
Variables, collections and modes may be given by name; they are stored by id. Expressions can also
be written as text: `expression: "count + 1"`
on a set variable action, or `expression: "count >= 3 and not done"` on a conditional block, in
place of `expr` / `condition` (variables by name, `{name}` for any name; numbers, `"text"`,
`true` / `false`, `#RRGGBB`; `or` / `||`, `and` / `&&`, `not` / `!`, `==` (or `=`), `!=`, `<`,
`<=`, `>`, `>=`, `+` (joins text), `-`, `*`, `/` and brackets; `accent[Dark]` for a variable's
value in a mode, `mode(Theme) == "Dark"` for a collection's current mode, names with brackets or
spaces at their ends in quotes). Text expressions are checked: a
condition must give true or false and a set variable's expression a value the variable can hold,
otherwise the command fails with what's wrong (`Something is missing at the end`, `No variable
\`x\``, `Can't compare a number with text`). Single actions, also inside conditionals, are edited
with `proto.action.add {id?, index, path?, action}` (→ `{path}`), `proto.action.set {index, path,
action}`, `proto.action.remove {index, path}` and `proto.action.move {index, path, to}`, where
`index` is the interaction and `path` the action: `[i]` the interaction's action `i`, `[i, b, j]`
action `j` of block `b` of the conditional action `i`, and so on (for `add`, where it goes:
`[i, b, n]` appends to a block with `n` actions; default: after the last action). A conditional's
blocks: `proto.conditional.addBlock {index, path, else?, expression? | condition?}` adds an else if
(before the else; by default the first boolean variable equal to its value) or with `else: true`
the else, `proto.conditional.setCondition {index, path, block, expression | condition}` and
`proto.conditional.removeBlock {index, path, block}` (removing the if when only an else is left
removes the whole conditional). Every one is an undo step and applies to all `ids` given (layers
with the same interactions). Flows `proto.flow.add` / `rename` / `remove`,
and the device, its rotation and the background `proto.settings {device?, background?,
rotation?, color?, width?, height?, responsive?}` (`None`, `iPhone 16`, `iPhone 16 Pro Max`,
`Android Compact`, `iPad mini`, `MacBook Air`, `Desktop` or `Custom size`; `rotation` `portrait` or
`landscape`, which turns phones and tablets; `color` one of the device's colours by name, e.g.
`Black`, `White`, `Pink`, `Teal`, `Blue` for the iPhone 16 (an error lists them), `""` for the
default; `width` / `height` set the custom size's screen, 1–10000 points, and pick `Custom size`,
which is a bare screen without a body). On a device the presented frame is sized to the device's
screen: what doesn't fit is clipped, or scrolls when the frame's `overflow` allows it, while `fixed`
layers stay on screen (pinned to the edges their constraints name). With `responsive: true` (saved
in the file) the presented frame is instead resized to the device's screen, or without a device to
the presenter's window at 100%, and its constraints and auto layout respond as when it is resized
in the editor (an axis it hugs keeps hugging its content); the file's frames don't change. `proto.present {start?}` plays the prototype full-window and `proto.preview
{start?}` in a resizable pane over the canvas that follows edits live; `proto.closePreview` closes
it. `proto.presentOptions {scale?, deviceFrame?, hotspotHints?, inspect?}` sets how the
full-window presenter sizes the prototype (`fit` scales it down to fit, `fitWidth` fills the width,
`fill` covers the window, `actual` shows it at 100%; what overflows pans), whether it draws the
device, whether a click where nothing reacts briefly flashes the screen's hotspots (the layers with
interactions, on by default; also in the presenter's Options menu) and whether its Inspect panel
shows the size, colours and typography of the layer under the pointer (also the Inspect button in
its bar); the options are kept for later presentations (in the presenter Z cycles them and R
restarts). The player's state is
in `inspect_ui` (`presenter`, `preview`: screen, scroll, `scale`, `zoom`, `pan`, the screen's
`hotspots` and whether they are flashing, the `inspected` layer) when attached, and
`ui.set {presentScroll: [x, y]}` scrolls its screen (see [control-protocol.md](control-protocol.md)).
The presenter's Options menu offers Responsive next to the size options (without a device) or
Responsive / Fixed size (on one), the same `proto.settings {responsive}`; its flow picker lists each
flow with its description.

### Local libraries

A `.figcraft` file can be published as a library and used from other files (local files only: no
accounts, nothing on the network; the browser build has no file system, so there the commands that
read library files return an error).

- **Publish** (in the library file): `library.publish {description?, include?, exclude?}` publishes
  the file's components, component sets, styles and variable collections as a new version (by
  default everything except names starting with `.` or `_`, or what the last publish offered;
  `include`/`exclude` take component ids and style or collection ids or names). A collection comes
  with its variables (values per mode, aliases, scopes), except those hidden from publishing
  (`variable.update {variable, hiddenFromPublishing: true}`). Components, styles and variables that
  published assets use (a component's bound variables, a variable's alias targets) come along,
  unoffered. Each asset keeps the version it last changed in; publishing with no changes is refused.
  Save the file (`file.save`) so other files see the new version.
- **Enable**: `library.add {path}` / `library.remove {id}` (id, name or path) / `library.list
  {assets?}` (versions, pending updates, errors; with `assets: true` each library's components,
  styles, variable collections (with their modes) and variables, and what this file can publish). Libraries are read read-only with size limits, and read
  again when the file changes.
- **Use**: `library.place {library, component, x?, y?}` (a component, set or variant by id or name;
  page point, default the view's centre) copies the component, with the components and styles it
  uses, into the file as a read-only main component and places an instance; `library.import
  {library, component}` only copies it (then `instance.create`, `instance.swap`…);
  `library.swapAt {library, component, x, y}` copies it and swaps the instance at that page point to
  it (as `instance.swapAt`, below).
  `library.applyStyle {library, style, ids?, target?}` copies a style in (linked, read-only) and
  applies it. Instances of library components behave like any instance: overrides, properties,
  variants, swap, reset, detach. Library components keep their variable bindings and explicit modes
  (bound to copies of the library's variables).
- **Library variables**: `library.bindVariable {library, variable, field, ids?}` copies a published
  variable in (linked, read-only, with the variables it aliases and its collection) and binds it
  like `variable.bind`; `library.aliasVariable {library, alias, variable, mode?}` makes a local
  variable's value an alias of a library variable; `library.setVariableMode {library, collection,
  mode, ids?}` sets a library collection's mode on frames (the page when no ids are given; the
Design panel's Variable modes section lists enabled libraries' collections under the library's
name and sets them this way);
  `library.importVariable {library, variable}` only copies it (then `variable.bind`, `variable.set
  {value: {alias}}`). The copies render without the library file. Editing the copies (library
  components, styles, variables or collections) is refused; edit the library file and publish.
- **Update**: `library.updates` lists the components, styles, variable collections and variables
  with a newer published version; `library.acceptUpdates {ids? (library ids)}` pulls them in,
  keeping instance overrides; bound layers, aliases and explicit modes follow the updated values.
- **Paste between files**: instances copied from another open file (`edit.copy` there, `edit.paste`
  here) follow their mains: library components join (or bring along) this file's copies of the
  same library, components the other file publishes as a library are used from it (and that
  library is enabled when the other file is saved), and other mains stay behind: the instance shows
  a missing main (`component.restoreMain` makes it a main here).

### Swapping instances

- `instance.swap {ids?, component}` shows another component in the instances (overrides on layers
  with matching names stay; property values with the same names too). Nested instances can be
  swapped on their own (an override of the instance they're in).
- `instance.swapAt {component, x, y}` swaps the instance at a page point: of the instances under
  it (innermost first), the first showing a variant of the same component set, else the innermost
  (never one inside the dropped component). The Assets panel does this when a component is dropped
  onto an instance with Alt (Option) held, highlighting that instance while dragging.
- Instance swap properties offer **preferred values**: `component.addProperty {type:
  "instanceSwap", default, preferred? [component or set ids]}` and `component.editProperty {name,
  preferred}`; `component.properties` lists them, and the Design panel's swap menus show them first.

### Slots

A slot is a frame inside a main component whose instances take any content of their own (add,
remove and reorder layers there, unlike the rest of an instance). It is a component property of type
`slot` (`component.properties` lists it: on a main with its `layers`, on an instance with
`value: {layer, edited, layers}`).

- **Define** (on the main): `component.makeSlot {id (a frame inside a main component), name?,
  preferred? [component or set ids]}` (the property is named after the frame by default);
  `component.editProperty {name, preferred}`; `component.removeSlot {id}` (instances lose their own
  content in it). `component.addProperty {type: "slot", layer}` and `component.bindProperty {field:
  "slot"}` work too (one frame per component for a slot).
- **Fill** (on an instance): `instance.slotInsert {id (the slot layer, or the instance with `slot`
  (property name)), component, index?, replace?}` places an instance of a component in it; any
  structure command (`shape.*`, `node.reparent`, `edit.paste` with the slot selected, `edit.delete`,
  `node.reorder`, `instance.place` or a drag onto it) works inside it. The main's default content
  shows (and follows the main) until the slot is first edited; then the slot's layers are the
  instance's own (override group `slot`): kept across main updates, variant switches, copy/paste and
  detach, and instances placed there stand alone (their own overrides, properties and detach).
- **Reset**: `instance.resetSlot {id}` (or `component.resetOverride {id, group: "slot"}`, or
  `component.resetInstance`) brings back the main's default content.

`get_node` shows `"slot": "<property>"` on slot layers.

### Boolean groups

`bool.union` / `bool.subtract` / `bool.intersect` / `bool.exclude {ids?}` wrap layers in a new
boolean group, except: with one boolean group selected they set its operation (every operand follows
it again), and with layers inside boolean groups they set those operands' own operation. Each
operand above the bottom one combines with the result of the operands below it by its own
operation, or the group's (`node.set {operandOperation: union|subtract|intersect|exclude | null}`
on any operand, or `booleanOperation` on an operand that isn't itself a boolean group; on a group
`booleanOperation` is the group's). `get_node` shows `operandOperation` on operands that have one.
Operands stay editable layers: `select.children` (Enter) or a double-click selects them, `cmd: true`
clicks reach them directly, and moving, resizing or editing an operand's points (`vector.edit.*`)
updates the group's result on every pointer move, one undo step per gesture; a dragged operand
stays in its group. `bool.flatten` bakes the result. Only the operands from the edited one up are
recombined during a drag (`figcraft_render::boolean`; timed by `cargo xtask bench`).

### Shape builder

The selected shapes' outlines split the canvas into regions (areas covered by the same shapes).
`shapeBuilder.regions {ids?}` lists them: an SVG `path` in page coordinates, the `sources` covering
each one (top-most first), the `parts` covering it (`{id, stroke}`: a shape's fill, or its stroke's
outlined area; top-most first), `bounds` and a `point` inside. Visible strokes take part as their
outlined areas, so stroked lines and open paths split the regions they cross and a stroke's band is a
region of its own. `shapeBuilder.merge {ids?, regions?, points?}` makes one vector from the regions
(by index, or the regions under page points), styled like the top-most fill or stroke over the first
one (a stroke's region is filled with the stroke's paints); `shapeBuilder.remove {ids?, regions?,
points?}` deletes them. Either way every shape covering a picked region loses that area (it becomes
a vector, or goes when nothing is left); the strokes of the shapes involved become outlined vectors
just above them (`outlines` in the result; an open path is replaced by its outline), and each call
is one undo step. The tool is `tool.shapeBuilder` (Shift+M):
click or drag across regions with `pointer` to merge, add `alt` to remove; `shapeBuilder.mode
{mode: merge|remove}` swaps what a plain click does.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"shapeBuilder.regions","params":{"ids":[2,3]}}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"shapeBuilder.merge","params":{"ids":[2,3],"points":[[120,80],[160,80]]}}}}
```

### Importing SVG

`open_file {path}` opens an SVG as a new document; `run_command {command: "file.import", params:
{path | data + name, x?, y?, center?}}` adds it to the current page as a frame (top-left at `x, y`,
or centred there with `center: true`, as pasting does). It comes in editable: groups named from
their `id`, transforms baked into the layers, `<rect>`/`<circle>`/`<ellipse>`/`<line>` (and paths
tracing those outlines) as rectangles with their corner radius, ellipses and lines, everything else
as vectors; strokes with their width, dashes, caps, joins and miter limit; linear and radial
gradients with their exact handles (`gradientTransform`, `reflect`/`repeat` unrolled, a focal
point as the centre); patterns as image fills (fill, fit, crop or tile); clip paths as vector masks
and masks as luminance or alpha masks; shadow and blur filters (`feDropShadow`, the offset / blur /
flood / composite chains, colour-matrix shadows, inner shadows, `feMorphology` spread) as drop
shadows, inner shadows and layer blurs; opacity and `mix-blend-mode`; text as text layers (fonts
the SVG names when installed, anchors, letter spacing, styled `<tspan>` runs, stacked lines as one
layer); images as image fills, and SVG images imported in place. CSS, `<use>`/`<symbol>`, units
and `viewBox`/`preserveAspectRatio` are resolved. The result's `warnings` list what was left out
(filters that aren't shadows or blurs, rotated repeating patterns, a clip path's own clip path).
Linked files are never read, only data URIs; documents nested more than 512 elements deep are
refused.

### Importing REST API file JSON

The JSON returned by the public REST API's file endpoint (`GET /v1/files/:key`, best with
`geometry=paths` so vectors carry their outlines) and its nodes endpoint opens like any document:
`open_file {path}` (a new, untitled document; the JSON is never saved over) or
`run_command {command: "file.import", params: {path}}` (pages appended to the current document,
with their flows, prototype settings and variables).
The result's `warnings` list what was skipped or approximated (unsupported layer, paint and effect
types, video and gamepad triggers, expression functions other than a variable mode lookup over a
variable and one of its modes, bindings to library
variables, instances of components outside the file, which come in as frames). Pages, the layer
tree, geometry, paints, strokes, effects, corners, constraints, auto layout, layout grids, exports,
text with styled runs, vectors, components, component sets, instances and component properties are
mapped, and so is prototyping: `interactions` (on click, hover, press, drag, after delay, mouse
enter/leave/down/up and key triggers; navigate, overlays, back, scroll to, change to, open link,
set variable (values, variable references and expressions), set variable mode and conditional
(if / else if / else over expressions) actions; dissolve, smart animate, move, push and slide transitions with their easing,
custom bezier and spring curves included), flow starting points, scroll overflow and fixed/sticky
layers, and the prototype device. Image fills keep their `imageRef` and show a grey
placeholder unless the image files sit in an `images` folder next to the JSON, named by `imageRef`
(`images/<imageRef>.png`); nothing is fetched from the network.

Variables come from the local variables response (`GET /v1/files/:key/variables/local`):
collections with their modes (the default mode first), values per mode, aliases and scopes. Pass it
as `variables` (a path, or the JSON object or text) to `open_file`, `file.open` or `file.import`,
put it next to the file as `<name>.variables.json` or `variables.json`, or embed its `meta` in the
file JSON. Layers' `boundVariables` (sizes, gaps and padding, corner radii, opacity, stroke weight,
visibility, text, font properties, single solid fills and strokes) and `explicitVariableModes` then
link to them, so switching a mode with `variable.setMode` restyles the layers (`mode: null` sets a
frame back to Auto, inheriting its parent's mode).

### Spell check

While a text layer is being edited, the desktop app underlines misspelled words (the system's
checker on Windows and macOS, in the system's language; the headless server and the browser have
none, so nothing is flagged there). `text.spelling {on?}` switches it (saved in preferences;
`available` says whether a checker is installed). With text being edited,
`text.spellingSuggestions {index}` returns the word at char `index` with `misspelled` and up to five
`suggestions` (null between words), and `text.replaceWord {index, with}` replaces that word (part of
the editing session's undo step). `spelling.addWord {word}` never flags a word again (saved in
preferences) and `spelling.ignore {word}` stops flagging it until the app quits.

### Comments

Comment threads live in the file, pinned to a page point or to a layer (`anchor`: a node id, or
`true` for the innermost frame under the point) so they move with it; when the layer is deleted the
pin stays where it was, on the nearest surviving parent. Driven with `run_command`:
`comment.add {x, y, text, page?, anchor?}` (returns `{id, link}`), `comment.reply {id, text}`,
`comment.edit {id, text, reply?}` and `comment.delete {id, reply?}` (a message is the thread's
first one, or `reply`: an index into its replies; deleting the first message deletes the thread),
`comment.react {id, emoji, reply?, on?}` (toggles your reaction), `comment.resolve {id, resolved?}`,
`comment.move {id, x, y, anchor?}` (pins it to the frame under the new point unless `anchor: false`).
Everyone acts as the name from Preferences (`pref.userName {name}`, default "You") unless `author`
says otherwise; only a message's author can edit or delete it. `@Name` in a message mentions
someone (`comment.people` lists the names used in the file).

Read state is kept per name: a new message is unread for everyone but its author;
`comment.markRead {id? | link?, read?}` marks one thread (or, without `id`, all of them) read or
unread (not an undo step). `comment.list {page? | "all", filter? (all|open|resolved|mine|unread),
sort? (date|page), query?, frame?}` returns the threads with their position, `frame`/`frameName`,
`pageName`, `unread`, `link` and replies (with `edited` times and `reactions`). Links are
`figcraft://comment/<id>`: `comment.link {id}` returns one, `comment.open {id | link, view?}` goes
to its page, zooms to its frame (`fit`, the default; `center` keeps the zoom; `keep` doesn't move)
and opens it in the app, and `comment.next {id?, dir?, …list params}` steps to the next or previous
thread of a list the same way. `comment.zoom {ids}` zooms in on a group of pins.

### Version history

Named snapshots of the document, kept in the `.figcraft` file (they persist once the file is saved;
a new, renamed or deleted version marks the file as changed). Driven with `run_command`:
`file.saveVersion {name?, description?}` (returns `{id, index, name, description, time}`; untitled
versions are named "Version N"), `version.list` (oldest first; `time` is Unix seconds),
`version.restore {id | index}` (replaces the document with the version as one undo step; the file
name stays), `version.rename {id | index, name, description?}`, `version.delete {id | index}` and
`version.duplicate {id | index}` (opens the version as a new document tab).
`version.preview {id | index}` shows a version in place of the document, read-only: edits, undo and
gestures are refused until `version.restore` keeps it or `version.exitPreview` goes back. Saving,
closing or saving a version ends the preview first, and `version.list` marks the shown version
`previewing`. Indexes are validated
(a negative or out-of-range index is an error). A file keeps at most 100 versions.
`version.autosave` saves an automatic checkpoint (`auto: true`, named "Autosave") when the document
has unsaved changes since its last version; the app runs it every 30 minutes for each open document,
and a file keeps the newest 20 autosaves (user versions are never dropped for them). `file.versionHistory` returns the same list and opens the panel in the app.

### Collaboration changes

`collab.diff {against? (undo: the last undoable action, the default | saved: since the last
save)}` returns `{changes, count, bytes}`: the change batch a live session would send for that edit
(per-property `create` / `set` / `delete` and document-level changes, see
`docs/collab-protocol.md`) and its binary size. It changes nothing. In a live session, where undo
is per user (`edit.undo` / `edit.redo` take back only your own steps, see
`docs/collab-protocol.md`), only `saved` applies.

### Live sessions

People (and agents) edit one file together and see each other's cursors, selections and
viewports. Both the headless session and the attached app can host and join (the web build can
join but not host); each call takes in what happened since the last one and sends your edits (a
joined file opens as a new document).

- `collab.host {port?, bind?}` starts hosting the active document: `bind` is `"local"` (this
  machine, the default), `"lan"` (the local network) or an interface address; the port is 47900,
  or any free one when it's taken (`port` picks one, 0 any). Returns the session as
  `collab.peers` does, with `link` (`figcraft://join?host=…&file=…&token=…`) and `url`
  (`ws://…/?file=…&token=…`): both carry the session's secret, so share them only with people you
  invite. `collab.stop {}` ends it.
- `collab.join {link}` (any form: an app's invite, a `figcraft-server` share link, or the
  `ws(s)://…?file=…&token=…` URL) returns at once with `status: "connecting"`; poll
  `collab.peers` until `status` is `"live"`, or `"failed"` with a `reason` (a wrong token, a full
  session, a revoked link, an unreachable host). `collab.leave {}` leaves; the synced document
  stays open as your copy.
- `collab.publish {server, name?, key?}` puts the active document on a `figcraft-server`
  ([`server.md`](server.md)) and syncs it from there: `server` is the address
  `figcraft-server key` prints (`ws://host:47900/?key=…`; or give `key` separately), `name` the
  file's name (default: the document's). It returns at once with `status: "connecting"`; the
  document is read-only until the upload is done, then `collab.peers` has `published: {server,
  file, name, edit, view}` with the file's first edit link and view link (they carry their
  tokens: share them like passwords). Someone joining with the view link gets `access: "view"`:
  every edit on the synced document is refused with "View only: …" (the server refuses them too).
- `collab.peers {}` → `{available, canHost (false in the web build), status
  (off|connecting|live|reconnecting|failed), reason?,
  attempt?, role (host|guest), me, name, color, fileName, pages, link, url, address, network,
  access (edit|view), readOnly (the active document can't be edited now: view only, or being
  published), published (a file you published: {server, file, name, edit, view}),
  follow, syncing (the document arrived), document (the index of the synced open document),
  editing (the text layer you are typing in: {node, layer, caret, selection}, or null), peers: [{id,
  name, color, editing (the text layer they are typing in, same shape, or null), presence: {cursor:
  [page, x, y], selection: [ids], viewport: [page, x, y, w, h], editing_text: {node, caret,
  selection?}}}]}`. Carets and selections count characters; someone's typing syncs when they
  finish, so their caret may be past the end of your copy until then. Starting `text.edit.begin`
  in a layer someone else is typing in sets the message "<name> is editing this text too: the
  version finished last is kept" (both edits sync as whole values when each person finishes; the
  last one wins).
- `collab.cursor {x, y}` puts your cursor at a page position (omit both to hide it): an agent's
  pointer. Your selection, page and view go out by themselves when they change.
- `collab.follow {client}` follows that person's viewport; `collab.follow {}` stops (so does any
  pan or zoom of your own, or Escape).
- Styles, variables, collections, modes and comments you create on the session's document take
  ids from your own id space (`S:<n>`, `VariableID:<n>`, … with large numbers), so two people
  creating them at once never overwrite each other's; outside a session ids look as before (`S:1`,
  `VariableID:1`…).
- On the session's document every command (or `batch`, or gesture) is one step of your own undo
  history: `edit.undo` / `edit.redo` take back only your steps and leave what others changed since.
  An edit the host refuses (someone deleted the layer) is rolled back and the reason is the
  session's message. While the connection is reconnecting, editing goes on; the edits go out when
  it is back (the same client, catching up with what it missed).
- `collab.registerLinks {on?}` → `{on, registered}` makes the desktop app the opener of
  `figcraft://` invite links, or stops (no `on`: switch). Only where the app registers them itself
  (Windows, per user); elsewhere it reports the command unavailable. It also sets the
  Preferences checkbox. Starting the app with an invite link (`figcraft "figcraft://join?…"`)
  joins that session.

### Plugins

Sandboxed WebAssembly plugins act through the same commands (see [`plugins.md`](plugins.md) for the
manifest, the ABI and the Rust SDK). Driven with `run_command`: `plugin.import {path}` (a
`figcraft-plugin.json` or its folder) or `plugin.import {manifest, wasm}` (the manifest as an
object or JSON text and the module as base64, for this session only), `plugin.list`,
`plugin.remove {id}`, and `plugin.run {id, command?, params?, fuel?}`, which returns
`{plugin, command, output, logs, calls, fuel}`. A run is one undo step and is rolled back when it
fails; a plugin only reads the document unless its manifest asks for `write`, may be limited to
some commands, and can never touch files, other documents, the undo history, dialogs or other
plugins. A headless MCP session starts with no plugins: import them first. `plugins.runLast` runs
the last plugin command again.

### Dev Mode

- **Component code:** `inspect.code {id, language: react|swiftui|compose}` on a component, a
  component set or a variant writes the component as a reusable unit: React (TSX) with a props
  interface, SwiftUI with a `View` struct, Compose with a `@Composable` function. Its component
  properties are the parameters, with their defaults: variant properties become string unions
  (React) or enums, booleans become flags around the layers they show, text properties strings,
  instance swaps views (`React.ReactNode`, `AnyView`, a composable lambda) and slots optional
  content. Every variant of a set shares one definition; in React the other variants' differences
  become CSS rules on `data-*` attributes. On an instance (or a layer tree containing instances)
  each instance is a call passing only the values it changes, e.g. `<Button state="Hover"
  label="Buy" />`, `Button(state: .hover, label: "Buy")`, `Button(state = ButtonState.Hover, label =
  "Buy")`. Names are escaped and unique (keywords get a suffix, `default` → `defaultValue`; names
  that would shadow framework types get one too, `Text` → `TextComponent`). `inspect.componentCode
  {id?, language?}` returns the parts: `{component, name, set, properties: [{name, param, type,
  default, value? (instances)}], definition, usage? (instances)}`.
- **Annotations:** notes for developers on any layer, each with optional pinned properties that
  Dev Mode shows with their current values (and the style or variable behind them).
  `annotation.add {id?, label?, properties?}` (a note, pinned properties or both; returns `{id,
  index}`), `annotation.edit {id?, index, label?, properties?}` (`properties` replaces the list),
  `annotation.remove {id?, index?}` (no index: all of the layer's), all undoable, and
  `annotation.list {ids? | id? | page? ("all")}` → `{annotations: [{id, name, index, label,
  properties: [{property, label, value, token?, color?}]}]}` (`value` is null when the layer doesn't
  have that property). Pinnable ids: `width`, `height`, `top`, `left`, `rotation`, `opacity`,
  `cornerRadius`, `direction`, `gap`, `padding`, `alignment`, `fontFamily`, `fontStyle`,
  `fontSize`, `lineHeight`, `letterSpacing`, `textAlign`, `fill`, `stroke`, `strokeWeight`,
  `effects`. A layer holds at most 64 annotations; notes are capped at 5,000 characters. They are
  saved with the file (`get_node` lists them as `annotations`); instances don't inherit their
  main's annotations and keep their own across main edits. In the app the Inspect tab draws them
  as callouts beside their layers and edits them in its Annotations section.
- **Annotation categories:** every annotation can have a category, drawn as the colour of its
  callout: the defaults are Development, Interaction, Accessibility and Content, and a file can add
  its own. `annotation.add` / `annotation.edit` take `category` (an id or name; `null` clears it)
  and `annotation.list` returns `category: {id, name, color}` and filters with `{category: id |
  name | "none"}`. `annotation.categories {}` → `{categories: [{id, name, color, count}]}`;
  undoable `annotation.addCategory {name, color?}` → `{id}` (a new one takes the first unused
  colour), `annotation.editCategory {id, name?, color?}` (rename, recolour; ids stay) and
  `annotation.removeCategory {id}` (its annotations stay, uncategorised). Names are unique (ignoring
  case), up to 64 characters; at most 64 categories. Files keep the list only once it differs from
  the defaults (old files open with the defaults). In the app, with nothing selected, the Inspect
  tab filters the callouts by category and edits the categories (colour, name, delete, add); each
  note has a category menu.
- **Ready for dev:** frames at the top of a page or section, and sections, carry a handoff status
  (`node.set {devStatus: none|readyForDev|completed}`). `inspect.readyForDev {status?:
  readyForDev|completed|any, page?}` lists them across pages (or on one page) in page and layer
  order: `{items: [{id, name, page, status}]}`. The Inspect tab sets the status of the selected
  frames and, with nothing selected, lists them with a status filter (click one to go to it, or its
  focus button to open it in the focus view).
- **Compare changes:** when a layer marked Ready for dev first changes (it turns "Changed"), the
  file keeps a copy of it and everything in it as it was just before that edit (saved in the
  `.figcraft` file only while there is a change; resolving the change, marking it again, another
  status or deleting it drops the copy, so the next change compares with that state).
  `dev.compare {id?, show?}` → `{id, name, changed, snapshot, added, removed, modified, truncated,
  changes: [{id, name, type, change: added|removed|changed, depth, layers, properties?: [{section,
  property, before, after, text}]}]}`: layers matched by id, added and removed ones listed as a
  whole (`layers` counts what's inside), changed ones with their properties formatted like Inspect
  (`width 120 → 136`, `fill #FF0000 → #E03030`, `text “Welcome” → “Hello”`, `parent Badge → Screen`,
  `order 3 → 1`, `component Card → Card 2`, auto layout, typography, strokes, effects, corner radius,
  opacity, visibility, name), nested layers and instance contents included. `snapshot: false` means
  the frame changed before copies were kept (an older file). `show: true` opens the compare view in
  the app: before and now side by side (or overlaid with a slider), changed layers outlined (added
  green, changed orange, removed red), the list of changes and Resolve change.
- **Focus view:** `dev.focus {id? | direction: next|previous}` → `{id, name, page, index, count}`
  shows one top-level frame or section alone on the canvas (fitted, selected, other layers neither
  drawn nor hit), switching page when needed; `direction` steps through the ready-for-dev frames
  across pages, wrapping around. `dev.unfocus {}` ends it, as do Escape (once the frame or nothing
  is selected), leaving the page or deleting the frame. The app shows a bar with the frame's name
  and status, Compare changes, previous / next and exit; `inspect_ui` reports `devFocus`.

### Effects

`effect.add {ids?, type? | effect?}` adds an effect (`dropShadow`, `innerShadow`, `layerBlur`,
`backgroundBlur`, `noise`, `texture`, `glass`) with its defaults, or the full `effect` object;
`effect.set {ids?, index, effect}` replaces one. A layer paints its drop shadows first, then its
fills, inner shadows, children and strokes (strokes draw over inner shadows), with layer blur,
noise and texture over the result.

- **Blurs** (`layerBlur`, `backgroundBlur`) are uniform (`{"radius": 8}`) or progressive:
  `{"type": "layerBlur", "blurType": "progressive", "startRadius": 0, "radius": 24, "startOffset":
  [0.5, 0], "endOffset": [0.5, 1]}` ramps the radius linearly from `startRadius` at `startOffset` to
  `radius` at `endOffset`, points in the layer's unit box (0–1 across and down, like a gradient's
  handles; before the start and past the end the radius stays). Radii above 10,000 draw as
  10,000. In the app, the Design panel's blur row picks Uniform or Progressive, and while a
  progressive blur's settings are open its start and end points are handles on the canvas.
  Generated CSS leaves progressive blurs out (CSS blurs are uniform); Inspect lists the ramp.
- **Exports:** PNG, PDF and SVG draw the same pixels as the canvas. PDF and SVG keep layers vector
  and embed images for what vectors can't show: background blur and glass (from what lies under
  the layer: on the page in PDF, among the exported layers in SVG), layer blur, noise and texture
  in PDF; noise grains (an image over the vector layer), texture and progressive layer blur (the
  layer whole) in SVG, where uniform layer blur and shadows stay SVG filters.
- **PDF for print:** a bleed grows each page by that much on every side and shows what lies past
  the frame there (the frame's fills reach out to the bleed edge with gradients and images in
  place, its children are clipped to the bleed instead of the frame, nothing is drawn past it);
  a page of one frame is trimmed to the frame's box (its shadows go into the bleed). Crop marks
  (at the trim's corners) and bleed marks (at the bleed's) are 0.25 pt hairlines 18 pt long,
  starting 3 pt outside the bleed, in registration colour. MediaBox is the sheet, BleedBox the
  trim grown by the bleed, TrimBox the frame. CMYK converts every colour (fills, strokes, text,
  gradient stops, images and rasterised effects) from sRGB with `K = 1 − max(R, G, B)`,
  `C = (1 − R − K) / (1 − K)` (M and Y alike), deterministic and profile-free: the file is
  DeviceCMYK without an output intent, because embedding one needs a CMYK ICC profile and none
  ships with FigCraft. Soft masks stay RGB (they aren't printed) and transparency groups blend in
  RGB, as the PDF writer declares them.

### Paint, effect and layout grid lists

A layer's fills, strokes, effects and layout grids are lists kept **bottom first**: index 0 paints
first, the last one is on top (the Design panel lists them the other way round, topmost first).

- **Reorder:** `fill.move`, `stroke.move`, `effect.move` and `grid.move {ids?, from, to}` move one
  item (one undo step for every layer given; out-of-range or equal indices change nothing, negative
  or fractional ones are refused). The item keeps everything (an image fill its image, scale mode
  and adjustments, a gradient its stops and handles), and a paint, effect or grid style linked to
  the list is unlinked (the list no longer matches it). In the app, drag a row by its grip (the
  empty strip left of its fields); a line shows where it lands.
- **Variable bindings follow their items** when a list is reordered or an item removed
  (`fill.remove`, `stroke.remove`, `effect.remove`, `grid.remove`); a removed item's bindings go
  with it. Binding fields (`variable.bind {field}`): `fill` / `stroke` (the first paint's colour) or
  `fills/<i>/color` / `strokes/<i>/color`, `effects/<i>/<offsetX|offsetY|radius|startRadius|spread|color>`,
  `layoutGrids/<i>/<size|count|gutter|offset>` (number variables; count is kept to 1–1000, size and
  gutter are never negative). Moving a bound paint rewrites its key (`fill` becomes `fills/2/color`).
- **Instance properties:** `variable.bind {ids: [instance], field: "componentProperties/<name>",
  variable}` drives a boolean property from a boolean variable and a text property from a string
  variable (other types are refused); the value follows the variable's modes like any binding, the
  binding follows renames of the property, and deleting the property, resetting the instance or
  detaching it drops it. The instance's properties panel offers the same with its variable button.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"fill.move","params":{"ids":[2],"from":0,"to":2}}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"variable.bind","params":{"ids":[5],"field":"layoutGrids/0/gutter","variable":"gutter"}}}}
```

### Ruler guides

`guide.add {vertical, offset, frame? | page?}` (returns `{index}`), `guide.move {index, offset, …}`,
`guide.remove {index, …}` and `guide.clear {…}` edit the guides of the current page, another
`page`, or a `frame` (frames, components and instances). A frame's guides are in its own
coordinates: they move with the frame, span only it, are copied with it and snap the layers inside
it. On a rotated frame they run along its own axes, and the layers inside it snap to them (and to
their siblings and the frame's edges) along those axes, keeping whole pixels in the frame's
coordinates. In the app, a guide dragged out of a ruler and dropped on a frame becomes that frame's
guide, along the frame's axis nearer the ruler's.

### Measuring, the Scale tool, arc handles and selecting

- **Measure** (what Alt/Option-hover shows): `measure.distances {ids? (default the selection), to?
  (a layer; default the selection's parent)}` returns `{lines: [{x0, y0, x1, y1, label, dashed}]}`
  in page coordinates, between bounding boxes (several layers by their union, rotated ones by
  their bounds): one box inside the other measures to each edge, boxes apart measure the gaps
  (`dashed` lines extend a gap line to a box it passes beside), overlapping boxes measure between
  matching edges. `measure.padding {id? (an auto layout frame or a child)}` returns `{frame, lines}`:
  the frame's padding and the gaps between neighbouring children (what Shift+Alt-hover shows).
  With `pointer`, `alt: true` hovers measure (add `cmd` for nested layers, `shift` for padding) and
  `alt` drags show live distances to the neighbours; `inspect_ui` reports them under
  `tool.overlay` (`guides`, and `measure_box` for the box measured to).
- **Scale tool** (`tool.scale`, K): dragging a selection handle with `pointer` scales everything by
  one factor (sizes and positions whatever the constraints, strokes, effects, corner radii, auto
  layout spacing, min/max sizes, layout grids, guides, tiled images, text), one undo step per drag,
  about the anchor point `tool.scaleAnchor {anchor?: tl|t|tr|l|c|r|bl|b|br}` (or, when the anchor
  lies on the side being dragged, the opposite handle; `alt: true` scales from the centre), with the
  factor and pivot in `tool.overlay.scale` while dragging. Without
  dragging, `transform.scale {ids?, factor, cx?, cy?}` does the same numerically.
- **Arc handles**: with the Move tool, a single selected ellipse (at least 40 px on screen, unlocked)
  shows its arc handles, listed by `inspect_ui` under `tool.arcHandles` (`{id, sweep: [x, y],
  start, ratio}` in page coordinates; `start` once there is an arc, `ratio` once there is an arc or
  a ring). Drag them with `pointer`. **Sweep** starts inside the right edge. Dragged round the
  ellipse, it opens an arc: positive up from the right edge, negative down. It closes again back at
  the start. **Start** turns the arc and keeps its sweep. **Ratio** (the centre, or the inner edge
  halfway round a ring) drags out to make a ring, and into the gap to show the other part.
  `shift: true` snaps angles to 15° steps and the ratio to 5% steps. Each drag is one undo step
  and writes `node.set`'s `arcStart` / `arcSweep` (degrees) and `arcInner` (0–0.99). The Design
  panel's Arc fields (Start °, Sweep %, Ratio %) write the same keys; number variables bound to
  `arcSweep` and `arcInner` (`variable.bind`) hold percentages, as the fields show them, and ones
  bound to `arcStart` degrees.
  `tool.overlay.arc` holds the readout while dragging (`{handle, label: "Sweep 75%", at}`), and
  `tool.overlay.arc_hover` holds the handle under the pointer.
- **Select**: `select.layersAt {x, y}` lists every layer under a page point, front-most first with its
  depth (the context menu's Select layer); `select.set`, `select.children` / `select.parent` (Enter
  / Shift+Enter), `select.next` / `select.previous` (Tab / Shift+Tab). With `pointer`, `cmd: true`
  clicks select the nested layer, `cmd` drags from empty canvas select the nested layers they touch,
  and `shift` drags toggle what they touch.

Tool failures come back as results with `isError: true` and a message; protocol problems (unknown
tool, malformed JSON, wrong parameter types) are JSON-RPC errors. Nothing crashes the server.

## Resources

| URI | Content |
|---|---|
| `figcraft://document` | The active document (like `inspect_document` at depth 8). |
| `figcraft://commands` | The command catalog. |
| `figcraft://node/{id}` (template) | One layer with children to depth 2. |
| `figcraft://command/{id}` (template) | One command's docs, e.g. `figcraft://command/layout.add`. |

## Prompts

| Prompt | Arguments |
|---|---|
| `mobile_screen` | `screen` (required), `device` (default "iPhone 16"), `brand_color` |
| `design_system` | `name`, `primary` |
| `landing_page` | `product` (required), `width` (default 1440), `accent` |
| `prototype_flow` | `screens` (comma-separated), `transition` |

Each returns a step-by-step plan naming the exact tools and commands to use and how to verify.

## Examples

Create a card with auto layout and check it:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_node","arguments":{
  "type":"frame","name":"Card","x":100,"y":100,"width":320,
  "props":{"fill":"#FFFFFF","cornerRadius":16,"layout":"vertical","padding":24,"gap":12,"sizingV":"hug",
           "effects":[{"type":"dropShadow","color":"#00000026","offset":[0,4],"radius":16}]}}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"create_node","arguments":{
  "type":"text","parent":2,"text":"Ada Lovelace","props":{"fontSize":24,"fontStyle":"Bold"}}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_node","arguments":{"id":2,"depth":2}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"screenshot","arguments":{"ids":[2]}}}
```

A component with an instance whose text is overridden (ids are the ones earlier calls returned; read
the instance's text layer id with `get_node`):

```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"component.create","params":{"ids":[2]}}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"instance.create","params":{"component":2,"x":500,"y":100}}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"type_text","arguments":{"id":7,"text":"Grace Hopper"}}}
```

Align and distribute as one undo step:

```json
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"batch","arguments":{"label":"Tidy","commands":[
  {"command":"align.top","params":{"ids":[3,4,5]}},
  {"command":"distribute.horizontal","params":{"ids":[3,4,5]}}]}}}
```

The end-to-end tests in `crates/mcp/tests/agent_tasks.rs` are worked examples of ten realistic
tasks (auto layout, components and overrides, gradients and shadows, paint styles, Light/Dark
variables, align/distribute/group, boolean subtract + flatten, export, undo/redo, drawing with tools).
