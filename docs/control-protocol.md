# FigCraft control protocol

The desktop app can be driven by other programs (tests, scripts, `figcraft-cli mcp --attach`,
sibling Crafting Apps) through a loopback socket:

```sh
cargo run -p figcraft -- --control 7979 [file.figcraft|file.svg|image]
```

The app listens on `127.0.0.1:<port>` only. Each connection carries **newline-delimited JSON**:
one request object per line, one reply line per request, in order.

```text
request:  {"id": 1, "method": "engine.execute", "params": {...}}
reply:    {"id": 1, "ok": true, "result": ...}
          {"id": 1, "ok": false, "error": "message"}
```

Requests are queued and run on the UI thread at the start of the next frame, so replies reflect the
real UI state. A request that the UI doesn't answer within 60 s gets a timeout error. A line that is
not JSON gets `{"id": null, "ok": false, "error": "bad JSON: …"}`. The implementation is in
`crates/ui-egui/src/control.rs` (methods) and `apps/figcraft/src/control_server.rs` (socket).

Coordinates are page units, y down. `x`/`y` in `node.set`/`node.create` are parent-relative.

## Methods

### `engine.execute {command, params?}`

Run any command (the same ids as the menus, shortcuts, Actions palette, CLI and MCP). Programmatic
calls never open dialogs: dialog requests are dropped. Returns the command's result (creation
commands return `{"id": N}`).

```json
{"id":1,"method":"engine.execute","params":{"command":"shape.rectangle","params":{"x":10,"y":10,"width":100,"height":50}}}
{"id":1,"ok":true,"result":{"id":2}}
{"id":2,"method":"engine.execute","params":{"command":"node.set","params":{"ids":[2],"props":{"fill":"#FF6B6B","cornerRadius":8}}}}
{"id":3,"method":"engine.execute","params":{"command":"command.batch","params":{"label":"Tidy","commands":[{"command":"align.left","params":{"ids":[2,3]}},{"command":"distribute.vertical","params":{"ids":[2,3]}}]}}}
```

### `engine.commands {}`

Every command: `[{id, label, params, menu, shortcut, undoable}]`.

### `document.inspect {depth?, allPages?}`

The active document as JSON: name, path, dirty, currentPage, selection, pages (the current page's
layer tree to `depth`, default 8; others summarised unless `allPages`), styles, variable count,
images, the colour profile (`colorProfile`: `srgb` or `displayP3`), the text being edited and the
undo history.

### `node.get {id, depth?}`

