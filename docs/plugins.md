# FigCraft plugins

A FigCraft plugin is a WebAssembly module with a small JSON manifest. It acts on the document only
through FigCraft's commands, the same ones the menus, the CLI, the control channel and the MCP
server run (`figcraft-cli commands` lists them), so a plugin can do what a script can do, within the
permissions its manifest asks for, and nothing more.

- **Sandboxed.** Plugins run in [wasmi](https://github.com/wasmi-labs/wasmi), a pure-Rust
  WebAssembly interpreter (no JIT), in the desktop app and in the browser build alike. There is no
  WASI: no file system, network, clock or randomness. A run has an instruction budget (fuel) and a
  memory cap, so an endless loop or a runaway allocation fails that run, not the app.
- **One undo step.** Everything a run does is one step in the undo history, named after the plugin.
  When a run fails (the plugin returns an error, traps, panics or runs out of fuel or memory), the
  document, selection and page are put back as they were.
- **Permissions.** The manifest says whether the plugin only reads the document or may edit it, and
  can narrow the commands it may call. FigCraft enforces both.

## Using plugins

- **Plugins ▸ Development ▸ Import plugin from manifest…** picks a `figcraft-plugin.json`. The
  plugin is kept in the preferences and its module is read again on every run, so a rebuilt
  development plugin runs its new code without importing it again.
- The installed plugins head the **Plugins** menu: a plugin with one command is an item, one with
  several a submenu. A command with parameters (an ellipsis after its name) first opens a small form
  with a field per parameter, drawn with FigCraft's own widgets; **Run** runs it.
- **Plugins ▸ Run last plugin** (⌥⌘P on macOS, Ctrl+Alt+Shift+P on Windows and Linux, where
  Ctrl+Alt+P is Pixel preview) runs the last plugin command again with the same parameters.
- **Plugins ▸ Manage plugins…** lists the installed plugins (version, access, manifest path) with a
  Remove button for each, and an Import button.

A message the plugin shows (`notify`) appears as a toast; so does a text result.

From scripts, the CLI and MCP the same commands apply:

```jsonl
{"command": "plugin.import", "params": {"path": "plugins/tidy-names"}}
{"command": "select.all", "params": {}}
{"command": "plugin.run", "params": {"id": "org.figcraft.examples.tidy-names", "command": "tidy", "params": {"case": "Title Case"}}}
```

`figcraft-cli run script.jsonl --in design.figcraft --out tidy.figcraft` runs that headless; over
MCP, `run_command` takes the same ids. A session started by the CLI or the MCP server starts with no
plugins: import them in the script.

| Command | Params | Does |
|---|---|---|
| `plugin.list` | `{}` | `{plugins: [{id, name, version, description, main, api, permissions, commands, path, loaded}], last: {id, command}}` |
| `plugin.import` | `{path}` or `{manifest, wasm}` | `path` is a `figcraft-plugin.json` or its folder. Without files (the browser, tests), pass the manifest (an object or JSON text) and the module as base64 `wasm`; such a plugin lasts for the session only. A plugin with the same id is replaced. The module is checked first. |
| `plugin.remove` | `{id}` | Uninstalls it. |
| `plugin.run` | `{id, command?, params?, fuel?}` | Runs a command (default: the first) with its parameters checked against the manifest and the missing ones filled with their defaults; `fuel` lowers the instruction budget. Returns `{plugin, command, output, logs, calls, fuel}`; `output` is the plugin's result, parsed as JSON when it is JSON. |
| `plugins.runLast` | `{}` | Runs the last plugin command again. |
| `plugins.manage` | `{}` | Opens the Plugins window; returns the list like `plugin.list`. |

## The manifest: `figcraft-plugin.json`

```json
{
  "id": "org.figcraft.examples.swatch-grid",
  "name": "Swatch grid",
  "version": "1.0.0",
  "description": "Makes a grid of colour swatches from a palette of hex colours.",
  "main": "swatch_grid.wasm",
  "api": 1,
  "permissions": {
    "access": "write",
    "commands": ["view.get", "shape.rectangle", "text.create", "select.set"]
  },
  "commands": [
    {
      "id": "grid",
      "label": "Swatch grid…",
      "params": [
        { "name": "palette", "label": "Palette", "type": "text", "default": "#FF5A5F, #00A699" },
        { "name": "columns", "label": "Columns", "type": "number", "default": 3, "min": 1, "max": 24 },
        { "name": "labels", "label": "Label each swatch", "type": "boolean", "default": true }
      ]
    }
  ]
}
```

| Field | |
|---|---|
| `id` | Required. 1–128 letters, digits, `.`, `-`, `_`; a reverse domain name is a good choice. |
| `name` | Required. Shown in the Plugins menu (64 characters at most). |
| `version`, `description` | Optional text. |
| `main` | Required. The module, a `.wasm` path relative to the manifest, inside its folder. |
| `api` | The plugin API version, `1` (the default). A manifest asking for a newer one is refused. |
| `permissions.access` | `read` (the default): commands that leave the document as it is. `write`: edits too. |
| `permissions.commands` | Optional allowlist: exact command ids or prefixes ending in `*` (`node.*`). Empty: any command the access allows. |
| `commands` | The menu commands, each `{id, label, params?}`. Without any, the plugin has one, `run`. |
| `params[]` | `{name, label?, type, default?, options?, min?, max?}`; `type` is `text`, `number` (clamped to `min`/`max`), `boolean` or `select` (one of `options`). |

## What a plugin can't do

Whatever its manifest says, FigCraft refuses these calls (the plugin gets the refusal as an error
it can handle):

- files and the app: `file.*`, `app.*`, `pref.*`, `help.*`, `library.add`, `libraries.*`,
  `image.place` with a `path` (pass the image as base64 `data` instead), `view.theme*`;
- other documents and the version history: `doc.*` and `version.*` (except `doc.list` and
  `version.list`);
- the undo history and nesting: `edit.undo`, `edit.redo`, `edit.repeat`, `command.batch` (a run is
  already one undo step), and `plugin.*` / `plugins.*` (no plugin runs another);
- commands that open a dialog or panel (for example `object.rename` without a `name`, which would
  start renaming in the layers panel): the call is undone and refused;
- for a `read` plugin, any command that changes the document (checked before the call for commands
  that make an undo step, and after it for the rest: a change is undone and refused).

Limits per run: 1,000,000,000 fuel (about a second or two of plain WebAssembly in a debug build,
far less in a release build), 64 MB of memory, 100,000 host calls (each also costs 10,000 fuel),
16 MB per string either way, 1,000 log lines, and modules up to 16 MB.

## ABI (version 1)

Strings are UTF-8 `(ptr, len)` pairs in the plugin's linear memory; all values are `i32`.

The module **exports** `memory` and `figcraft_run(input_len) -> status`. When `figcraft_run` starts,
the reply buffer holds the input JSON, `input_len` bytes:

```json
{"api": 1, "command": "grid", "params": {"palette": "#FF5A5F", "columns": 3, "labels": true}, "selection": [12, 15]}
```

It returns `0` for success; anything else fails the run (and rolls it back), with the text passed
to `set_output` as the error message.

The module may **import** these functions from the module `figcraft` (anything else, WASI
included, is refused when the plugin is imported):

| Import | Signature | Does |
|---|---|---|
| `call` | `(cmd_ptr, cmd_len, params_ptr, params_len) -> status` | Runs a command with JSON params (empty: `{}`). `0`: done, the reply holds its result JSON. `1`: refused or failed, the reply holds the message. |
| `selection` | `() -> len` | The reply becomes the selected layer ids, a JSON array. |
| `reply_len` | `() -> len` | The reply's length in bytes. |
| `reply_read` | `(ptr, len) -> copied` | Copies up to `len` bytes of the reply to `ptr`. |
| `log` | `(ptr, len)` | A line of the run's log (returned in `logs`, and written to FigCraft's log). |
| `notify` | `(ptr, len)` | A short message for the user (a toast in the app). |
| `set_output` | `(ptr, len)` | The run's result (JSON or text), or its error message when it fails. |

