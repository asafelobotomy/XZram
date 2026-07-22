use xzram::snapshot;

use crate::args::SnapshotCommands;
use crate::dbus_client;
use crate::print::confirm;
use crate::privileged::run_privileged;
use crate::snapshot_ops::{
    resolve_snapshot_id, run_snapshot_create_dbus, run_snapshot_create_pkexec,
};

pub(crate) fn run(command: SnapshotCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        SnapshotCommands::List => {
            let list = snapshot::list_snapshots()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else if list.is_empty() {
                println!("No snapshots stored");
            } else {
                println!("{:<40} {:<12} LABEL", "ID", "TRIGGER");
                for s in list {
                    println!("{:<40} {:<12} {}", s.id, s.trigger.as_str(), s.label);
                }
            }
        }
        SnapshotCommands::Create { label } => {
            let meta = if dbus && dbus_client::is_available() {
                run_snapshot_create_dbus(label.as_deref())?
            } else {
                run_snapshot_create_pkexec(label.as_deref())?
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&meta)?);
            } else {
                println!("Created snapshot: {} ({})", meta.label, meta.id);
            }
        }
        SnapshotCommands::Restore { id } => {
            let snapshot_id = resolve_snapshot_id(&id)?;
            run_privileged(
                dbus,
                "snapshot.restore",
                &serde_json::json!({ "id": snapshot_id }).to_string(),
            )?;
        }
        SnapshotCommands::Delete { id, yes } => {
            if !yes && !confirm("Delete snapshot? This cannot be undone.")? {
                println!("Cancelled");
                return Ok(());
            }
            run_privileged(
                dbus,
                "snapshot.delete",
                &serde_json::json!({ "id": id }).to_string(),
            )?;
            println!("Deleted snapshot {id}");
        }
        SnapshotCommands::Prune { keep, yes } => {
            if !yes && !confirm(&format!("Prune snapshots, keeping newest {keep}?"))? {
                println!("Cancelled");
                return Ok(());
            }
            run_privileged(
                dbus,
                "snapshot.prune",
                &serde_json::json!({ "keep": keep }).to_string(),
            )?;
        }
    }
    Ok(())
}
