use xzram::apply::{PendingConfig, ZramConfig};
use xzram::backend::available_zram_backend;
use xzram::config::default_zram_config;

use crate::args::ZramCommands;
use crate::print::print_zram_config;
use crate::privileged::run_privileged;

pub(crate) fn run(command: ZramCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        ZramCommands::Show => {
            let backend = available_zram_backend()?;
            let config = backend.show()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else if let Some(c) = config {
                print_zram_config(&c);
            } else {
                println!("No zram configuration found");
            }
        }
        ZramCommands::Set {
            device,
            size,
            algorithm,
            priority,
            now,
        } => {
            let defaults = default_zram_config();
            let config = ZramConfig {
                device: device.unwrap_or(defaults.name),
                zram_size: Some(size.unwrap_or_else(|| {
                    defaults
                        .zram_size
                        .unwrap_or_else(|| "min(ram / 2, 4096)".into())
                })),
                zram_resident_limit: None,
                compression_algorithm: Some(algorithm.unwrap_or_else(|| {
                    defaults
                        .compression_algorithm
                        .unwrap_or_else(|| "zstd".into())
                })),
                swap_priority: Some(priority.unwrap_or(defaults.swap_priority.unwrap_or(100))),
                fs_type: None,
                mount_point: None,
            };
            let pending = PendingConfig {
                zram: Some(config.clone()),
                ..Default::default()
            };
            if now {
                run_privileged(dbus, "zram.configure", &serde_json::to_string(&config)?)?;
            } else {
                run_privileged(dbus, "stage", &serde_json::to_string(&pending)?)?;
                println!("Staged zram configuration; run 'xzram apply' to apply");
            }
        }
        ZramCommands::Disable { now } => {
            if now {
                run_privileged(dbus, "zram.disable", "{}")?;
            } else {
                let pending = PendingConfig {
                    disable_zram: true,
                    ..Default::default()
                };
                run_privileged(dbus, "stage", &serde_json::to_string(&pending)?)?;
                println!("Staged zram disable; run 'xzram apply' to apply");
            }
        }
        ZramCommands::Migrate { now } => {
            run_privileged(dbus, "zram.migrate", "{}")?;
            if now {
                run_privileged(dbus, "apply", "{}")?;
            }
        }
    }
    Ok(())
}
