//! The sandbox: a plugin module runs in a metered WebAssembly interpreter (wasmi; no JIT) with a
//! memory cap, and sees the outside world only through the host functions below. There is no WASI:
//! no file system, network, clock or randomness.
//!
//! # ABI (version 1)
//!
//! The module exports its `memory` and `figcraft_run(input_len: i32) -> i32`. Host functions are
//! imported from the module `figcraft`; strings are UTF-8 `(ptr, len)` pairs in the plugin's memory.
//!
//! | Import | Signature | Does |
//! |---|---|---|
//! | `call` | `(cmd_ptr, cmd_len, params_ptr, params_len) -> i32` | Runs a command with JSON params. 0: ok, the reply holds the result JSON; 1: refused or failed, the reply holds the message. |
//! | `selection` | `() -> i32` | The reply becomes the selected layer ids (a JSON array); returns its length. |
//! | `reply_len` | `() -> i32` | Length of the reply. |
//! | `reply_read` | `(ptr, len) -> i32` | Copies up to `len` bytes of the reply to `ptr`; returns the count. |
//! | `log` | `(ptr, len)` | A line for the run's log. |
//! | `notify` | `(ptr, len)` | A short message shown to the user. |
//! | `set_output` | `(ptr, len)` | The run's result (JSON or text), or its error message when it fails. |
//!
//! When `figcraft_run` starts, the reply holds the input JSON (`input_len` bytes):
//! `{"command", "params", "selection"}`. It returns 0 on success; anything else fails the run.

use wasmi::{Caller, Config, Engine, Extern, FuncType, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TrapCode, ValType};

/// The module host functions are imported from.
pub const IMPORT_MODULE: &str = "figcraft";
/// The exported entry point.
pub const ENTRY: &str = "figcraft_run";

/// The host functions: (name, params, results), all i32.
const IMPORTS: &[(&str, usize, usize)] =
    &[("call", 4, 1), ("selection", 0, 1), ("reply_len", 0, 1), ("reply_read", 2, 1), ("log", 2, 0), ("notify", 2, 0), ("set_output", 2, 0)];

/// What a run may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Instruction budget (wasmi fuel). Each host call costs [`Limits::call_fuel`] more.
    pub fuel: u64,
    pub call_fuel: u64,
    /// Linear memory cap in bytes.
    pub memory_bytes: usize,
    /// Largest module accepted.
    pub module_bytes: usize,
    /// Host calls per run.
    pub max_calls: u32,
    /// Largest string passed either way (command params, results, logs).
    pub max_message: usize,
    /// Log lines kept per run.
    pub max_logs: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: 1_000_000_000,
            call_fuel: 10_000,
            memory_bytes: 64 * 1024 * 1024,
            module_bytes: 16 * 1024 * 1024,
            max_calls: 100_000,
            max_message: 16 * 1024 * 1024,
            max_logs: 1000,
        }
    }
}

/// Why a run failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PluginError {
    #[error("not a valid plugin module: {0}")]
    InvalidModule(String),
    #[error("the plugin ran out of its instruction budget (an endless loop?)")]
    OutOfFuel,
    #[error("the plugin used more than its memory limit")]
    OutOfMemory,
    #[error("the plugin crashed: {0}")]
    Trap(String),
    #[error("the plugin went over a limit: {0}")]
    Limit(String),
    #[error("{0}")]
    Failed(String),
}

/// What the sandbox asks of its host (the engine): run commands, read the selection, log.
pub trait Host {
    /// Run `command` with `params` (JSON text): the result JSON, or why it was refused or failed.
    fn call(&mut self, command: &str, params: &str) -> Result<String, String>;
    /// The selected layer ids as a JSON array.
    fn selection(&mut self) -> String;
    /// A message for the user.
    fn notify(&mut self, message: &str);
    /// A line of the run's log, as the plugin writes it (it's also in [`RunOutput::logs`]).
    fn log(&mut self, _line: &str) {}
}

