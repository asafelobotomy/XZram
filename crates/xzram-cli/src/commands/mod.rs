pub(crate) mod apply_rollback;
mod config;
mod defaults;
mod dry_run;
mod read;
mod snapshot;
mod swap;
mod swapfile;
mod sysctl;
mod zram;

use crate::args::{Cli, Commands, DaemonCommands, PendingCommands};

/// Run a parsed command; returns the process exit code (see `crate::exit_codes`).
pub(crate) fn run(cli: Cli) -> anyhow::Result<u8> {
    match cli.command {
        Commands::Status => read::status(cli.json)?,
        Commands::Config { command } => {
            let mode = config::Mode {
                json: cli.json,
                ndjson: cli.ndjson,
                dbus: cli.dbus,
            };
            return config::run(command, mode);
        }
        Commands::Detect => read::detect(cli.json)?,
        Commands::Doctor => return read::doctor(cli.json),
        Commands::Zram { command } => zram::run(command, cli.json, cli.dbus, cli.ndjson)?,
        Commands::Swapfile { command } => swapfile::run(command, cli.json, cli.dbus, cli.ndjson)?,
        Commands::Swap { command } => swap::run(command, cli.json, cli.dbus)?,
        Commands::Sysctl { command } => sysctl::run(command, cli.json, cli.dbus)?,
        Commands::Pending { command } => match command {
            PendingCommands::Show => read::pending_show(cli.json)?,
            PendingCommands::Clear => apply_rollback::pending_clear(cli.dbus)?,
        },
        Commands::Daemon { command } => match command {
            DaemonCommands::Start => apply_rollback::daemon_start()?,
        },
        Commands::Defaults { command } => defaults::run(command, cli.json, cli.dbus)?,
        Commands::Apply { zram } => apply_rollback::apply(cli.dbus, zram.options())?,
        Commands::Rollback { dry_run } => {
            if dry_run {
                dry_run::rollback(cli.json)?
            } else {
                apply_rollback::rollback(cli.dbus, cli.ndjson)?
            }
        }
        Commands::Snapshot { command } => snapshot::run(command, cli.json, cli.dbus, cli.ndjson)?,
    }
    Ok(crate::exit_codes::OK)
}
