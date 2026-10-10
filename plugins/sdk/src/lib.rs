//! Write FigCraft plugins in Rust.
//!
//! A plugin is a `cdylib` built for `wasm32-unknown-unknown`. It gets the command the user picked
//! with its parameters and the selection, and acts on the document by calling FigCraft commands
//! (the ids `figcraft-cli commands` lists) with JSON parameters. Everything one run does is one
//! undo step; when the run returns an error (or panics), the document is put back.
//!
//! ```ignore
//! use figcraft_plugin_sdk::{Input, Result, Value, call, entry, json};
//!
//! fn run(input: Input) -> Result<Value> {
//!     for id in &input.selection {
//!         call("object.rename", &json!({"ids": [id], "name": "Tidy"}))?;
//!     }
//!     Ok(json!(format!("Renamed {} layers", input.selection.len())))
//! }
//!
//! entry!(run);
//! ```
//!
//! The SDK is `no_std` (it needs `alloc`); outside WebAssembly every host call fails, so a plugin's
//! own logic can still be unit-tested on the host. See `docs/plugins.md` for the ABI underneath.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub use serde_json::{self, Value, json};

/// What a plugin function returns: its output (shown or returned to the caller), or an error that
/// fails the run and rolls the document back.
pub type Result<T> = core::result::Result<T, String>;

/// The run's input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Input {
    /// The manifest command the user picked.
    pub command: String,
    /// Its parameters, with defaults filled in.
    pub params: Value,
    /// The selected layer ids.
    pub selection: Vec<u64>,
}

impl Input {
    /// Parse the input JSON FigCraft hands to the plugin.
    pub fn parse(text: &str) -> Result<Input> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("input: {e}"))?;
        Ok(Input {
            command: v.get("command").and_then(Value::as_str).unwrap_or_default().to_string(),
            params: v.get("params").cloned().unwrap_or(Value::Null),
            selection: ids(v.get("selection")),
        })
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.params.get(name).and_then(Value::as_str)
    }

    pub fn f64(&self, name: &str) -> Option<f64> {
        self.params.get(name).and_then(Value::as_f64).filter(|v| v.is_finite())
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.params.get(name).and_then(Value::as_bool)
    }
}

/// Layer ids in a JSON array.
fn ids(v: Option<&Value>) -> Vec<u64> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

/// Run FigCraft command `command` with `params`: its result, or why it was refused or failed.
pub fn call(command: &str, params: &Value) -> Result<Value> {
    let params = serde_json::to_string(params).map_err(|e| e.to_string())?;
    let status = host::call(command.as_bytes(), params.as_bytes());
    let reply = String::from_utf8(host::reply()).map_err(|_| "the reply isn't UTF-8".to_string())?;
    if status != 0 {
        return Err(reply);
    }
    serde_json::from_str(&reply).map_err(|e| format!("`{command}` returned {e}"))
}

/// The selected layer ids now (the input has them as they were when the run started).
pub fn selection() -> Vec<u64> {
    host::selection();
    let reply = host::reply();
    ids(serde_json::from_slice::<Value>(&reply).ok().as_ref())
}

/// A line for the run's log (returned with the result, and in FigCraft's log).
pub fn log(line: &str) {
    host::log(line.as_bytes());
}

/// A short message shown to the user.
pub fn notify(message: &str) {
    host::notify(message.as_bytes());
}

/// The body of `figcraft_run` (use [`entry!`]): read the input, run `f`, hand back its output.
pub fn run_plugin(input_len: i32, f: fn(Input) -> Result<Value>) -> i32 {
    let input = host::reply();
    let input = input.get(..usize::try_from(input_len).unwrap_or(0)).unwrap_or(&input);
    let result = core::str::from_utf8(input).map_err(|_| "the input isn't UTF-8".to_string()).and_then(Input::parse).and_then(f);
    match result {
        Ok(v) => {
            host::set_output(serde_json::to_string(&v).unwrap_or_default().as_bytes());
            0
        }
        Err(e) => {
            host::set_output(e.as_bytes());
            1
        }
    }
}

/// Export `f: fn(Input) -> Result<Value>` as the plugin's entry point.
#[macro_export]
macro_rules! entry {
    ($f:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn figcraft_run(input_len: i32) -> i32 {
            $crate::run_plugin(input_len, $f)
        }
    };
}

/// The host functions FigCraft provides (module `figcraft`).
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod host {
    use alloc::vec;
    use alloc::vec::Vec;

    mod sys {
        #[link(wasm_import_module = "figcraft")]
        unsafe extern "C" {
            pub fn call(cmd: *const u8, cmd_len: usize, params: *const u8, params_len: usize) -> i32;
            pub fn selection() -> i32;
            pub fn reply_len() -> i32;
            pub fn reply_read(ptr: *mut u8, len: usize) -> i32;
            pub fn log(ptr: *const u8, len: usize);
            pub fn notify(ptr: *const u8, len: usize);
            pub fn set_output(ptr: *const u8, len: usize);
        }
    }

    // SAFETY (every call below): the host reads only the `len` bytes at `ptr`, which are a live
    // slice, and writes only up to `len` bytes into the buffer it's given; it checks bounds itself.
    pub fn call(cmd: &[u8], params: &[u8]) -> i32 {
        unsafe { sys::call(cmd.as_ptr(), cmd.len(), params.as_ptr(), params.len()) }
    }
    pub fn selection() -> i32 {
        unsafe { sys::selection() }
    }
    pub fn reply() -> Vec<u8> {
        let len = usize::try_from(unsafe { sys::reply_len() }).unwrap_or(0);
        let mut buf = vec![0u8; len];
        let got = unsafe { sys::reply_read(buf.as_mut_ptr(), buf.len()) };
        buf.truncate(usize::try_from(got).unwrap_or(0));
        buf
    }
    pub fn log(s: &[u8]) {
        unsafe { sys::log(s.as_ptr(), s.len()) }
    }
    pub fn notify(s: &[u8]) {
        unsafe { sys::notify(s.as_ptr(), s.len()) }
    }
    pub fn set_output(s: &[u8]) {
        unsafe { sys::set_output(s.as_ptr(), s.len()) }
    }
}

/// Outside WebAssembly (a plugin's own unit tests) every host call fails.
#[cfg(not(target_arch = "wasm32"))]
mod host {
    use alloc::vec::Vec;

    pub fn call(_cmd: &[u8], _params: &[u8]) -> i32 {
        1
    }
    pub fn selection() -> i32 {
        0
    }
    pub fn reply() -> Vec<u8> {
        b"not running inside FigCraft".to_vec()
    }
    pub fn log(_s: &[u8]) {}
    pub fn notify(_s: &[u8]) {}
    pub fn set_output(_s: &[u8]) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_parses_and_host_calls_fail_off_wasm() {
        let i = Input::parse(r#"{"api":1,"command":"grid","params":{"n":3,"s":"x","b":true},"selection":[4,"x",7]}"#).unwrap_or_default();
        assert_eq!(i.command, "grid");
        assert_eq!(i.selection, alloc::vec![4, 7]);
        assert_eq!((i.f64("n"), i.str("s"), i.bool("b")), (Some(3.0), Some("x"), Some(true)));
        assert!(Input::parse("nope").is_err());
        assert!(call("node.get", &json!({"id": 1})).is_err());
        assert!(selection().is_empty());
    }
}
