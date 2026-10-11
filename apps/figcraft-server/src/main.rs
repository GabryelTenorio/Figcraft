//! `figcraft-server`: see `docs/server.md`, or run it with `--help`.

use std::process::ExitCode;

use figcraft_server::cli::{self, Command};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match cli::parse(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("figcraft-server: {e}\n(figcraft-server --help lists what it takes)");
            return ExitCode::from(2);
        }
    };
    if cli.command != Command::Serve {
        return match cli::run(&cli) {
            Ok(out) => {
                print!("{out}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("figcraft-server: {e}");
                ExitCode::FAILURE
            }
        };
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).target(env_logger::Target::Stdout).init();
    match figcraft_server::start(&cli.config) {
        // Serves until the process is stopped: what was confirmed is on disk already, and clients
        // reconnect once it runs again.
        Ok(_running) => loop {
            std::thread::park();
        },
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
