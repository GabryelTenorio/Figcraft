//! FigCraft plugins: the manifest (`figcraft-plugin.json`) and the WebAssembly sandbox a plugin
//! runs in. The engine decides what a plugin may do (its permissions, which commands it may call,
//! one undo step per run); this crate only runs modules safely: a metered interpreter with a memory
//! cap, no WASI and a handful of host functions. See `docs/plugins.md`.

pub mod manifest;
mod sandbox;

pub use manifest::{API_VERSION, Access, MANIFEST_FILE, Manifest, Param, ParamKind, Permissions, PluginCommand};
pub use sandbox::{ENTRY, Host, IMPORT_MODULE, Limits, PluginError, RunOutput, check_module, run};

#[cfg(test)]
mod tests;
