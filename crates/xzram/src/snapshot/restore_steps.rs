//! The live-system steps of snapshot restore: the memory check, swapoff, removing and
//! recreating swapfiles, swapon, restarting zram, and putting swap back after a failure.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::fstab::swapfile_entries;
use super::paths::{etc_path, FSTAB, ZRAM_CONF};
use super::types::SnapshotMeta;
use crate::apply::commands::{
    deactivate_swap_path, deactivate_zram_device, restart_zram_setup_unit, run_command,
};
use crate::apply::guard::{margin_bytes, MemoryCheck};
use crate::error::{Result, XzramError};
use crate::status::StatusReport;
use crate::swapfile_btrfs;

/// zram units restore stops; zram-generator devices are numbered from 0.
const ZRAM_UNITS: usize = 8;

/// Restore swaps off all zram and every swapfile it manages, so their contents come back
/// into RAM. Same rule as the guard on `apply`: refuse up front when RAM can't take it.
pub(super) fn check_restore_memory(
    status: &StatusReport,
    meta: &SnapshotMeta,
    extra: &[String],
) -> MemoryCheck {
    let files: HashSet<&str> = meta
        .swapfiles
        .iter()
        .map(|s| s.path.as_str())
        .chain(extra.iter().map(String::as_str))
        .collect();
    let incoming = status
        .swaps
        .iter()
        .filter(|s| s.name.starts_with("/dev/zram") || files.contains(s.name.as_str()))
        .map(|s| s.used_bytes)
        .sum();
    MemoryCheck {
        what: "restoring the snapshot (swapping off zram and its swapfiles)".into(),
        incoming_bytes: incoming,
        margin_bytes: margin_bytes(status.memory.mem_total_kb * 1024),
        available_bytes: status.memory.mem_available_kb * 1024,
    }
}

pub(super) fn ensure_restore_memory(meta: &SnapshotMeta, extra: &[String]) -> Result<()> {
    let status = crate::status::status()?;
    let check = check_restore_memory(&status, meta, extra);
    if check.ok() {
        return Ok(());
    }
    Err(XzramError::Validation(check.refusal(
        "restore the snapshot after a reboot, when swap is nearly empty",
    )))
}

pub(super) fn swapoff_managed_swaps(meta: &SnapshotMeta, extra: &[String]) -> Result<()> {
    for swap in &meta.swapfiles {
        deactivate_swap_path(&swap.path)?;
    }
    for path in extra {
        deactivate_swap_path(path)?;
    }
    for device in &meta.zram_devices {
        deactivate_zram_device(&device.name)?;
    }
    for i in 0..ZRAM_UNITS {
        deactivate_zram_device(&format!("zram{i}"))?;
    }
    Ok(())
}

pub(super) fn stop_all_zram_units() -> Result<()> {
    for i in 0..ZRAM_UNITS {
        crate::apply::commands::stop_zram_setup_unit(&format!("zram{i}"))?;
    }
    Ok(())
}

/// Delete swapfiles created since the snapshot. Each path goes through the same checks as
/// `swapfile remove` (allowlist, no symlinks, root-owned parents); anything else is left
/// in place and reported, never deleted.
pub(super) fn remove_extra_swapfiles(extra: &[String], messages: &mut Vec<String>) -> Result<()> {
    for path in extra {
        deactivate_swap_path(path)?;
        if !Path::new(path).exists() {
            continue;
        }
        match crate::validation::validate_swapfile_remove_path(path) {
            Ok(_) => {
                fs::remove_file(path)?;
                messages.push(format!("Removed swapfile {path} (not in snapshot)"));
            }
            Err(e) => messages.push(format!(
                "Left {path} in place (not in snapshot, but not removable by xzram: {e})"
            )),
        }
    }
    Ok(())
}

