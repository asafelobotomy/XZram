use std::fs;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use super::fstab::{extra_swapfiles, merge_swap_entries};
use super::index::{get_snapshot, latest_pre_apply_id};
use super::paths::{etc_path, snapshots_root, FSTAB, SYSCTL_FILE, ZRAMSWAP_FILE, ZRAM_CONF};
use super::restore_steps as steps;
use super::types::SnapshotMeta;
use crate::apply::commands::{run_command, run_systemctl};
use crate::apply::types::ApplyResult;
use crate::error::{Result, XzramError};

/// When `XZRAM_ETC_ROOT` is set, restore only mutates files under the fake etc root.
fn hermetic_etc() -> bool {
    std::env::var_os("XZRAM_ETC_ROOT").is_some()
}

pub fn restore_snapshot(id: &str) -> Result<ApplyResult> {
    let meta = get_snapshot(id)?;
    let dir = snapshots_root().join(&meta.id);
    if !dir.exists() {
        return Err(XzramError::NotFound(format!(
            "snapshot directory missing: {id}"
        )));
    }
    let hermetic = hermetic_etc();

    // Decide which swapfiles to remove *before* touching /etc: once fstab is restored it
    // only lists the snapshot's swapfiles, hiding any created since.
    let extra = extra_swapfiles(&meta, hermetic)?;

    // Refusals happen here, before anything changes.
    if !hermetic {
        // A recreated swapfile in a snapshotted btrfs subvolume blocks its snapshots (#31).
        let paths = meta.swapfiles.iter().map(|s| s.path.as_str());
        crate::btrfs_subvol::ensure_restore_swapfiles_placement(paths)?;
        // Swapping everything off brings it all into RAM (the #28 stall `apply` refuses).
        steps::ensure_restore_memory(&meta, &extra)?;
    }

    let mut messages = Vec::new();
    if let Err(err) = restore_into_system(&meta, &dir, &extra, hermetic, &mut messages) {
        if hermetic {
            return Err(err);
        }
        // Past swapoff, a failed step would otherwise leave the machine with no swap.
        let notes = steps::reactivate_swap_after_failure();
        warn!(error = %err, ?notes, "snapshot restore failed; re-enabled swap");
        let note = if notes.is_empty() {
            "swap was turned back on from the configuration now on disk".to_string()
        } else {
            format!(
                "tried to turn swap back on from the configuration now on disk: {}",
                notes.join("; ")
            )
        };
        return Err(err.with_note(&note));
    }

    info!(id = %meta.id, hermetic, "restored snapshot");
    Ok(ApplyResult {
        success: true,
        messages,
    })
}

/// Everything from swapoff on. An error here is answered by re-enabling swap.
fn restore_into_system(
    meta: &SnapshotMeta,
    dir: &Path,
    extra: &[String],
    hermetic: bool,
    messages: &mut Vec<String>,
) -> Result<()> {
    if !hermetic {
        steps::swapoff_managed_swaps(meta, extra)?;
    }
    restore_etc_files(meta, dir, messages)?;

    if hermetic {
        for path in extra {
            messages.push(format!(
                "Would remove swapfile {path} (not in snapshot; skipped in hermetic restore)"
            ));
        }
        return Ok(());
    }

    steps::remove_extra_swapfiles(extra, messages)?;
    steps::recreate_missing_swapfiles(meta, messages)?;

    run_systemctl(&["daemon-reload"])?;
    messages.push("Reloaded systemd".into());

    if meta.artifacts.zram_generator_conf.present {
        let backup = dir.join("zram-generator.conf");
        if backup.exists() {
            steps::restart_zram_units_from_config(&backup)?;
            messages.push("Restarted zram units".into());
        }
    } else if let Some(vendor) = crate::detect::find_zram_generator_config() {
        // No /etc config in the snapshot: the /etc file is gone, so the vendor or runtime
        // config is in effect again. Start what it defines instead of leaving zram off.
        steps::restart_zram_units_from_config(Path::new(&vendor))?;
        messages.push(format!("Restarted zram units from {vendor}"));
    } else {
        steps::stop_all_zram_units()?;
        messages.push("Stopped zram units (no zram configuration in snapshot)".into());
    }

    if meta.artifacts.sysctl.present || etc_path(SYSCTL_FILE).exists() {
        match run_command("sysctl", &["--system"]) {
            Ok(_) => messages.push("Reloaded sysctl".into()),
            Err(e) => messages.push(format!("Could not reload sysctl: {e}")),
        }
    }

    steps::swapon_from_fstab(messages)?;
    crate::zswap::restore_runtime(meta.zswap_enabled, messages)?;
    crate::oom::restore_runtime(meta.oomd_enabled, messages);
    crate::mm_tunables::restore_runtime(meta.mm_runtime.as_ref(), messages);
    Ok(())
}