/// A finished run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunOutput {
    /// What the plugin passed to `set_output` (empty when nothing).
    pub output: String,
    pub logs: Vec<String>,
    /// Host calls made.
    pub calls: u32,
    /// Fuel used.
    pub fuel: u64,
}

struct State<'h> {
    host: &'h mut dyn Host,
    limits: Limits,
    store_limits: StoreLimits,
    reply: Vec<u8>,
    out: RunOutput,
    /// A limit the host tripped (reported instead of the trap it causes).
    abort: Option<PluginError>,
}

fn engine() -> Engine {
    let mut c = Config::default();
    c.consume_fuel(true);
    Engine::new(&c)
}

fn func_type(params: usize, results: usize) -> FuncType {
    FuncType::new(vec![ValType::I32; params], vec![ValType::I32; results])
}

/// Compile and check a module: its size, that it imports only FigCraft's host functions (no WASI)
/// and exports `memory` and `figcraft_run`.
pub fn check_module(bytes: &[u8], limits: &Limits) -> Result<(), PluginError> {
    compile(&engine(), bytes, limits).map(|_| ())
}

fn compile(engine: &Engine, bytes: &[u8], limits: &Limits) -> Result<Module, PluginError> {
    if bytes.len() > limits.module_bytes {
        return Err(PluginError::InvalidModule(format!("larger than {} MB", limits.module_bytes / (1024 * 1024))));
    }
    if !bytes.starts_with(b"\0asm") {
        return Err(PluginError::InvalidModule("not a WebAssembly binary".into()));
    }
    let module = Module::new(engine, bytes).map_err(|e| PluginError::InvalidModule(e.to_string()))?;
    for imp in module.imports() {
        let known = IMPORTS.iter().find(|(n, ..)| imp.module() == IMPORT_MODULE && *n == imp.name());
        match (known, imp.ty().func()) {
            (Some((_, p, r)), Some(ty)) if *ty == func_type(*p, *r) => {}
            (Some(_), _) => {
                return Err(PluginError::InvalidModule(format!("`{}.{}` is imported with the wrong signature", imp.module(), imp.name())));
            }
            (None, _) => {
                return Err(PluginError::InvalidModule(format!(
                    "it imports `{}.{}`, which FigCraft doesn't provide (plugins get no file system, network or clock)",
                    imp.module(),
                    imp.name()
                )));
            }
        }
    }
    let export = |name: &str| module.exports().find(|e| e.name() == name).map(|e| e.ty().clone());
    if !export("memory").is_some_and(|t| t.memory().is_some()) {
        return Err(PluginError::InvalidModule("it doesn't export its `memory`".into()));
    }
    if !export(ENTRY).is_some_and(|t| t.func() == Some(&func_type(1, 1))) {
        return Err(PluginError::InvalidModule(format!("it doesn't export `{ENTRY}(input_len: i32) -> i32`")));
    }
    Ok(module)
}

/// Run a plugin module: `input` is the JSON handed to `figcraft_run`.
pub fn run(bytes: &[u8], input: &str, limits: &Limits, host: &mut dyn Host) -> Result<RunOutput, PluginError> {
    let engine = engine();
    let module = compile(&engine, bytes, limits)?;
    if input.len() > limits.max_message {
        return Err(PluginError::Limit("the input is too large".into()));
    }
    let store_limits = StoreLimitsBuilder::new().memory_size(limits.memory_bytes).memories(1).tables(4).table_elements(100_000).instances(1).build();
    let state = State { host, limits: *limits, store_limits, reply: input.as_bytes().to_vec(), out: RunOutput::default(), abort: None };
    let mut store = Store::new(&engine, state);
    store.limiter(|s| &mut s.store_limits);
    store.set_fuel(limits.fuel).map_err(|e| PluginError::Trap(e.to_string()))?;
    let linker = linker(&engine)?;
    let instance = linker.instantiate_and_start(&mut store, &module).map_err(|e| classify(&mut store, &e, true))?;
    let entry = instance.get_typed_func::<i32, i32>(&store, ENTRY).map_err(|e| PluginError::InvalidModule(e.to_string()))?;
    let input_len = i32::try_from(input.len()).map_err(|_| PluginError::Limit("the input is too large".into()))?;
    let status = entry.call(&mut store, input_len).map_err(|e| classify(&mut store, &e, false))?;
    let fuel = limits.fuel.saturating_sub(store.get_fuel().unwrap_or(0));
    let mut out = std::mem::take(&mut store.data_mut().out);
    out.fuel = fuel;
    if status != 0 {
        let msg = if out.output.trim().is_empty() { format!("the plugin failed (status {status})") } else { out.output };
        return Err(PluginError::Failed(msg));
    }
    Ok(out)
}

