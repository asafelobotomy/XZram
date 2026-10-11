use std::fs;
use std::path::{Path, PathBuf};

use tracing::info;

use super::fstab::{extra_swapfiles, merge_swap_entries, swapfile_entries};
use super::index::{get_snapshot, latest_pre_apply_id};
use super::paths::{etc_path, snapshots_root, FSTAB, SYSCTL_FILE, ZRAMSWAP_FILE, ZRAM_CONF};
use super::types::SnapshotMeta;
use crate::apply::commands::{
    deactivate_swap_path, deactivate_zram_device, restart_zram_setup_unit, run_command,
    run_systemctl, stop_zram_setup_unit,
};
use crate::apply::types::ApplyResult;
use crate::error::{Result, XzramError};
use crate::swapfile_btrfs;

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

    let mut messages = Vec::new();
    let hermetic = hermetic_etc();

    // Decide which swapfiles to remove *before* touching /etc: once fstab is restored it
    // only lists the snapshot's swapfiles, hiding any created since.
    let extra = extra_swapfiles(&meta, hermetic)?;

    if !hermetic {
        swapoff_managed_swaps(&meta, &extra)?;
    }

    restore_etc_file(
        &dir,
        "zram-generator.conf",
        etc_path(ZRAM_CONF),
        meta.artifacts.zram_generator_conf.present,
        &mut messages,
        "zram-generator.conf",
    )?;

    restore_fstab(&dir, meta.artifacts.fstab.present, &mut messages)?;

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

    if hermetic {
        for path in &extra {
            messages.push(format!(
                "Would remove swapfile {path} (not in snapshot; skipped in hermetic restore)"
            ));
        }
    } else {
        remove_extra_swapfiles(&extra, &mut messages)?;
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
    }

    info!(id = %meta.id, hermetic, "restored snapshot");
    Ok(ApplyResult {
        success: true,
        messages,
    })
}

pub fn rollback() -> Result<ApplyResult> {
    let id = latest_pre_apply_id()?;
    restore_snapshot(&id)
}

fn swapoff_managed_swaps(meta: &SnapshotMeta, extra: &[String]) -> Result<()> {
    for swap in &meta.swapfiles {
        deactivate_swap_path(&swap.path)?;
    }
    for path in extra {
        deactivate_swap_path(path)?;
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

fn remove_extra_swapfiles(extra: &[String], messages: &mut Vec<String>) -> Result<()> {
    for path in extra {
        deactivate_swap_path(path)?;
        if Path::new(path).exists() {
            fs::remove_file(path)?;
            messages.push(format!("Removed swapfile {path} (not in snapshot)"));
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
        crate::backend::swapfile::restorecon_if_present(path);
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
    for (device, options) in swapfile_entries(&content) {
        let priority = crate::backend::swapfile::parse_fstab_priority(&options);
        let _ = run_command("swapon", &["-p", &priority.to_string(), "--", &device]);
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
