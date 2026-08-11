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
            prepare,
            mkdir,
        } => {
            let config = SwapfileConfig {
                path: path.clone(),
                size_mb,
                priority,
            };
            let pending = PendingConfig {
                swapfile: Some(config.clone()),
                ..Default::default()
            };
            if now {
                let mut create_payload = serde_json::to_value(&config)?;
                if prepare {
                    if let Some(obj) = create_payload.as_object_mut() {
                        obj.insert("prepare".into(), serde_json::Value::Bool(true));
                        obj.insert("mkdir_parents".into(), serde_json::Value::Bool(mkdir));
                    }
                }
                run_privileged(dbus, "swapfile.create", &create_payload.to_string())?;
            } else if prepare {
                let payload = serde_json::json!({
                    "pending": pending,
                    "prepare_swapfile": {
                        "path": path,
                        "mkdir_parents": mkdir,
                    }
                });
                run_privileged(dbus, "stage", &payload.to_string())?;
                println!("Staged swapfile create (with prepare); run 'xzram apply' to apply");
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
            xzram::validation::validate_swapfile_prepare_path(&path)?;
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