A pointer or length outside the plugin's memory traps, which fails the run.

## Writing a plugin in Rust

`plugins/` in the repository is a workspace of its own: `plugins/sdk` (the crate
`figcraft-plugin-sdk`, `no_std` with `alloc`) wraps the ABI with serde_json, and
`plugins/examples` has two plugins:

- **Tidy names** (`tidy-names`): cleans up the selected layers' names (extra spaces, the "copy" left
  by duplicating) and optionally their letter case, inside the selection too. Two commands: one
  with a form (case, include layers inside), and a quick one without.
- **Swatch grid** (`swatch-grid`): a grid of colour swatches from a palette of hex colours, centred in
  the view, named after their colours and optionally labelled.

`cargo xtask plugins` (part of `cargo xtask wasm` and `cargo xtask ci`; it needs the
`wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`) checks, tests and
builds them, and leaves each example's manifest and module side by side in
`target/plugins/dist/<example>/`, ready to import.

A plugin of your own:

```toml
# Cargo.toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
figcraft-plugin-sdk = { path = "../figcraft/plugins/sdk" }

[profile.release]
opt-level = "z"
lto = true
panic = "abort"
strip = true
```

```rust
// src/lib.rs
use figcraft_plugin_sdk::{Input, Result, Value, call, entry, json, notify};

fn run(input: Input) -> Result<Value> {
    if input.selection.is_empty() {
        return Err("Select some layers first".into());
    }
    for id in &input.selection {
        let layer = call("node.get", &json!({"id": id}))?;
        let name = layer["name"].as_str().unwrap_or_default().to_uppercase();
        call("object.rename", &json!({"ids": [id], "name": name}))?;
    }
    notify(&format!("Renamed {} layers", input.selection.len()));
    Ok(json!({"renamed": input.selection.len()}))
}

entry!(run);
```

Build it with `cargo build --release --target wasm32-unknown-unknown`, put
`target/wasm32-unknown-unknown/release/my_plugin.wasm` next to a `figcraft-plugin.json` whose `main`
names it, and import the manifest. The SDK gives `call`, `selection`, `log`, `notify` and the
`Input` (`command`, `params` with `str`/`f64`/`bool` helpers, `selection`); returning `Err` fails
the run and rolls it back. Outside WebAssembly every host call fails, so the plugin's own logic can
be unit-tested with `cargo test`.

Any language that compiles to WebAssembly works the same way: export `memory` and `figcraft_run`,
import what you need from `figcraft`.

## Not yet

- Plugin-defined UI: a plugin can't yet return a form or panel of its own at run time; parameters
  come from the manifest.
- Storage of the plugin's own: plugins can keep data on layers with `node.set {props: {pluginData:
  {key: value}}}`, but have no settings of their own between runs.
- Distribution: no plugin browser, signing or automatic updates; plugins are imported from a
  manifest.
