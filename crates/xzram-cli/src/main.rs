mod args;
mod commands;
#[cfg(test)]
mod completion;
mod dbus_client;
mod exit_codes;
mod helper_version;
mod plan;
mod print;
mod privileged;
mod progress;
mod settings;
mod snapshot_ops;
mod store_read;

use std::process::ExitCode;

use clap::Parser;

use args::Cli;

fn main() -> ExitCode {
    // uQ's log format and a crash record on a panic (uQ's native/uq-diag).
    xzram::uq_diag::init("xzram", "warn");

    // clap exits with `exit_codes::USAGE` (2) on argument errors.
    let cli = Cli::parse();
    privileged::quiet_helper_stdout(cli.json || cli.ndjson);
    match commands::run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("xzram: {e:#}");
            ExitCode::from(exit_codes::ERROR)
        }
    }
}