One layer (children to `depth`, default 1). Fields: id, name, type, x, y, width, height, rotation,
absoluteBounds, visible, locked, opacity, blend, fills, strokes, strokeWeight, effects, cornerRadius,
layout (including grid `columnTracks` / `rowTracks`), sizing, `minWidth` / `maxWidth` /
`minHeight` / `maxHeight`, `absolute`, constraints (children of plain frames and absolute
children), `gridCell` (a grid auto layout child's column, row and spans), clip, text properties,
component/instance links, overrides, `slot` (a slot layer's property name), boundVariables, interactions, exports. The auto layout props
that `node.set` accepts are listed in [mcp.md](mcp.md#conventions-agents-must-know).

### `ui.inspect {}`

UI state: panels and tabs, current tool, canvas rect, dialogs, view, timings (`frameMs`,
`renderMs`) and what the last canvas render had to do (`canvas.renderWork`: `full`, `partial` with
the `pixels` re-rendered after a pan or an edit, or `reused`). While a prototype plays, `presenter`
(full window) and `preview` (the preview pane) hold the player's state: the `current` frame and its
`name`, `start`, `history`, open `overlays`, the screen's `scroll` offset and `scrollRange`, the
`device` and the `viewport` of the frame its screen shows (`null` when nothing plays).

### `ui.pointer {kind, x, y | sx, sy, to?, steps?, shift?, alt?, cmd?, ctrl?}`

A mouse gesture with the current tool. `kind`: `down`, `move`, `up`, `click`, `double`, `hover`,
`drag` (needs `to: [x, y]`; `steps` intermediate moves, default 8). Position in page coordinates
(`x`, `y`) or screen pixels (`sx`, `sy`). Returns `{selection, tool}`.

```json
{"id":4,"method":"engine.execute","params":{"command":"tool.set","params":{"tool":"ellipse"}}}
{"id":5,"method":"ui.pointer","params":{"kind":"drag","x":200,"y":200,"to":[300,260],"shift":true}}
```

### `ui.key {key, shift?, alt?, cmd?, ctrl?}`

A key as if typed on the canvas (in vector edit mode the edit keys come first: `V` `P` `Q` `X` `Shift+B` `Shift+E` `Shift+W` tools, arrows nudge points, `Delete`/`Shift+Delete`, `Cmd+A`, `Cmd+J`; with the shape builder, `Enter` finishes): a menu shortcut (`{"key":"G","cmd":true}` groups), a tool key,
`Escape`, `Delete`/`Backspace`, `Enter`, arrows (nudge; shift = big nudge; auto layout children move one place along the flow; with `cmd` they resize the selection). While text is being
edited, keys go to the text editor. Returns `{"command": id}` for shortcuts.

### `ui.type {text}`

Type into the text being edited (start editing with a double-click, `text.edit.begin {id}`, or the
text tool).

### `ui.menu.invoke {id}`

Invoke a menu item exactly like clicking it (this may open a dialog). Returns `{ok, lastError}`.

### `ui.set {rightTab?, leftTab?, theme?, home?, palette?, mainMenu?, colorPicker?, variables?, layerFilter?, collapseLayers?, exportPreview?, preview?, presentScroll?, colorProfileDialog?, stylePicker?, stylePickerEdit?, stylePickerView?}`

Switch panels and modes: `rightTab` `design|prototype|inspect`, `leftTab` `file|assets`, `theme`,
`home` (bool), `palette` (bool: the Actions palette), `mainMenu` (bool), `colorPicker`
(`{fills?, index?}`: open the colour picker on the selection's paint; `null` closes it), `variables` (bool,
or a collection id or name to show: the Variables editor), `layerFilter` (string:
open the layers search with that filter; `null` closes it), `collapseLayers` (true: collapse the
layers tree), `exportPreview` (bool: the Design panel Export section's preview), `preview` (true or a
frame id: open the prototype preview pane like `proto.preview`; false or `null` closes it),
`presentScroll` (`[x, y]`: scroll the presented screen, the full window's else the pane's, clamped
to what it can scroll), `colorProfileDialog` (`srgb|displayP3`: open File ▸ Color profile with that
profile chosen; `null` closes it), `stylePicker` (`fill|stroke|effect|grid`: open the style picker
for the selection; `null` closes it) with `stylePickerEdit` (a style id or name: open its editor),
`stylePickerView` (`list|grid`). Returns `ui.inspect` (which reports `stylePicker` and
`stylePickerView`).

### `ui.screenshot {path}`

PNG of the whole window, UI included (needs a visible window). Returns `{path, width, height}`.

### `ui.render {path, scale?, ids?}`

PNG of the document without UI chrome (works without a window). Without `ids` it exports the
selection, else the page's top-level layers. Returns the export result.

### `app.quit {}`

Close the app.

## Example session (shell)

```sh
port=7979
figcraft --control $port &
exec 3<>/dev/tcp/127.0.0.1/$port
echo '{"id":1,"method":"engine.execute","params":{"command":"frame.create","params":{"preset":"iPhone 16","x":0,"y":0}}}' >&3
read -r reply <&3; echo "$reply"
echo '{"id":2,"method":"ui.screenshot","params":{"path":"/tmp/figcraft.png"}}' >&3
read -r reply <&3; echo "$reply"
```

For agents, prefer the MCP server (`figcraft-cli mcp --attach 7979`, see `docs/mcp.md`): it wraps
these methods in documented tools and returns images inline.