fn restore_etc_files(meta: &SnapshotMeta, dir: &Path, messages: &mut Vec<String>) -> Result<()> {
    use crate::oom::{OOMD_ROOT_DROPIN as ROOT, OOMD_SESSION_DROPIN as SESSION};
    let a = &meta.artifacts;
    restore_etc_file(
        dir,
        "zram-generator.conf",
        etc_path(ZRAM_CONF),
        a.zram_generator_conf.present,
        messages,
        "zram-generator.conf",
    )?;
    restore_fstab(dir, a.fstab.present, messages)?;
    restore_etc_file(
        dir,
        "99-xzram.conf",
        etc_path(SYSCTL_FILE),
        a.sysctl.present,
        messages,
        "sysctl configuration",
    )?;
    restore_etc_file(
        dir,
        "zramswap",
        etc_path(ZRAMSWAP_FILE),
        a.zramswap.present,
        messages,
        "zram-tools configuration",
    )?;
    let drop_ins = [
        (&a.zswap, crate::zswap::ZSWAP_TMPFILES, "zswap drop-in"),
        (&a.oomd_root, ROOT, "systemd-oomd root policy"),
        (
            &a.oomd_user,
            crate::oom::OOMD_USER_DROPIN,
            "systemd-oomd user policy",
        ),
        (&a.oomd_session, SESSION, "systemd-oomd session guard"),
        (
            &a.mm,
            crate::mm_tunables::MM_TMPFILES,
            "memory tunables drop-in",
        ),
    ];
    for (artifact, relative, label) in drop_ins {
        let filename = super::paths::snapshot_filename(relative);
        restore_etc_file(
            dir,
            filename,
            etc_path(relative),
            artifact.present,
            messages,
            label,
        )?;
    }
    Ok(())
}

pub fn rollback() -> Result<ApplyResult> {
    let id = latest_pre_apply_id()?;
    restore_snapshot(&id)
}

fn restore_etc_file(
    snapshot_dir: &Path,
    filename: &str,
    target: PathBuf,
    was_present: bool,
    messages: &mut Vec<String>,
    label: &str,
) -> Result<()> {
    let backup = snapshot_dir.join(filename);
    if was_present {
        if backup.exists() {
            let bytes = fs::read(&backup)?;
            crate::fsutil::atomic_write(&target, &bytes, 0o644)?;
            messages.push(format!("Restored {label}"));
        }
    } else if target.exists() {
        fs::remove_file(&target)?;
        messages.push(format!("Removed {label} (was absent in snapshot)"));
    }
    Ok(())
}

