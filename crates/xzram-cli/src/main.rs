mod args;
mod commands;
mod dbus_client;
mod print;
mod privileged;
mod snapshot_ops;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use args::Cli;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();
    commands::run(cli)
}
