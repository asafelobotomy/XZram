//! Immediate ("apply now") helper actions that change live swap state.
//!
//! Each takes an automatic `pre_apply` snapshot after validating its payload and before
//! touching the system, exactly like `apply` does, so `rollback` (which restores the newest
//! `pre_apply` snapshot) undoes the most recent change rather than jumping back past it.

use std::path::Path;

use serde::Deserialize;
use xzram::apply::{self, SwapfileConfig, ZramConfig};
use xzram::backend::{available_swapfile_backend, ensure_zram_backend, SwapfileBackendTrait};
use xzram::snapshot::{self, SnapshotTrigger};
use xzram::sysctl::{self, SysctlValues};
use xzram::validation;
use xzram::{Result, XzramError};

fn parse<T: serde::de::DeserializeOwned>(payload: &str) -> Result<T> {
    serde_json::from_str(payload).map_err(|e| XzramError::Parse(e.to_string()))
}

/// Snapshot current state before a direct action mutates it.
fn pre_change_snapshot(action: &str) -> Result<()> {
    let label = format!("Before {action}");
    snapshot::create_snapshot(SnapshotTrigger::PreApply, Some(&label), None)?;
    Ok(())
}

fn swapfile_backend() -> Result<Box<dyn SwapfileBackendTrait>> {
    let backend = available_swapfile_backend();
    if !backend.is_available() {
        return Err(XzramError::Backend(
            "swapfile backend is not available".into(),
        ));
    }
    Ok(backend)
}

/// Run a direct action. Returns `Ok(false)` when `action` is not a direct action.
pub fn run(action: &str, payload: &str) -> Result<bool> {
    match action {
        "zram.configure" => {
            let config: ZramConfig = validation::from_json_strict(payload)?;
            validation::validate_zram_config(&config)?;
            let backend = ensure_zram_backend()?;
            pre_change_snapshot(action)?;
            backend.configure(&config)?;
            backend.apply()?;
            println!("ZRAM configured");
        }
        "zram.disable" => {
            let backend = ensure_zram_backend()?;
            pre_change_snapshot(action)?;
            backend.disable()?;
            println!("ZRAM disabled");
        }
        "swapfile.create" => {
            let value: serde_json::Value = parse(payload)?;
            let config: SwapfileConfig = serde_json::from_value(value.clone())
                .map_err(|e| XzramError::Parse(e.to_string()))?;
            let config = validation::validate_swapfile_config(&config)?;
            let flag = |key: &str| value.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            let prepare = flag("prepare");
            if prepare {
                validation::validate_swapfile_prepare_path(&config.path)?;
            }
            let backend = swapfile_backend()?;
            pre_change_snapshot(action)?;
            if prepare {
                xzram::swapfile_btrfs::prepare_nodatacow(
                    Path::new(&config.path),
                    flag("mkdir_parents"),
                )?;
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
            let p: ResizePayload = parse(payload)?;
            validation::validate_swapfile_resize_path(&p.path, p.size_mb)?;
            let backend = swapfile_backend()?;
            pre_change_snapshot(action)?;
            backend.resize(&p.path, p.size_mb)?;
            println!("Swapfile resized: {}", p.path);
        }
        "swapfile.remove" => {
            #[derive(Deserialize)]
            struct RemovePayload {
                path: String,
            }
            let p: RemovePayload = parse(payload)?;
            validation::validate_swapfile_remove_path(&p.path)?;
            let backend = swapfile_backend()?;
            pre_change_snapshot(action)?;
            backend.remove(&p.path)?;
            println!("Swapfile removed: {}", p.path);
        }
        "swap.activate" => {
            #[derive(Deserialize)]
            struct SwapPayload {
                action: String,
                device: String,
            }
            let p: SwapPayload = parse(payload)?;
            validation::validate_swap_device(&p.device)?;
            let cmd = match p.action.as_str() {
                "on" => "swapon",
                "off" => "swapoff",
                _ => return Err(XzramError::Validation("unknown swap action".into())),
            };
            pre_change_snapshot(action)?;
            apply::run_command(cmd, &["--", &p.device])?;
            println!("Swap {}: {}", p.action, p.device);
        }
        "sysctl.set" => {
            let values: SysctlValues = validation::from_json_strict(payload)?;
            sysctl::validate_sysctl_values(&values)?;
            pre_change_snapshot(action)?;
            sysctl::set(&values)?;
            println!("Sysctl values applied");
        }
        _ => return Ok(false),
    }
    Ok(true)
}