pub(super) fn recreate_missing_swapfiles(
    meta: &SnapshotMeta,
    messages: &mut Vec<String>,
) -> Result<()> {
    for sf in &meta.swapfiles {
        let path = Path::new(&sf.path);
        if path.exists() {
            continue;
        }
        // Allocates, chmods and relabels (create_allocated_swapfile builds the whole file).
        swapfile_btrfs::create_allocated_swapfile(path, sf.size_mb)?;
        messages.push(format!(
            "Recreated swapfile {} ({} MiB)",
            sf.path, sf.size_mb
        ));
    }
    Ok(())
}

/// swapon every swapfile in fstab. A failure is reported, not fatal: one bad entry must not
/// keep the others off.
pub(super) fn swapon_from_fstab(messages: &mut Vec<String>) -> Result<()> {
    let fstab = etc_path(FSTAB);
    if !fstab.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(fstab)?;
    let mut activated = 0;
    for (path, options) in swapfile_entries(&content) {
        let priority = crate::backend::swapfile::parse_fstab_priority(&options);
        match run_command("swapon", &["-p", &priority.to_string(), "--", &path]) {
            Ok(_) => activated += 1,
            Err(e) => messages.push(format!("Could not activate swapfile {path}: {e}")),
        }
    }
    if activated > 0 {
        messages.push(format!("Activated {activated} swapfile(s) from fstab"));
    }
    Ok(())
}

pub(super) fn restart_zram_units_from_config(path: &Path) -> Result<()> {
    let conf = crate::config::parse_zram_generator_conf(path)?;
    for device in &conf.devices {
        restart_zram_setup_unit(&device.name)?;
    }
    Ok(())
}

/// After a restore failed past swapoff: bring swap back from whatever config is on disk now,
/// so the failure doesn't also leave the machine with no swap. Best effort; returns notes.
pub(super) fn reactivate_swap_after_failure() -> Vec<String> {
    let mut notes = Vec::new();
    let managed = etc_path(ZRAM_CONF);
    let zram_conf = if managed.exists() {
        Some(managed)
    } else {
        crate::detect::find_zram_generator_config().map(Into::into)
    };
    if let Some(conf) = zram_conf {
        if let Err(e) = restart_zram_units_from_config(&conf) {
            notes.push(format!("could not restart zram: {e}"));
        }
    }
    if let Err(e) = swapon_from_fstab(&mut notes) {
        notes.push(format!("could not re-enable swapfiles: {e}"));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::types::{SnapshotArtifacts, SnapshotSwapfile, SnapshotTrigger};
    use crate::status::{MemoryInfo, SwapEntry};

    fn swap(name: &str, used: u64) -> SwapEntry {
        SwapEntry {
            name: name.into(),
            swap_type: "file".into(),
            size_bytes: used,
            used_bytes: used,
            priority: 0,
        }
    }

    #[test]
    fn restore_memory_counts_zram_and_managed_swapfiles_only() {
        const GIB: u64 = 1 << 30;
        let status = StatusReport {
            swaps: vec![
                swap("/dev/zram0", 6 * GIB),
                swap("/swap/swapfile", 3 * GIB),
                swap("/home/me/other.swap", 5 * GIB),
            ],
            zram_devices: vec![],
            memory: MemoryInfo {
                mem_total_kb: 16 << 20,
                mem_available_kb: 4 << 20,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let meta = SnapshotMeta {
            id: "x".into(),
            created_at: "0".into(),
            label: String::new(),
            trigger: SnapshotTrigger::Manual,
            state_hash: String::new(),
            xzram_version: String::new(),
            pending_summary: None,
            artifacts: SnapshotArtifacts::default(),
            swapfiles: vec![SnapshotSwapfile {
                path: "/swap/swapfile".into(),
                size_mb: 3072,
                priority: 10,
                present_on_disk: true,
            }],
            zram_devices: vec![],
            zswap_enabled: None,
            oomd_enabled: None,
            mm_runtime: None,
        };
        let check = check_restore_memory(&status, &meta, &[]);
        // zram0 + the managed swapfile; the unrelated swapfile stays on.
        assert_eq!(check.incoming_bytes, 9 * GIB);
        assert!(!check.ok(), "9 GiB can't fit in 4 GiB available");
    }
}