/// Restore only the swap entries of fstab; mounts added since the snapshot are kept.
fn restore_fstab(snapshot_dir: &Path, was_present: bool, messages: &mut Vec<String>) -> Result<()> {
    let snapshot_content = if was_present {
        match fs::read_to_string(snapshot_dir.join("fstab")) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        }
    } else {
        String::new()
    };

    let target = etc_path(FSTAB);
    let current = if target.try_exists()? {
        fs::read_to_string(&target)?
    } else {
        String::new()
    };
    let merged = merge_swap_entries(&current, &snapshot_content);
    if merged == current {
        return Ok(());
    }
    crate::fsutil::atomic_write(&target, merged.as_bytes(), 0o644)?;
    messages.push("Restored swap entries in /etc/fstab".into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::create::create_snapshot;
    use crate::snapshot::test_lock;
    use crate::snapshot::types::SnapshotTrigger;

    struct TestEnv {
        _data: tempfile::TempDir,
        _etc: tempfile::TempDir,
    }

    fn test_env() -> TestEnv {
        let data = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        TestEnv {
            _data: data,
            _etc: etc,
        }
    }

    fn cleanup_test_env() {
        std::env::remove_var("XZRAM_DATA_DIR");
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn restore_unknown_id_errors() {
        let _guard = test_lock();
        let _env = test_env();
        let err = restore_snapshot("does-not-exist").unwrap_err().to_string();
        assert!(err.contains("not found") || err.contains("NotFound") || err.contains("missing"));
        cleanup_test_env();
    }

    #[test]
    fn restore_writes_etc_files_from_snapshot() {
        let _guard = test_lock();
        let env = test_env();
        let etc = env._etc.path();

        fs::create_dir_all(etc.join("systemd")).unwrap();
        fs::create_dir_all(etc.join("sysctl.d")).unwrap();
        let zram_orig = "[zram0]\nzram-size = ram / 2\n";
        let fstab_orig = "/swapfile none swap sw,pri=10 0 0\n";
        let sysctl_orig = "vm.swappiness = 180\n";
        fs::write(etc.join("systemd/zram-generator.conf"), zram_orig).unwrap();
        fs::write(etc.join("fstab"), fstab_orig).unwrap();
        fs::write(etc.join("sysctl.d/99-xzram.conf"), sysctl_orig).unwrap();

        let meta = create_snapshot(SnapshotTrigger::Manual, Some("restore-test"), None).unwrap();

        fs::write(
            etc.join("systemd/zram-generator.conf"),
            "[zram0]\nzram-size = ram\n",
        )
        .unwrap();
        fs::write(etc.join("fstab"), "# mutated\n").unwrap();
        fs::write(etc.join("sysctl.d/99-xzram.conf"), "vm.swappiness = 60\n").unwrap();

        let result = restore_snapshot(&meta.id).unwrap();
        assert!(result.success);

        assert_eq!(
            fs::read_to_string(etc.join("systemd/zram-generator.conf")).unwrap(),
            zram_orig
        );
        // Non-swap lines added since the snapshot are kept; swap entries come back.
        assert_eq!(
            fs::read_to_string(etc.join("fstab")).unwrap(),
            format!("# mutated\n{fstab_orig}")
        );
        assert_eq!(
            fs::read_to_string(etc.join("sysctl.d/99-xzram.conf")).unwrap(),
            sysctl_orig
        );

        cleanup_test_env();
    }

    #[test]
    fn hermetic_restore_detects_new_swapfile_and_keeps_new_mounts() {
        let _guard = test_lock();
        let env = test_env();
        let etc = env._etc.path();

        let root = "UUID=root / ext4 defaults 0 1\n";
        fs::write(
            etc.join("fstab"),
            format!("{root}/swapfile none swap sw,pri=10 0 0\n"),
        )
        .unwrap();
        let meta = create_snapshot(SnapshotTrigger::PreApply, Some("pre"), None).unwrap();

        // Simulate an apply that added a swapfile, plus an unrelated mount added later.
        fs::write(
            etc.join("fstab"),
            format!(
                "{root}/swapfile none swap sw,pri=10 0 0\n/swap/new none swap sw,pri=5 0 0\n\
                 UUID=data /data ext4 defaults 0 2\n"
            ),
        )
        .unwrap();

        let result = restore_snapshot(&meta.id).unwrap();
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.contains("Would remove swapfile /swap/new")),
            "messages: {:?}",
            result.messages
        );
        assert!(!result.messages.iter().any(|m| m.contains("/swapfile ")));

        let fstab = fs::read_to_string(etc.join("fstab")).unwrap();
        assert!(fstab.contains("UUID=data /data ext4"));
        assert!(fstab.contains("/swapfile none swap"));
        assert!(!fstab.contains("/swap/new"));

        cleanup_test_env();
    }
}
