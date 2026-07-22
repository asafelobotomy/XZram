mod apply_rollback;
mod defaults;
mod read;
mod snapshot;
mod swap;
mod swapfile;
mod sysctl;
mod zram;

use crate::args::{Cli, Commands, DaemonCommands, PendingCommands};

pub(crate) fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Status => read::status(cli.json)?,
        Commands::Detect => read::detect(cli.json)?,
        Commands::Doctor => read::doctor(cli.json)?,
        Commands::Zram { command } => zram::run(command, cli.json, cli.dbus)?,
        Commands::Swapfile { command } => swapfile::run(command, cli.json, cli.dbus)?,
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
        Commands::Apply => apply_rollback::apply(cli.dbus)?,
        Commands::Rollback => apply_rollback::rollback(cli.dbus)?,
        Commands::Snapshot { command } => snapshot::run(command, cli.json, cli.dbus)?,
    }
    Ok(())
}
