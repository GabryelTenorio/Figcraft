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
- Every mutation is one undo step (`batch` makes several one step). A failing command leaves the
  document as it was.
- Programmatic calls never open dialogs.
- Verify your work: `inspect_document` / `get_node` for numbers, `screenshot` for pixels.

## Tools

| Tool | What it does |
|---|---|
| `list_commands {filter?}` | Every command: id, label, menu, shortcut, undoable, params doc. |
| `run_command {command, params}` | Run any command (e.g. `align.left`, `layout.add`, `component.create`, `instance.place` (drop an instance at a page point into the frame or auto layout slot under it), `component.exposeInstance` / `instance.exposed` (nested instance properties on instances), `component.makeSlot` / `instance.slotInsert` / `instance.resetSlot` (slots, below), `inspect.code` (CSS, HTML + CSS for a whole frame, SwiftUI or Jetpack Compose for a layer), `inspect.properties` (layout, auto layout, typography, fills, strokes and effects with their style and variable names), `inspect.assets` (layers under the selection with export settings), `style.create`, `variable.bind`, `variable.update` (rename, describe, scope), `variable.set` with `{alias}`, `variable.renameMode` / `variable.removeMode`, `variable.deleteCollection`, `proto.interaction.add`). |
| `batch {commands: [{command, params}], label?}` | Several commands as one undo step (`command.batch`); all-or-nothing. |
| `create_node {type, x, y, width, height, parent, index, name, text, path, props}` | Create a frame, rectangle, ellipse, polygon, star, line, arrow, text, vector, section, component or slice; returns `{id, node}`. |
| `set_properties {ids, props}` | `node.set` on layers (geometry, fills, strokes, effects, corner radius, auto layout, sizing, constraints, masks (`isMask`, `maskType: alpha\|vector\|luminance`), handoff status (`devStatus: none\|readyForDev\|completed`), text…); returns the updated layers. |
| `inspect_document {depth, allPages}` | The document as JSON (layer tree, styles, selection, history). |
| `get_node {id, depth}` | One layer as JSON. |
| `select {ids}` | Replace the selection (`[]` deselects). |
| `export {ids, format, scale \| size, path?}` | png, jpg, svg, webp, pdf; `size` is `2x`, `512w` or `512h`. PDF: frames one page each (sized in points), anything else together on one page; `run_command` `file.exportFramesPdf {page?, path?}` writes every top-level frame of a page. Without `path`, images come back as MCP image content, SVG as text. Layers' own export settings: `run_command` with `export.add` / `export.update {index, size?, suffix?, format?}` / `export.remove` and `file.exportLayers {ids?, dir?}` (every layer once per setting, named `<layer><suffix>.<format>`). |
| `screenshot {ids?, scale?, maxSize?, window?}` | PNG of the page (with background) or of layers, scaled to fit `maxSize` (default 1600). `window: true` captures the app window when attached. |
| `open_file {path, variables?}` / `save_file {path?}` | Open `.figcraft`, `.json` (FigCraft JSON or REST API file JSON with its variables, see below), `.svg` or an image; save as `.figcraft`. |
| `undo` / `redo` | Step through history. |
| `pointer {kind, x, y, to?, steps?, tool?, shift, alt, cmd}` | Mouse gestures in page coordinates with the current tool: `click`, `double`, `drag`, `down`, `move`, `up`, `hover`. |
| `key {key, shift, alt, cmd}` | A shortcut or key as if typed on the canvas (menu shortcuts, tool keys, Escape, Delete, arrows; text editing keys while editing text). |
| `type_text {text, id?}` | Type into the text being edited, or replace the contents of text layer `id`. |
| `inspect_ui {}` | Tool, view and status (and panels/timings when attached). |

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
`and`, `or`, `not` and `negate`. Flows `proto.flow.add` / `rename` / `remove`,
and the device, its rotation and the background `proto.settings {device?, background?,
rotation?}` (`None`, `iPhone 16`, `iPhone 16 Pro Max`, `Android Compact`, `iPad mini`, `MacBook
Air`, `Desktop`; `rotation` `portrait` or `landscape`, which turns phones and tablets). On a device the
presented frame is sized to the device's screen: what doesn't fit is clipped, or scrolls when the
frame's `overflow` allows it, while `fixed` layers stay on screen (pinned to the edges their
constraints name). `proto.present {start?}` plays the prototype full-window and `proto.preview
{start?}` in a resizable pane over the canvas that follows edits live; `proto.closePreview` closes
it. `proto.presentOptions {scale?, deviceFrame?}` sets how the full-window presenter sizes the
prototype (`fit` scales it down to fit, `fitWidth` fills the width, `fill` covers the window,
`actual` shows it at 100%; what overflows pans) and whether it draws the device; the options are
kept for later presentations (in the presenter Z cycles them and R restarts). The player's state is
in `inspect_ui` (`presenter`, `preview`: screen, scroll, `scale`, `zoom`, `pan`) when attached, and
`ui.set {presentScroll: [x, y]}` scrolls its screen (see [control-protocol.md](control-protocol.md)).

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
  {library, component}` only copies it (then `instance.create`, `instance.swap`…).
  `library.applyStyle {library, style, ids?, target?}` copies a style in (linked, read-only) and
  applies it. Instances of library components behave like any instance: overrides, properties,
  variants, swap, reset, detach. Library components keep their variable bindings and explicit modes
  (bound to copies of the library's variables).
- **Library variables**: `library.bindVariable {library, variable, field, ids?}` copies a published
  variable in (linked, read-only, with the variables it aliases and its collection) and binds it
  like `variable.bind`; `library.aliasVariable {library, alias, variable, mode?}` makes a local
  variable's value an alias of a library variable; `library.setVariableMode {library, collection,
  mode, ids?}` sets a library collection's mode on frames (the page when no ids are given);
  `library.importVariable {library, variable}` only copies it (then `variable.bind`, `variable.set
  {value: {alias}}`). The copies render without the library file. Editing the copies (library
  components, styles, variables or collections) is refused; edit the library file and publish.
- **Update**: `library.updates` lists the components, styles, variable collections and variables
  with a newer published version; `library.acceptUpdates {ids? (library ids)}` pulls them in,
  keeping instance overrides; bound layers, aliases and explicit modes follow the updated values.

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

### Shape builder

The selected shapes' outlines split the canvas into regions (areas covered by the same shapes).
`shapeBuilder.regions {ids?}` lists them: an SVG `path` in page coordinates, the `sources` covering
each one (top-most first), `bounds` and a `point` inside. `shapeBuilder.merge {ids?, regions?,
points?}` makes one vector from the regions (by index, or the regions under page points), styled
like the top-most shape over the first one; `shapeBuilder.remove {ids?, regions?, points?}` deletes
them. Either way every shape covering a picked region loses that area (it becomes a vector, or goes
when nothing is left), and each call is one undo step. The tool is `tool.shapeBuilder` (Shift+M):
click or drag across regions with `pointer` to merge, add `alt` to remove; `shapeBuilder.mode
{mode: merge|remove}` swaps what a plain click does.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"shapeBuilder.regions","params":{"ids":[2,3]}}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_command","arguments":{"command":"shapeBuilder.merge","params":{"ids":[2,3],"points":[[120,80],[160,80]]}}}}
```

### Importing REST API file JSON

The JSON returned by the public REST API's file endpoint (`GET /v1/files/:key`, best with
`geometry=paths` so vectors carry their outlines) and its nodes endpoint opens like any document:
`open_file {path}` (a new, untitled document; the JSON is never saved over) or
`run_command {command: "file.import", params: {path}}` (pages appended to the current document,
with their flows, prototype settings and variables).
The result's `warnings` list what was skipped or approximated (unsupported layer, paint and effect
types, video and gamepad triggers, variable mode lookups in expressions, bindings to library
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
