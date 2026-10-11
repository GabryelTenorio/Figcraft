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
component/instance links, overrides, `slot` (a slot layer's property name), boundVariables, interactions, exports,
`devStatus` (handoff status), `devChanged` (changed since it was marked ready for dev),
`annotations` (Dev Mode notes: `[{label, properties, category?}]`). The auto layout props
that `node.set` accepts are listed in [mcp.md](mcp.md#conventions-agents-must-know).

### `ui.inspect {}`

UI state: panels and tabs, current tool (`tool`: its id, overlay hints and, for a selected
ellipse, its arc handles `arcHandles`; see [mcp.md](mcp.md#measuring-the-scale-tool-arc-handles-and-selecting)), canvas rect, dialogs, view, timings (`frameMs`,
`renderMs`), frames drawn since start or the last `ui.set {resetFrameStats: true}` (`framesDrawn`;
an idle window draws none) with the most frequent repaint requests behind them (`repaintCauses`:
`[{at: "file:line", count}]`, every request made in the frame before each one, delayed ones such as
the autosave deadline included; egui's `context.rs` begin-pass line means input events) and what the last canvas render had to do (`canvas.renderWork`: `full`, `partial` with
the `pixels` re-rendered after a pan or an edit, or `reused`). While a prototype plays, `presenter`
(full window) and `preview` (the preview pane) hold the player's state: the `current` frame and its
`name`, `start`, `history`, open `overlays`, the screen's `scroll` offset and `scrollRange`, the
`device` (and whether it is turned to `landscape`, its `deviceColor`) and the `viewport` of the frame
its screen shows, the presented frame's `frameSize` and, for a responsive prototype, the screen it
was resized to (`responsive`: `[w, h]`, else `null`), the size option (`scale`:
`fit|fitWidth|fill|actual`, `deviceFrame`), the `zoom` it shows at, how
far the view is panned (`pan`, `panRange`), whether a scroll is still coasting (`flinging`), the
active screen's `hotspots` (`[{id, rect: [x0, y0, x1, y1]}]` in window points, bottom-most first),
whether a click where nothing reacts flashes them (`hotspotHints`) and they are flashing now
(`hotspotFlash`), and whether the Inspect panel shows (`inspect`) with the layer it shows
(`inspected`) (`null` when nothing plays). `presentOptions` holds the options the next presentation
starts with (`scale`, `deviceFrame`, `comments`, `hotspotHints`, `inspect`).
`textPreview` is true while a live text preview shows on the canvas (a hovered text style or named
instance, a dragged axis slider; see `text.preview` in [mcp.md](mcp.md)).
`focus` is keyboard navigation's state: the `region` with the keyboard (`toolbar`, `layers`,
`canvas` or `properties`), whether `keyboard` mode is on (F6 or Tab moved the focus since the last
pointer press, so the focus ring shows) and the name of the `control` with the keyboard (what a
screen reader announces; empty when nothing has it). `reduceMotion` is true when the preference or
the system's accessibility setting turns the interface's animations off. `live` is the live
session as the window shows it: `status`, `role`, the number of `peers`, who you `follow`, the
peers whose cursors are drawn on the canvas (`cursorsShown`), whether the Join and Publish to
server dialogs are open (`joinDialog`, `publishDialog`; Escape closes them) and why the active
document can't be edited now (`readOnly`: the "View only" or "Publishing" reason, else null); the
session itself is `engine.execute {command: "collab.peers"}` (hosting, joining and publishing are
`collab.host`, `collab.join` and `collab.publish`, opening invite links `collab.registerLinks`,
see [mcp.md](mcp.md#live-sessions)). The app also joins the invite link it
was started with (`figcraft "figcraft://join?…"`).

### `ui.pointer {kind, x, y | sx, sy, to?, steps?, shift?, alt?, cmd?, ctrl?}`

A mouse gesture with the current tool. `kind`: `down`, `move`, `up`, `click`, `double`, `hover`,
`drag` (needs `to: [x, y]`; `steps` intermediate moves, default 8). Position in page coordinates
(`x`, `y`) or screen pixels (`sx`, `sy`). Returns `{selection, tool}`. With the Move and Scale
tools, a press on a frame's or section's name label (as the canvas last drew it, along a rotated
frame's top edge) selects and drags that frame.

```json
{"id":4,"method":"engine.execute","params":{"command":"tool.set","params":{"tool":"ellipse"}}}
{"id":5,"method":"ui.pointer","params":{"kind":"drag","x":200,"y":200,"to":[300,260],"shift":true}}
```

### `ui.key {key, shift?, alt?, cmd?, ctrl?}`

A key as if typed on the canvas (in vector edit mode the edit keys come first: `V` `P` `B` `Q` `X` `Shift+E` `Shift+W` tools, arrows nudge points, `Delete`/`Shift+Delete`, `Cmd+A`, `Cmd+J`; with the shape builder, `Enter` finishes): a menu shortcut (`{"key":"G","cmd":true}` groups), a tool key,
`Escape`, `Delete`/`Backspace`, `Enter`, number keys (opacity; two quick digits make an exact percentage), arrows (nudge; shift = big nudge; auto layout children move one place instead: along the flow, between wrapped rows, to the next grid cell, or into the neighbouring nested auto layout frame; with `cmd` they resize the selection). With the Comment
tool, `ArrowUp`/`ArrowDown` open the previous/next thread of the Comments panel's list instead
(returns `{"comment": id}`). While text is being edited, the text shortcuts (Bold, alignment,
size steps, Save…) run and other keys go to the text editor. Shortcuts follow the platform the app
runs on (`cmd` is Cmd on macOS and Ctrl elsewhere; on macOS `ctrl` is the separate Control key).
Returns `{"command": id}` for shortcuts.

### `ui.type {text}`

Type into the text being edited (start editing with a double-click, `text.edit.begin {id}`, or the
text tool).

### `ui.input {events}`

Raw window input for the next frame, as if from the mouse and keyboard: it reaches the panels,
popovers and fields (`ui.pointer` and `ui.key` drive the canvas tools and shortcuts instead).
`events` is a list of `{move: [x, y]}`, `{down: [x, y], button?: "secondary"}`,
`{up: [x, y], button?}`, `{key, shift?, alt?, cmd?}` (pressed and released; egui key names such as
`ArrowLeft`, `Escape`, `A`), `{text}` (typed text), `{scroll: [dx, dy], shift?, alt?, cmd?}` (a
mouse wheel at the pointer: the content moves by that many points, so a negative `dy` scrolls down)
`{leave: true}` (the pointer left the window) and `{hold: {alt?, shift?, cmd?}}` (keep those modifier
keys down for the frames that follow, e.g. Alt while dragging a component from the Assets panel onto
an instance to swap it; all false releases them); positions are window points. Events of one call
arrive in the same frame, so send a press and its release (or a double-click's two clicks) as
separate calls. Returns `{queued}`.

```json
{"id":6,"method":"ui.input","params":{"events":[{"move":[1000,315]}]}}
{"id":7,"method":"ui.input","params":{"events":[{"down":[1000,315]}]}}
{"id":8,"method":"ui.input","params":{"events":[{"up":[1000,315]}]}}
```

### `ui.menu.invoke {id}`

Invoke a menu item exactly like clicking it (this may open a dialog). Returns `{ok, lastError}`.

### `ui.set {rightTab?, leftTab?, theme?, home?, palette?, mainMenu?, focusRegion?, colorPicker?, variables?, layerFilter?, collapseLayers?, exportPreview?, preview?, presentScroll?, colorProfileDialog?, stylePicker?, stylePickerEdit?, stylePickerView?, iconGallery?, protoEditor?, contextMenu?, contextSelectLayer?, commentOpen?, commentDraft?, commentFilter?, commentSort?, commentSearch?, commentReply?, commentEdit?, presentComments?, devCompare?, devCompareMode?, devCompareSplit?, annotationFilter?, resetFrameStats?, pluginsWindow?, pluginForm?}`

Switch panels and modes: `rightTab` `design|prototype|inspect`, `leftTab` `file|assets`, `theme`,
`home` (bool), `palette` (bool: the Actions palette), `mainMenu` (bool, or a top-level menu such
as `"Plugins"` to open the main menu with it open), `pluginsWindow` (bool: Plugins ▸ Manage
plugins…), `pluginForm` (`{id, command?}`: start that plugin command like its menu item, opening
its parameters form when it has parameters; `null` closes the form), `focusRegion`
(`toolbar|layers|canvas|properties`: give that region the keyboard as F6 would, on the next frame),
`iconGallery` (bool:
every UI icon at 12, 16, 24 and 32 px over the window, for reviewing the icon set), `colorPicker`
(`{fills?, index?}`: open the colour picker on the selection's paint; `null` closes it), `variables` (bool,
or a collection id or name to show, or `{library, collection?}` for an enabled library's collection: the
Variables editor), `layerFilter` (string:
open the layers search with that filter; `null` closes it), `collapseLayers` (true: collapse the
layers tree), `exportPreview` (bool: the Design panel Export section's preview), `preview` (true or a
frame id: open the prototype preview pane like `proto.preview`; false or `null` closes it),
`presentScroll` (`[x, y]`: scroll the presented screen, the full window's else the pane's, clamped
to what it can scroll), `colorProfileDialog` (`srgb|displayP3`: open File ▸ Color profile with that
profile chosen; `null` closes it), `stylePicker` (`fill|stroke|effect|grid|text`: open the style
picker for the selection, `text` being the text style browser of the Typography section; `null`
closes it) with `stylePickerEdit` (a style id or name: open its editor), `stylePickerView`
(`list|grid`), `protoEditor` (`[layer id, interaction index]`: open that interaction's editor in the
Prototype panel; `null` closes it), `contextMenu` (`[sx, sy]`: right-click the canvas at that screen
point, selecting what is under it (with `cmd: true`, the nested layer) and opening the context menu;
`null` closes it), `contextSelectLayer` (bool: open the context menu's Select layer submenu, which
lists every layer under the click). Comments: `commentOpen` (a thread id opens its popover, `null`
closes it), `commentDraft` (`{x, y}`: start a new comment there), `commentFilter`
(`open|resolved|mine|unread|all`: what the Comments panel and the pins show), `commentSort`
(`date|page`), `commentSearch` (the panel's search text), `commentReply` (the open thread's reply
text, focused with the cursor at its end: an `@` there shows the mention picker), `commentEdit` (a
message index of the open thread to edit, 0 for the first; `null` stops), `presentComments` (bool:
the presenter shows the presented frame's comments, also Shift+C there). Dev Mode: `devCompare` (a
ready-for-dev frame or section id opens the Compare changes view, like `dev.compare {id, show:
true}`; `null` closes it) with `devCompareMode` (`sideBySide|overlay`) and `devCompareSplit` (the
overlay slider, 0–1), `annotationFilter` (`all`, `none` for uncategorised, or a category id or name:
the annotations the canvas shows). `resetFrameStats` (true: start `framesDrawn` and
`repaintCauses` over). Returns `ui.inspect` (which reports `stylePicker`,
`stylePickerView`, `comments`: `open`, `draft`, `filter`, `sort`, `search`, `editing` and the
`mentions` the picker offers, `devCompare` (`{id, mode, split}` or `null`), `devFocus` (the frame the
focus view shows, or `null`) and `annotationFilter`).

### `ui.screenshot {path}`

PNG of the whole window, UI included (needs a visible window; an idle one draws the frame to
capture). Returns `{path, width, height}`, or an error after 60 s when the window doesn't deliver.

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
