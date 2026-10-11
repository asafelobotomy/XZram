mod direct;
mod error_sink;

use std::process;

use clap::Parser;
use serde::Deserialize;
use tracing::info;
use tracing_subscriber::EnvFilter;
use xzram::apply::{
    self, apply, apply_pending, clear_pending, stage, stage_then_apply,
    stage_with_optional_prepare, ApplyRequest, PendingConfig,
};
use xzram::migrate::migrate_from_zram_tools;
use xzram::snapshot::{self, SnapshotTrigger};

use error_sink::ErrorSink;

/// Privileged helper for xzram — invoked via pkexec with polkit authorization.
#[derive(Parser)]
#[command(name = "xzram-helper", about = "XZram privileged helper")]
struct Args {
    /// Action to perform
    action: String,
    /// JSON payload
    payload: String,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let args = Args::parse();

    // Always start from a clean slate so a stale message can't be blamed on this run.
    let errors = ErrorSink::from_env();
    errors.clear();
    // One privileged run at a time, however it was launched (pkexec or xzramd) and even
    // across xzramd restarts; released when this process exits.
    let result = apply::lock_helper_run().and_then(|_lock| run(&args.action, &args.payload));
    if let Err(e) = result {
        let msg = format!("xzram-helper: {e}");
        errors.write(&msg);
        eprintln!("{msg}");
        process::exit(1);
    }
}

/// Snapshot triggers a caller may request. `pre_apply` is reserved for the automatic
/// snapshots taken before apply / direct actions, because `rollback` restores the newest
/// `pre_apply` snapshot and a forged one would redirect it.
fn caller_snapshot_trigger(raw: &str) -> xzram::Result<SnapshotTrigger> {
    match SnapshotTrigger::parse(raw)? {
        SnapshotTrigger::PreApply => Err(xzram::XzramError::Validation(
            "snapshot trigger 'pre_apply' is reserved for automatic snapshots".into(),
        )),
        t => Ok(t),
    }
}

fn run(action: &str, payload: &str) -> xzram::Result<()> {
    info!(action, "helper action");
    if direct::run(action, payload)? {
        return Ok(());
    }
    match action {
        // Note: `stage` may also prepare the swapfile's parent directory (mkdir + chattr +C)
        // when the payload carries `prepare_swapfile`; the CLI relies on this to get a
        // single polkit prompt, and the `stage` polkit message says so.
        "stage" => {
            #[derive(Deserialize)]
            struct PrepareSpec {
                path: String,
                #[serde(default)]
                mkdir_parents: bool,
            }
            #[derive(Deserialize)]
            struct StageWithPrepare {
                pending: PendingConfig,
                prepare_swapfile: PrepareSpec,
            }
            if let Ok(wrapped) = serde_json::from_str::<StageWithPrepare>(payload) {
                stage_with_optional_prepare(
                    &wrapped.pending,
                    Some((
                        &wrapped.prepare_swapfile.path,
                        wrapped.prepare_swapfile.mkdir_parents,
                    )),
                )?;
            } else {
                let partial: PendingConfig = serde_json::from_str(payload)
                    .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
                stage_with_optional_prepare(&partial, None)?;
            }
            println!("Configuration staged");
        }
        "pending.clear" => {
            clear_pending()?;
            println!("Pending configuration cleared");
        }
        "zram.migrate" => {
            #[derive(Deserialize, Default)]
            struct MigratePayload {
                #[serde(default)]
                apply_now: bool,
            }
            let opts: MigratePayload = if payload.trim().is_empty() || payload.trim() == "{}" {
                MigratePayload::default()
            } else {
                serde_json::from_str(payload)
                    .map_err(|e| xzram::XzramError::Parse(e.to_string()))?
            };
            let pending = migrate_from_zram_tools()?;
            if opts.apply_now {
                let result = stage_then_apply(&pending)?;
                for msg in &result.messages {
                    println!("{msg}");
                }
                println!("Migrated from zram-tools and applied");
            } else {
                stage(&pending)?;
                println!("Staged migration from zram-tools; run 'xzram apply' to apply");
            }
        }
        "apply" => {
            let result = if payload.trim().is_empty() || payload.trim() == "{}" {
                apply_pending()?
            } else {
                #[derive(Deserialize)]
                struct ApplyBundle {
                    pending: PendingConfig,
                }
                if let Ok(bundle) = serde_json::from_str::<ApplyBundle>(payload) {
                    stage_then_apply(&bundle.pending)?
                } else {
                    let request: ApplyRequest = serde_json::from_str(payload)
                        .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
                    apply(&request)?
                }
            };
            for msg in &result.messages {
                println!("{msg}");
            }
        }
        "rollback" => {
            let result = snapshot::rollback()?;
            for msg in &result.messages {
                println!("{msg}");
            }
        }
        "snapshot.create" => {
            #[derive(Deserialize)]
            struct CreatePayload {
                trigger: String,
                #[serde(default)]
                label: Option<String>,
            }
            let p: CreatePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let trigger = caller_snapshot_trigger(&p.trigger)?;
            let meta = snapshot::create_snapshot(trigger, p.label.as_deref(), None)?;
            println!(
                "{}",
                serde_json::to_string(&meta)
                    .map_err(|e| xzram::XzramError::Parse(e.to_string()))?
            );
        }
        "snapshot.restore" => {
            #[derive(Deserialize)]
            struct RestorePayload {
                id: String,
            }
            let p: RestorePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let result = snapshot::restore_snapshot(&p.id)?;
            for msg in &result.messages {
                println!("{msg}");
            }
        }
        "snapshot.delete" => {
            #[derive(Deserialize)]
            struct DeletePayload {
                id: String,
            }
            let p: DeletePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            snapshot::delete_snapshot(&p.id)?;
            println!("Deleted snapshot {}", p.id);
        }
        "snapshot.prune" => {
            #[derive(Deserialize)]
            struct PrunePayload {
                keep: usize,
            }
            let p: PrunePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let removed = snapshot::prune_snapshots(p.keep)?;
            println!("Pruned {removed} snapshot(s)");
        }
        "daemon.start" => {
            apply::run_systemctl(&["daemon-reload"])?;
            apply::run_systemctl(&["enable", "--now", "xzramd.service"])?;
            println!("xzramd enabled and started");
        }
        _ => {
            return Err(xzram::XzramError::Validation(format!(
                "unknown action: {action}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_create_rejects_pre_apply_trigger() {
        assert!(caller_snapshot_trigger("pre_apply").is_err());
        assert!(caller_snapshot_trigger("nope").is_err());
        assert_eq!(
            caller_snapshot_trigger("manual").unwrap(),
            SnapshotTrigger::Manual
        );
        assert_eq!(
            caller_snapshot_trigger("app_open").unwrap(),
            SnapshotTrigger::AppOpen
        );
    }
}