/// The error a failed instantiation or call stands for: a limit the host tripped first, else the
/// trap.
fn classify(store: &mut Store<State<'_>>, e: &wasmi::Error, instantiating: bool) -> PluginError {
    if let Some(abort) = store.data_mut().abort.take() {
        return abort;
    }
    match e.as_trap_code() {
        Some(TrapCode::OutOfFuel) => PluginError::OutOfFuel,
        Some(TrapCode::GrowthOperationLimited) => PluginError::OutOfMemory,
        Some(code) => PluginError::Trap(code.to_string()),
        // A memory larger than the cap fails instantiation.
        None if instantiating && e.to_string().to_ascii_lowercase().contains("limit") => PluginError::OutOfMemory,
        None if instantiating => PluginError::InvalidModule(e.to_string()),
        None => PluginError::Trap(e.to_string()),
    }
}

type Ctx<'a, 'h> = Caller<'a, State<'h>>;

/// Stop the run because of `err` (the trap it causes reports `err`).
fn abort(caller: &mut Ctx<'_, '_>, err: PluginError) -> wasmi::Error {
    let msg = err.to_string();
    caller.data_mut().abort = Some(err);
    wasmi::Error::new(msg)
}

/// Charge a host call: fuel and the call count.
fn charge(caller: &mut Ctx<'_, '_>) -> Result<(), wasmi::Error> {
    let cost = caller.data().limits.call_fuel;
    let left = caller.get_fuel()?;
    if left < cost {
        return Err(abort(caller, PluginError::OutOfFuel));
    }
    caller.set_fuel(left - cost)?;
    let st = caller.data_mut();
    st.out.calls = st.out.calls.saturating_add(1);
    if st.out.calls > st.limits.max_calls {
        let n = st.limits.max_calls;
        return Err(abort(caller, PluginError::Limit(format!("more than {n} host calls"))));
    }
    Ok(())
}

/// The bytes at `(ptr, len)` in the plugin's memory.
fn read_bytes(caller: &mut Ctx<'_, '_>, ptr: i32, len: i32) -> Result<Vec<u8>, wasmi::Error> {
    let (ptr, len) = (ptr as u32 as usize, len as u32 as usize);
    if len > caller.data().limits.max_message {
        return Err(abort(caller, PluginError::Limit(format!("a {len}-byte string is too long"))));
    }
    let Some(mem) = caller.get_export("memory").and_then(Extern::into_memory) else {
        return Err(abort(caller, PluginError::InvalidModule("no memory".into())));
    };
    match ptr.checked_add(len).and_then(|end| mem.data(&*caller).get(ptr..end)) {
        Some(b) => Ok(b.to_vec()),
        None => Err(wasmi::Error::from(TrapCode::MemoryOutOfBounds)),
    }
}

/// The UTF-8 text at `(ptr, len)`.
fn read_str(caller: &mut Ctx<'_, '_>, ptr: i32, len: i32) -> Result<String, wasmi::Error> {
    let bytes = read_bytes(caller, ptr, len)?;
    String::from_utf8(bytes).map_err(|_| abort(caller, PluginError::Trap("a string isn't valid UTF-8".into())))
}

