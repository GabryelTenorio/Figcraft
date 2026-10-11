//! `figcraft-cli`: FigCraft without a window. Inspect, export, convert and render documents, run
//! command scripts, list commands, serve MCP on stdio and measure performance.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod args;
mod files;
mod perf;

use std::io::Write;
use std::process::ExitCode;

use args::Args;
use serde_json::{Value, json};

pub const USAGE: &str = "figcraft-cli — FigCraft without a window

USAGE:
  figcraft-cli info <file> [--json]
  figcraft-cli export <file> -o <out> [--format png|jpg|svg|webp|pdf] [--scale N] [--page NAME] [--node ID|NAME]
                      [--bleed LENGTH] [--crop-marks] [--bleed-marks] [--cmyk]
  figcraft-cli convert <in> <out>
  figcraft-cli render <file> -o <out.png> [--scale N] [--page NAME]
  figcraft-cli run <script.json|script.jsonl> [--in <file>] [--out <file>]
  figcraft-cli commands [--json] [--filter TEXT]
  figcraft-cli mcp [--attach PORT]
  figcraft-cli perf [--layers N[,N...]] [--instances N[,N...]]
  figcraft-cli help | --help | --version

Files: .figcraft (native), .json (FigCraft or REST API file JSON), .fig (best effort; `info` lists what
wasn't imported), .svg, .png/.jpg/.webp/.gif/.bmp (open as an image layer). `export` writes one file
per top-level frame into the <out> directory, or one file when <out> has an image extension or --node
is given; PDFs take print options (a bleed like 3mm or 0.125in,
crop and bleed marks, CMYK colour). `run` executes a JSON array (or JSON lines) of
{\"command\": id, \"params\": {...}} in a headless session and prints one JSON result line per step.
See docs/cli.md and docs/mcp.md.";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(code) => code,
        Err(CliError::Usage(msg)) => {
            eprintln!("error: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug)]
pub enum CliError {
    /// Bad arguments: usage is printed, exit code 2.
    Usage(String),
    /// The operation failed: exit code 1.
    Failed(String),
}

impl From<String> for CliError {
    fn from(s: String) -> Self {
        CliError::Failed(s)
    }
}

pub type CliResult<T> = Result<T, CliError>;

fn run(argv: &[String]) -> CliResult<ExitCode> {
    let Some((cmd, rest)) = argv.split_first() else { return Err(CliError::Usage("missing subcommand".into())) };
    match cmd.as_str() {
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "--version" | "-V" | "version" => {
            println!("figcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        "info" => {
            let a = Args::parse(rest, &[], &["json"])?;
            let file = a.one_positional("info <file>")?;
            files::info(&file, a.flag("json"))
        }
        "export" => {
            let a = Args::parse(rest, &["format", "scale", "page", "node", "o", "out", "bleed"], &["crop-marks", "bleed-marks", "cmyk"])?;
            let file = a.one_positional("export <file> -o <out>")?;
            let out = a.value("o").or_else(|| a.value("out")).ok_or_else(|| CliError::Usage("export needs -o <out>".into()))?;
            let target =
                files::Target { format: a.value("format"), scale: a.scale()?, page: a.value("page"), node: a.value("node"), print: a.print()? };
            files::export(&file, &out, &target)
        }
        "convert" => {
            let a = Args::parse(rest, &[], &[])?;
            match a.positional.as_slice() {
                [i, o] => files::convert(i, o),
                _ => Err(CliError::Usage("convert <in> <out>".into())),
            }
        }
        "render" => {
            let a = Args::parse(rest, &["o", "out", "scale", "page"], &[])?;
            let file = a.one_positional("render <file> -o <out.png>")?;
            let out = a.value("o").or_else(|| a.value("out")).ok_or_else(|| CliError::Usage("render needs -o <out.png>".into()))?;
            files::render(&file, &out, a.scale()?, a.value("page").as_deref())
        }
        "run" => {
            let a = Args::parse(rest, &["in", "out"], &[])?;
            let script = a.one_positional("run <script.json|.jsonl>")?;
            files::run_script(&script, a.value("in").as_deref(), a.value("out").as_deref())
        }
        "commands" => {
            let a = Args::parse(rest, &["filter"], &["json"])?;
            if !a.positional.is_empty() {
                return Err(CliError::Usage("commands takes no positional arguments".into()));
            }
            commands(a.flag("json"), a.value("filter").as_deref());
            Ok(ExitCode::SUCCESS)
        }
        "mcp" => {
            let a = Args::parse(rest, &["attach"], &[])?;
            if !a.positional.is_empty() {
                return Err(CliError::Usage("mcp takes no positional arguments".into()));
            }
            mcp(a.value("attach").as_deref())
        }
        "perf" => {
            let a = Args::parse(rest, &["layers", "instances"], &[])?;
            let sizes: Vec<usize> = a.usizes("layers", &[1000, 10_000])?.into_iter().map(|n| n.clamp(10, 2_000_000)).collect();
            let instances: Vec<usize> = a.usizes("instances", &[2000])?.into_iter().filter(|n| *n > 0).map(|n| n.min(100_000)).collect();
            perf::run(&sizes, &instances)
        }
        other => Err(CliError::Usage(format!("unknown subcommand `{other}`"))),
    }
}

fn commands(as_json: bool, filter: Option<&str>) {
    let f = filter.map(str::to_lowercase).unwrap_or_default();
    let list: Vec<&figcraft_engine::CommandSpec> = figcraft_engine::all_commands()
        .iter()
        .filter(|c| f.is_empty() || c.id.to_lowercase().contains(&f) || c.label.to_lowercase().contains(&f) || c.params.to_lowercase().contains(&f))
        .collect();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if as_json {
        let v: Vec<Value> = list
            .iter()
            .map(|c| json!({"id": c.id, "label": c.label, "params": c.params, "menu": c.menu(), "shortcut": c.shortcut(), "undoable": c.history}))
            .collect();
        let _ = writeln!(out, "{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return;
    }
    for c in list {
        let sc = c.shortcut().map(|s| format!(" ({s})")).unwrap_or_default();
        let undo = if c.history { "*" } else { "" };
        let _ = writeln!(out, "{}{undo}  {}{sc}\n    {}", c.id, c.label, c.params);
    }
}

fn mcp(attach: Option<&str>) -> CliResult<ExitCode> {
    let backend: Box<dyn figcraft_mcp::Backend> = match attach {
        Some(p) => {
            let addr = if p.contains(':') {
                p.to_string()
            } else {
                format!("127.0.0.1:{}", p.parse::<u16>().map_err(|_| CliError::Usage(format!("--attach needs a port number, got `{p}`")))?)
            };
            let b = figcraft_mcp::Attached::connect(&addr)
                .map_err(|e| format!("can't reach the FigCraft app at {addr}: {e} (start it with `figcraft --control PORT`)"))?;
            Box::new(b)
        }
        None => Box::new(figcraft_mcp::Headless::new()),
    };
    eprintln!("figcraft MCP server ({}) on stdio", backend.describe());
    let mut server = figcraft_mcp::Server::new(backend);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    server.serve(stdin.lock(), stdout.lock()).map_err(|e| format!("stdio: {e}"))?;
    Ok(ExitCode::SUCCESS)
}
