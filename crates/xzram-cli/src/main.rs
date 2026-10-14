mod args;
mod commands;
mod dbus_client;
mod exit_codes;
mod helper_version;
mod print;
mod privileged;
mod snapshot_ops;
mod store_read;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use args::Cli;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    // clap exits with `exit_codes::USAGE` (2) on argument errors.
    let cli = Cli::parse();
    match commands::run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("xzram: {e:#}");
            ExitCode::from(exit_codes::ERROR)
        }
    }
}