/// Make `bytes` the reply; returns its length.
fn set_reply(caller: &mut Ctx<'_, '_>, bytes: Vec<u8>) -> Result<i32, wasmi::Error> {
    let max = caller.data().limits.max_message;
    let bytes = if bytes.len() > max { format!("the result is larger than {max} bytes").into_bytes() } else { bytes };
    let len = i32::try_from(bytes.len()).unwrap_or(i32::MAX);
    caller.data_mut().reply = bytes;
    Ok(len)
}

fn linker<'h>(engine: &Engine) -> Result<Linker<State<'h>>, PluginError> {
    let mut l = <Linker<State<'h>>>::new(engine);
    let err = |e: wasmi::errors::LinkerError| PluginError::InvalidModule(e.to_string());
    l.func_wrap(IMPORT_MODULE, "call", |mut c: Ctx<'_, 'h>, cp: i32, cl: i32, pp: i32, pl: i32| -> Result<i32, wasmi::Error> {
        charge(&mut c)?;
        let cmd = read_str(&mut c, cp, cl)?;
        let params = read_str(&mut c, pp, pl)?;
        let params = if params.trim().is_empty() { "{}".to_string() } else { params };
        let (status, reply) = match c.data_mut().host.call(&cmd, &params) {
            Ok(v) => (0, v),
            Err(m) => (1, m),
        };
        set_reply(&mut c, reply.into_bytes())?;
        Ok(status)
    })
    .map_err(err)?;
    l.func_wrap(IMPORT_MODULE, "selection", |mut c: Ctx<'_, 'h>| -> Result<i32, wasmi::Error> {
        charge(&mut c)?;
        let sel = c.data_mut().host.selection();
        set_reply(&mut c, sel.into_bytes())
    })
    .map_err(err)?;
    l.func_wrap(IMPORT_MODULE, "reply_len", |c: Ctx<'_, 'h>| -> i32 { i32::try_from(c.data().reply.len()).unwrap_or(i32::MAX) }).map_err(err)?;
    l.func_wrap(IMPORT_MODULE, "reply_read", |mut c: Ctx<'_, 'h>, ptr: i32, len: i32| -> Result<i32, wasmi::Error> {
        let (ptr, want) = (ptr as u32 as usize, len as u32 as usize);
        let Some(mem) = c.get_export("memory").and_then(Extern::into_memory) else {
            return Err(abort(&mut c, PluginError::InvalidModule("no memory".into())));
        };
        let (data, st) = mem.data_and_store_mut(&mut c);
        let n = want.min(st.reply.len());
        let src = st.reply.get(..n).ok_or(TrapCode::MemoryOutOfBounds)?;
        let dst = ptr.checked_add(n).and_then(|end| data.get_mut(ptr..end)).ok_or(TrapCode::MemoryOutOfBounds)?;
        dst.copy_from_slice(src);
        Ok(i32::try_from(n).unwrap_or(i32::MAX))
    })
    .map_err(err)?;
    l.func_wrap(IMPORT_MODULE, "log", |mut c: Ctx<'_, 'h>, ptr: i32, len: i32| -> Result<(), wasmi::Error> {
        charge(&mut c)?;
        let line = read_str(&mut c, ptr, len)?;
        let st = c.data_mut();
        st.host.log(&line);
        if st.out.logs.len() < st.limits.max_logs {
            st.out.logs.push(line);
        }
        Ok(())
    })
    .map_err(err)?;
    l.func_wrap(IMPORT_MODULE, "notify", |mut c: Ctx<'_, 'h>, ptr: i32, len: i32| -> Result<(), wasmi::Error> {
        charge(&mut c)?;
        let msg = read_str(&mut c, ptr, len)?;
        let msg: String = msg.chars().filter(|ch| !ch.is_control()).take(200).collect();
        c.data_mut().host.notify(&msg);
        Ok(())
    })
    .map_err(err)?;
    l.func_wrap(IMPORT_MODULE, "set_output", |mut c: Ctx<'_, 'h>, ptr: i32, len: i32| -> Result<(), wasmi::Error> {
        let out = read_str(&mut c, ptr, len)?;
        c.data_mut().out.output = out;
        Ok(())
    })
    .map_err(err)?;
    Ok(l)
}
