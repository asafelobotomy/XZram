use xzram::apply::{PendingConfig, SwapfileConfig, SwapfileResizeConfig};
use xzram::backend::available_swapfile_backend;
use xzram::swapfile_btrfs;

use crate::args::SwapfileCommands;
use crate::print::print_nodatacow_status;
use crate::privileged::run_privileged;

pub(crate) fn run(command: SwapfileCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        SwapfileCommands::List => {
            let backend = available_swapfile_backend();
            let files = backend.list()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&files)?);
            } else {
                for f in &files {
                    println!("{}  {} MiB  priority {}", f.path, f.size_mb, f.priority);
                }
            }
        }
        SwapfileCommands::Create {
            path,
            size_mb,
            priority,
            now,
        } => {
            let config = SwapfileConfig {
                path,
                size_mb,
                priority,
            };
            let pending = PendingConfig {
                swapfile: Some(config.clone()),
                ..Default::default()
            };
            if now {
                run_privileged(dbus, "swapfile.create", &serde_json::to_string(&config)?)?;
            } else {
                run_privileged(dbus, "stage", &serde_json::to_string(&pending)?)?;
                println!("Staged swapfile create; run 'xzram apply' to apply");
            }
        }
        SwapfileCommands::Resize { path, size_mb, now } => {
            let resize = SwapfileResizeConfig { path, size_mb };
            let pending = PendingConfig {
                swapfile_resize: Some(resize.clone()),
                ..Default::default()
            };
            if now {
                run_privileged(dbus, "swapfile.resize", &serde_json::to_string(&resize)?)?;
            } else {
                run_privileged(dbus, "stage", &serde_json::to_string(&pending)?)?;
                println!("Staged swapfile resize; run 'xzram apply' to apply");
            }
        }
        SwapfileCommands::Remove { path, now } => {
            let pending = PendingConfig {
                remove_swapfile: Some(path.clone()),
                ..Default::default()
            };
            if now {
                let payload = serde_json::json!({ "path": path });
                run_privileged(dbus, "swapfile.remove", &payload.to_string())?;
            } else {
                run_privileged(dbus, "stage", &serde_json::to_string(&pending)?)?;
                println!("Staged swapfile remove; run 'xzram apply' to apply");
            }
        }
        SwapfileCommands::Check { path } => {
            let status = swapfile_btrfs::check_nodatacow(std::path::Path::new(&path))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_nodatacow_status(&status);
            }
        }
        SwapfileCommands::Prepare { path, mkdir } => {
            let payload = serde_json::json!({
                "path": path,
                "mkdir_parents": mkdir,
            });
            run_privileged(dbus, "swapfile.prepare", &payload.to_string())?;
            let status = swapfile_btrfs::check_nodatacow(std::path::Path::new(&path))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_nodatacow_status(&status);
            }
        }
    }
    Ok(())
}
