use std::collections::HashSet;
use std::fs;
use std::path::Path;

use tracing::info;

use super::index::{get_snapshot, latest_pre_apply_id};
use super::paths::{etc_path, snapshots_root, FSTAB, SYSCTL_FILE, ZRAMSWAP_FILE, ZRAM_CONF};
use super::types::SnapshotMeta;
use crate::apply::commands::{
    deactivate_swap_path, deactivate_zram_device, restart_zram_setup_unit, run_command,
    run_systemctl, stop_zram_setup_unit,
};
use crate::apply::types::ApplyResult;
use crate::backend::available_swapfile_backend;
use crate::error::{Result, XzramError};
use crate::swapfile_btrfs;

pub fn restore_snapshot(id: &str) -> Result<ApplyResult> {
    let meta = get_snapshot(id)?;
    let dir = snapshots_root().join(&meta.id);
    if !dir.exists() {
        return Err(XzramError::NotFound(format!(
            "snapshot directory missing: {id}"
        )));
    }

    let mut messages = Vec::new();

    swapoff_managed_swaps(&meta)?;

    restore_etc_file(
        &dir,
        "zram-generator.conf",
        etc_path(ZRAM_CONF),
        meta.artifacts.zram_generator_conf.present,
        &mut messages,
        "zram-generator.conf",
    )?;

    restore_etc_file(
        &dir,
        "fstab",
        etc_path(FSTAB),
        meta.artifacts.fstab.present,
        &mut messages,
        "/etc/fstab",
    )?;

    restore_etc_file(
        &dir,
        "99-xzram.conf",
        etc_path(SYSCTL_FILE),
        meta.artifacts.sysctl.present,
        &mut messages,
        "sysctl configuration",
    )?;

    restore_etc_file(
        &dir,
        "zramswap",
        etc_path(ZRAMSWAP_FILE),
        meta.artifacts.zramswap.present,
        &mut messages,
        "zram-tools configuration",
    )?;

    cleanup_extra_swapfiles(&meta, &mut messages)?;
    recreate_missing_swapfiles(&meta, &mut messages)?;

    run_systemctl(&["daemon-reload"])?;
    messages.push("Reloaded systemd".into());

    if meta.artifacts.zram_generator_conf.present {
        let backup = dir.join("zram-generator.conf");
        if backup.exists() {
            restart_zram_units_from_config(backup.to_str().unwrap())?;
            messages.push("Restarted zram units".into());
        }
    } else {
        for i in 0..8 {
            stop_zram_setup_unit(&format!("zram{i}"))?;
        }
        messages.push("Stopped zram units (absent in snapshot)".into());
    }

    if meta.artifacts.sysctl.present || etc_path(SYSCTL_FILE).exists() {
        let _ = run_command("sysctl", &["--system"]);
        messages.push("Reloaded sysctl".into());
    }

    swapon_from_fstab(&mut messages)?;

    info!(id = %meta.id, "restored snapshot");
    Ok(ApplyResult {
        success: true,
        messages,
    })
}

pub fn rollback() -> Result<ApplyResult> {
    let id = latest_pre_apply_id()?;
    restore_snapshot(&id)
}

fn swapoff_managed_swaps(meta: &SnapshotMeta) -> Result<()> {
    for swap in &meta.swapfiles {
        deactivate_swap_path(&swap.path)?;
    }
    for device in &meta.zram_devices {
        deactivate_zram_device(&device.name)?;
    }
    for i in 0..8 {
        deactivate_zram_device(&format!("zram{i}"))?;
    }
    Ok(())
}

fn restore_etc_file(
    snapshot_dir: &Path,
    filename: &str,
    target: std::path::PathBuf,
    was_present: bool,
    messages: &mut Vec<String>,
    label: &str,
) -> Result<()> {
    let backup = snapshot_dir.join(filename);
    if was_present {
        if backup.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&backup, &target)?;
            messages.push(format!("Restored {label}"));
        }
    } else if target.exists() {
        fs::remove_file(&target)?;
        messages.push(format!("Removed {label} (was absent in snapshot)"));
    }
    Ok(())
}

fn cleanup_extra_swapfiles(meta: &SnapshotMeta, messages: &mut Vec<String>) -> Result<()> {
    let snapshot_paths: HashSet<&str> = meta.swapfiles.iter().map(|s| s.path.as_str()).collect();
    let backend = available_swapfile_backend();
    let current = backend.list().unwrap_or_default();
    for sf in current {
        if !snapshot_paths.contains(sf.path.as_str()) {
            deactivate_swap_path(&sf.path)?;
            if Path::new(&sf.path).exists() {
                fs::remove_file(&sf.path)?;
                messages.push(format!("Removed swapfile {} (not in snapshot)", sf.path));
            }
        }
    }
    Ok(())
}

fn recreate_missing_swapfiles(meta: &SnapshotMeta, messages: &mut Vec<String>) -> Result<()> {
    for sf in &meta.swapfiles {
        let path = Path::new(&sf.path);
        if path.exists() {
            continue;
        }
        swapfile_btrfs::create_allocated_swapfile(path, sf.size_mb)?;
        fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        messages.push(format!(
            "Recreated swapfile {} ({} MiB)",
            sf.path, sf.size_mb
        ));
    }
    Ok(())
}

fn swapon_from_fstab(messages: &mut Vec<String>) -> Result<()> {
    let fstab = etc_path(FSTAB);
    if !fstab.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(fstab)?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 3 && parts[2] == "swap" {
            let device = parts[0];
            if device.starts_with('/') && !device.starts_with("/dev/") {
                let priority = crate::backend::swapfile::parse_fstab_priority(
                    parts.get(3).copied().unwrap_or("defaults"),
                );
                let _ = run_command("swapon", &["-p", &priority.to_string(), device]);
            }
        }
    }
    messages.push("Activated swapfiles from fstab".into());
    Ok(())
}

fn restart_zram_units_from_config(path: &str) -> Result<()> {
    let conf = crate::config::parse_zram_generator_conf(path)?;
    for device in &conf.devices {
        restart_zram_setup_unit(&device.name)?;
    }
    Ok(())
}
