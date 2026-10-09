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
- Every mutation is one undo step (`batch` makes several one step). A failing command leaves the
  document as it was.
- Programmatic calls never open dialogs.
- Verify your work: `inspect_document` / `get_node` for numbers, `screenshot` for pixels.

## Tools

| Tool | What it does |
|---|---|
| `list_commands {filter?}` | Every command: id, label, menu, shortcut, undoable, params doc. |
| `run_command {command, params}` | Run any command (e.g. `align.left`, `layout.add`, `component.create`, `style.create`, `variable.bind`, `proto.interaction.add`). |
| `batch {commands: [{command, params}], label?}` | Several commands as one undo step (`command.batch`); all-or-nothing. |
| `create_node {type, x, y, width, height, parent, index, name, text, path, props}` | Create a frame, rectangle, ellipse, polygon, star, line, arrow, text, vector, section, component or slice; returns `{id, node}`. |
| `set_properties {ids, props}` | `node.set` on layers (geometry, fills, strokes, effects, corner radius, auto layout, sizing, constraints, text…); returns the updated layers. |
| `inspect_document {depth, allPages}` | The document as JSON (layer tree, styles, selection, history). |
| `get_node {id, depth}` | One layer as JSON. |
| `select {ids}` | Replace the selection (`[]` deselects). |
| `export {ids, format, scale, path?}` | png, jpg, svg, webp (pdf when available). Without `path`, images come back as MCP image content, SVG as text. |
| `screenshot {ids?, scale?, maxSize?, window?}` | PNG of the page (with background) or of layers, scaled to fit `maxSize` (default 1600). `window: true` captures the app window when attached. |
| `open_file {path}` / `save_file {path?}` | Open `.figcraft`, `.json`, `.svg` or an image; save as `.figcraft`. |
| `undo` / `redo` | Step through history. |
| `pointer {kind, x, y, to?, steps?, tool?, shift, alt, cmd}` | Mouse gestures in page coordinates with the current tool: `click`, `double`, `drag`, `down`, `move`, `up`, `hover`. |
| `key {key, shift, alt, cmd}` | A shortcut or key as if typed on the canvas (menu shortcuts, tool keys, Escape, Delete, arrows; text editing keys while editing text). |
| `type_text {text, id?}` | Type into the text being edited, or replace the contents of text layer `id`. |
| `inspect_ui {}` | Tool, view and status (and panels/timings when attached). |

### Editing vectors

Vector edit mode is driven with `run_command` (or `pointer`/`key` for gestures):
`vector.edit.begin {id}` (converts shapes to vectors), `vector.edit.state` (page-space overlay, the
local network and each point's handle mirroring), `vector.edit.tool {tool: move|pen|bend|paint|lasso|cut}`,
`vector.edit.select {vertices?, segments?, add?}`, `vector.edit.selectAll`, `vector.edit.moveVertices`,
`vector.edit.nudge`, `vector.edit.position {x?, y?}`, `vector.edit.setHandle`, `vector.edit.mirroring
{mode: none|angle|angleAndLength}`, `vector.edit.smooth {smooth?}`, `vector.edit.bend`,
`vector.edit.addVertex`, `vector.edit.connect {a, b}`, `vector.edit.pen {x, y, dx?, dy?}` (continues
from the pen's open end; clicking a point connects or branches), `vector.edit.penEnd`,
`vector.edit.paint {x, y}` (paint bucket: fills or unfills the closed area under a page point),
`vector.edit.cut {segment, t} | {vertex}`, `vector.edit.lasso {points}`, `vector.edit.delete`,
`vector.deleteHeal`, `vector.join` / `vector.smoothJoin` (join the selected points while editing),
`vector.edit.cornerRadius`, `vector.edit.end`. Point indices refer to `network.vertices` in
`vector.edit.state`; local coordinates are the layer's own.

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
