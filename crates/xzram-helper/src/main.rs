use std::process;

use clap::Parser;
use serde::Deserialize;
use tracing::info;
use tracing_subscriber::EnvFilter;
use xzram::apply::{
    self, apply, apply_pending, clear_pending, stage, ApplyRequest, PendingConfig, SwapfileConfig,
    ZramConfig,
};
use xzram::backend::{available_swapfile_backend, ensure_zram_backend};
use xzram::migrate::migrate_from_zram_tools;
use xzram::snapshot::{self, SnapshotTrigger};
use xzram::sysctl::{self, SysctlValues};

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
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    if let Err(e) = run(&args.action, &args.payload) {
        eprintln!("xzram-helper: {e}");
        process::exit(1);
    }
}

fn run(action: &str, payload: &str) -> xzram::Result<()> {
    info!(action, "helper action");
    match action {
        "stage" => {
            let partial: PendingConfig = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            stage(&partial)?;
            println!("Configuration staged");
        }
        "pending.clear" => {
            clear_pending()?;
            println!("Pending configuration cleared");
        }
        "zram.configure" => {
            let config: ZramConfig = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = ensure_zram_backend()?;
            backend.configure(&config)?;
            backend.apply()?;
            println!("ZRAM configured");
        }
        "zram.disable" => {
            let backend = ensure_zram_backend()?;
            backend.disable()?;
            println!("ZRAM disabled");
        }
        "swapfile.create" => {
            let config: SwapfileConfig = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_swapfile_backend();
            if !backend.is_available() {
                return Err(xzram::XzramError::Backend(
                    "swapfile backend is not available".into(),
                ));
            }
            backend.create(&config)?;
            println!("Swapfile created: {}", config.path);
        }
        "swapfile.resize" => {
            #[derive(Deserialize)]
            struct ResizePayload {
                path: String,
                size_mb: u64,
            }
            let p: ResizePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_swapfile_backend();
            if !backend.is_available() {
                return Err(xzram::XzramError::Backend(
                    "swapfile backend is not available".into(),
                ));
            }
            backend.resize(&p.path, p.size_mb)?;
            println!("Swapfile resized: {}", p.path);
        }
        "swapfile.remove" => {
            #[derive(Deserialize)]
            struct RemovePayload {
                path: String,
            }
            let p: RemovePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_swapfile_backend();
            if !backend.is_available() {
                return Err(xzram::XzramError::Backend(
                    "swapfile backend is not available".into(),
                ));
            }
            backend.remove(&p.path)?;
            println!("Swapfile removed: {}", p.path);
        }
        "swapfile.prepare" => {
            #[derive(Deserialize)]
            struct PreparePayload {
                path: String,
                #[serde(default)]
                mkdir_parents: bool,
            }
            let p: PreparePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let status = xzram::swapfile_btrfs::prepare_nodatacow(
                std::path::Path::new(&p.path),
                p.mkdir_parents,
            )?;
            println!(
                "{}",
                serde_json::to_string(&status)
                    .map_err(|e| xzram::XzramError::Parse(e.to_string()))?
            );
        }
        "swap.activate" => {
            #[derive(Deserialize)]
            struct SwapPayload {
                action: String,
                device: String,
            }
            let p: SwapPayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            match p.action.as_str() {
                "on" => {
                    apply::run_command("swapon", &[&p.device])?;
                }
                "off" => {
                    apply::run_command("swapoff", &[&p.device])?;
                }
                _ => {
                    return Err(xzram::XzramError::Validation("unknown swap action".into()));
                }
            }
            println!("Swap {}: {}", p.action, p.device);
        }
        "sysctl.set" => {
            let values: SysctlValues = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            sysctl::set(&values)?;
            println!("Sysctl values applied");
        }
        "zram.migrate" => {
            let pending = migrate_from_zram_tools()?;
            stage(&pending)?;
            println!("Staged migration from zram-tools; run 'xzram apply' to apply");
        }
        "apply" => {
            let result = if payload == "{}" {
                apply_pending()?
            } else {
                let request: ApplyRequest = serde_json::from_str(payload)
                    .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
                apply(&request)?
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
            let trigger = SnapshotTrigger::from_str(&p.trigger)?;
            let meta = snapshot::create_snapshot(trigger, p.label.as_deref(), None)?;
            println!("{}", serde_json::to_string(&meta).map_err(|e| xzram::XzramError::Parse(e.to_string()))?);
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
        "snapshot.list" => {
            let list = snapshot::list_snapshots()?;
            println!("{}", serde_json::to_string(&list).map_err(|e| xzram::XzramError::Parse(e.to_string()))?);
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
