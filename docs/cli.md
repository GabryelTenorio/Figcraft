# figcraft-cli

FigCraft without a window: inspect, export, convert and render documents, run command scripts, list
commands, serve MCP and measure performance. Build it with
`cargo build --release -p figcraft-cli` (binary `target/release/figcraft-cli`).

Bad arguments print usage and exit with code 2; failures exit with 1. The CLI never panics on input.

## Subcommands

### `info <file> [--json]`

Pages, node counts, top-level frames (id, type, name, size), style/variable/image counts. Repairs
of a damaged file and import warnings go to stderr as `note:` lines (`notes` with `--json`).

Every subcommand that reads a file also takes the public REST API file JSON (the response of
`GET /v1/files/:key` or of its nodes endpoint, written to a `.json` file): it is imported on the fly.
Image fills come from `images/<imageRef>.png` (or `.jpg`, …) next to the JSON when present, else a
grey placeholder.

```text
$ figcraft-cli info app.figcraft
app.figcraft — App (412 nodes, 6 styles, 12 variables, 3 images)
page 1 "Screens": 380 nodes, 4 top-level
  2 FRAME "Home" 393.0×852.0
  …
```

### `export <file> -o <out> [--format png|jpg|svg|webp|pdf] [--scale N] [--page NAME] [--node ID|NAME]`

- `<out>` is a directory: one file per top-level frame of the page (default the first page), named
  after the frame (`Home.png`, `Settings.png`…); with no frames, one file per top-level layer.
- `<out>` has an image extension: one file with the given `--node`, or the whole page composited.
- `--node` takes a layer id (`12` or `0:12`) or a layer name.
- The format comes from `--format`, else `<out>`'s extension, else png. `--scale` (default 1, max 64)
  applies to raster formats.

```sh
figcraft-cli export app.figcraft -o exports/ --scale 2
figcraft-cli export app.figcraft -o hero.svg --node "Hero"
```

### `convert <in> <out>`

By extension. Inputs: `.figcraft`, `.json` (FigCraft or REST API file JSON), `.svg` (becomes vectors), `.png/.jpg/.webp/.gif/.bmp`
(an image layer). Outputs: `.figcraft`, `.json` (FigCraft JSON, pretty-printed), `.svg`,
`.png/.jpg/.webp` (the current page rendered at 1x with its background), `.pdf` (when PDF export is
available).

### `render <file> -o <out.png> [--scale N] [--page NAME]`

The whole page (default the first) with its background, as png, jpg or webp. Large pages are scaled
down to fit 16384 px.

### `run <script.json|script.jsonl> [--in <file>] [--out <file>]`

Runs commands in a headless session (a new document, or `--in`), printing one JSON line per step:

```text
{"command":"frame.create","ok":true,"result":{"id":2},"step":0}
```

A failing step prints `"ok": false` with the error and exits with code 1 (nothing is saved).
`--out` saves the result by extension like `convert`.

Script formats: a JSON array of `{"command": id, "params": {...}}`, an object
`{"commands": [...]}`, or JSON lines (one step per line; blank lines and `//` lines are skipped).

```json
[
  {"command": "frame.create", "params": {"preset": "iPhone 16", "x": 0, "y": 0, "name": "Home"}},
  {"command": "node.create", "params": {"type": "text", "parent": 2, "x": 24, "y": 64, "text": "Welcome", "props": {"fontSize": 32}}},
  {"command": "layout.add", "params": {"ids": [2], "direction": "vertical"}}
]
```

Ids are assigned in order (a new document's first page is 1, so the first layer is 2), which lets a
script refer to layers it created.

### `commands [--json] [--filter TEXT]`

Every command with its label, shortcut, undoable flag (`*`) and parameter docs; `--json` for
machines.

### `mcp [--attach PORT]`

Serve MCP on stdio: headless by default, or attached to a running app started with
`figcraft --control PORT`. See [mcp.md](mcp.md).

### `perf [--layers N[,N...]]`

Builds synthetic documents of each size (default `1000,10000` layers: screens with an auto layout
header, a card group, button instances and a grid of shapes and text) and prints a Markdown table of
the hot paths: build + normalise, save/load, 1440×900 renders at 100% and zoomed to fit, hit testing
and hover, single-layer edits through the engine (fill, move, text in auto layout, a main component
with all its instances), undo/redo, a drag preview frame, `normalize` after a one-node edit,
`document.inspect` and select-all + move. Use a release build for meaningful numbers;
`cargo xtask bench` runs it at 1k/10k/50k layers together with the UI frame benchmarks.
