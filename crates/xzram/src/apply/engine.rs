use tracing::{error, info, warn};

use super::pending::{clear_pending, load_pending, pending_is_empty};
use super::types::{ApplyRequest, ApplyResult, PendingConfig};
use crate::backend::{
    available_swapfile_backend, available_zram_backend, SwapfileBackendTrait, ZramBackendTrait,
};
use crate::error::{Result, XzramError};
use crate::migrate;
use crate::snapshot::{self, SnapshotTrigger};
use crate::sysctl;

pub fn apply_pending() -> Result<ApplyResult> {
    let pending = load_pending()?
        .ok_or_else(|| XzramError::NotFound("No pending configuration to apply".into()))?;
    if pending_is_empty(&pending) {
        return Err(XzramError::Validation(
            "Pending configuration is empty".into(),
        ));
    }

    let pre_apply = snapshot::create_snapshot(
        SnapshotTrigger::PreApply,
        Some(&snapshot::label_from_pending(&pending)),
        Some(&pending),
    )?;
    let result = match apply_from_pending(&pending) {
        Ok(result) => result,
        Err(err) => return Err(rollback_after_failure(err, &pre_apply.id)),
    };
    clear_pending()?;
    info!("applied pending configuration");
    Ok(result)
}

/// Best-effort restore of the pre-apply snapshot after a partial apply.
///
/// Returns `err` (same variant, so callers' error classification still works) extended with
/// the rollback outcome. pending.json is left in place so the user can fix and retry.
fn rollback_after_failure(err: XzramError, snapshot_id: &str) -> XzramError {
    warn!(error = %err, snapshot = snapshot_id, "apply failed; rolling back");
    let outcome = match snapshot::restore_snapshot(snapshot_id) {
        Ok(_) => format!("rolled back to pre-apply snapshot {snapshot_id}"),
        Err(rollback_err) => {
            error!(error = %rollback_err, "automatic rollback failed");
            format!(
                "automatic rollback to snapshot {snapshot_id} FAILED: {rollback_err}; \
                 restore it manually with 'xzram snapshot restore {snapshot_id}'"
            )
        }
    };
    let note = format!("{outcome}; pending configuration kept for retry");
    match err {
        XzramError::Io(e) => XzramError::Io(std::io::Error::new(e.kind(), format!("{e} ({note})"))),
        XzramError::Parse(m) => XzramError::Parse(format!("{m} ({note})")),
        XzramError::Config(m) => XzramError::Config(format!("{m} ({note})")),
        XzramError::Backend(m) => XzramError::Backend(format!("{m} ({note})")),
        XzramError::NotFound(m) => XzramError::NotFound(format!("{m} ({note})")),
        XzramError::Permission(m) => XzramError::Permission(format!("{m} ({note})")),
        XzramError::Validation(m) => XzramError::Validation(format!("{m} ({note})")),
        XzramError::Command(m) => XzramError::Command(format!("{m} ({note})")),
    }
}

/// Stage `pending` then apply it in one privileged session (single polkit challenge).
pub fn stage_then_apply(pending: &PendingConfig) -> Result<ApplyResult> {
    crate::validation::validate_staged_pending(pending)?;
    super::pending::stage(pending)?;
    apply_pending()
}

/// Optionally prepare a swapfile parent, then stage pending (single polkit `stage` challenge).
pub fn stage_with_optional_prepare(
    pending: &PendingConfig,
    prepare: Option<(&str, bool)>,
) -> Result<()> {
    if let Some((path, mkdir_parents)) = prepare {
        crate::validation::validate_swapfile_prepare_path(path)?;
        crate::swapfile_btrfs::prepare_nodatacow(std::path::Path::new(path), mkdir_parents)?;
    }
    crate::validation::validate_staged_pending(pending)?;
    super::pending::stage(pending)?;
    Ok(())
}

pub(crate) fn apply_from_pending(pending: &PendingConfig) -> Result<ApplyResult> {
    let request = ApplyRequest {
        zram: pending.zram.clone(),
        swapfile: pending.swapfile.clone(),
        disable_zram: pending.disable_zram,
        remove_swapfile: pending.remove_swapfile.clone(),
    };
    let mut result = apply(&request)?;

    if let Some(ref resize) = pending.swapfile_resize {
        let backend = available_swapfile_backend();
        ensure_backend_available(backend.as_ref())?;
        SwapfileBackendTrait::resize(backend.as_ref(), &resize.path, resize.size_mb)?;
        result
            .messages
            .push(format!("Resized swapfile {}", resize.path));
    }

    if let Some(ref sysctl) = pending.sysctl {
        sysctl::set(sysctl)?;
        result.messages.push("Applied sysctl values".into());
    }

    for path in &pending.prepare_swapfiles {
        crate::validation::validate_swapfile_prepare_path(path)?;
        crate::swapfile_btrfs::prepare_nodatacow(std::path::Path::new(path), false)?;
        result
            .messages
            .push(format!("Set btrfs nodatacow on the parent of {path}"));
    }

    if pending.disable_zswap {
        result.messages.extend(crate::zswap::disable()?);
    }

    if pending.finalize_zram_tools
        && std::env::var_os("XZRAM_ETC_ROOT").is_none()
        && migrate::zramswap_config_exists()
    {
        let migrate_msgs = migrate::finalize_zram_tools_migration()?;
        result.messages.extend(migrate_msgs);
    }

    Ok(result)
}

pub fn apply(request: &ApplyRequest) -> Result<ApplyResult> {
    let mut messages = Vec::new();

    if request.disable_zram {
        let backend = available_zram_backend()?;
        ensure_backend_available(backend.as_ref())?;
        ZramBackendTrait::disable(backend.as_ref())?;
        messages.push("Disabled zram configuration".into());
    } else if let Some(ref zram) = request.zram {
        let backend = available_zram_backend()?;
        ensure_backend_available(backend.as_ref())?;
        ZramBackendTrait::configure(backend.as_ref(), zram)?;
        ZramBackendTrait::apply(backend.as_ref())?;
        messages.push(format!("Applied zram config for {}", zram.device));
    }

    if let Some(ref path) = request.remove_swapfile {
        let backend = available_swapfile_backend();
        ensure_backend_available(backend.as_ref())?;
        SwapfileBackendTrait::remove(backend.as_ref(), path)?;
        messages.push(format!("Removed swapfile {path}"));
    } else if let Some(ref swapfile) = request.swapfile {
        let backend = available_swapfile_backend();
        ensure_backend_available(backend.as_ref())?;
        SwapfileBackendTrait::create(backend.as_ref(), swapfile)?;
        messages.push(format!("Created swapfile {}", swapfile.path));
    }

    Ok(ApplyResult {
        success: true,
        messages,
    })
}

fn ensure_backend_available(backend: &dyn crate::backend::SwapBackend) -> Result<()> {
    if !backend.is_available() {
        return Err(XzramError::Backend(format!(
            "backend '{}' is not available on this system",
            backend.name()
        )));
    }
    Ok(())
}

pub fn rollback() -> Result<ApplyResult> {
    snapshot::rollback()
}

#[cfg(test)]
mod tests {
    use super::super::pending::{load_pending, write_pending};
    use super::*;
    use crate::apply::test_lock;
    use crate::sysctl::SysctlValues;
    use std::fs;

    #[test]
    fn apply_pending_empty_errors() {
        let _guard = test_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", dir.path());
        write_pending(&PendingConfig::default()).unwrap();
        let err = apply_pending().unwrap_err().to_string();
        assert!(err.contains("empty"));
        std::env::remove_var("XZRAM_DATA_DIR");
    }

    #[test]
    fn failed_apply_rolls_back_and_keeps_pending() {
        let _guard = test_lock();
        let data = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        fs::create_dir_all(etc.path().join("sysctl.d")).unwrap();
        let drop_in = etc.path().join("sysctl.d/99-xzram.conf");
        fs::write(&drop_in, "vm.swappiness = 60\n").unwrap();

        // zram step writes the generator config; the out-of-range sysctl step then fails.
        let pending = PendingConfig {
            zram: Some(crate::apply::types::ZramConfig {
                device: "zram0".into(),
                zram_size: Some("4096".into()),
                zram_resident_limit: None,
                compression_algorithm: Some("zstd".into()),
                swap_priority: Some(100),
                fs_type: None,
                mount_point: None,
            }),
            sysctl: Some(SysctlValues {
                swappiness: Some(999),
                watermark_boost_factor: None,
                watermark_scale_factor: None,
                page_cluster: None,
            }),
            ..Default::default()
        };
        write_pending(&pending).unwrap();
        let err = apply_pending().unwrap_err().to_string();

        let zram_conf_exists = etc.path().join("systemd/zram-generator.conf").exists();
        let drop_in_content = fs::read_to_string(&drop_in).unwrap();
        let pending_kept = load_pending().unwrap().is_some();
        std::env::remove_var("XZRAM_DATA_DIR");
        std::env::remove_var("XZRAM_ETC_ROOT");

        assert!(err.contains("rolled back to pre-apply snapshot"), "{err}");
        assert!(err.contains("pending configuration kept"), "{err}");
        assert!(!zram_conf_exists, "zram config should be rolled back");
        assert_eq!(drop_in_content, "vm.swappiness = 60\n");
        assert!(pending_kept);
    }

    #[test]
    fn apply_from_pending_sysctl_under_etc_root() {
        let _guard = test_lock();
        let data = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());

        let pending = PendingConfig {
            sysctl: Some(SysctlValues {
                swappiness: Some(180),
                watermark_boost_factor: Some(0),
                watermark_scale_factor: Some(125),
                page_cluster: Some(0),
            }),
            ..Default::default()
        };
        let result = apply_from_pending(&pending).unwrap();
        assert!(result.success);
        assert!(result.messages.iter().any(|m| m.contains("sysctl")));

        let drop_in = etc.path().join("sysctl.d/99-xzram.conf");
        let content = fs::read_to_string(&drop_in).unwrap();
        assert!(content.contains("vm.swappiness = 180"));
        assert!(content.contains("vm.page-cluster = 0"));

        std::env::remove_var("XZRAM_DATA_DIR");
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn stage_then_apply_sysctl_bundle() {
        let _guard = test_lock();
        let data = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());

        let pending = PendingConfig {
            sysctl: Some(SysctlValues {
                swappiness: Some(150),
                watermark_boost_factor: Some(0),
                watermark_scale_factor: Some(125),
                page_cluster: Some(0),
            }),
            ..Default::default()
        };
        let result = stage_then_apply(&pending).unwrap();
        assert!(result.success);
        assert!(load_pending().unwrap().is_none());
        let drop_in = etc.path().join("sysctl.d/99-xzram.conf");
        assert!(fs::read_to_string(drop_in)
            .unwrap()
            .contains("vm.swappiness = 150"));

        std::env::remove_var("XZRAM_DATA_DIR");
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn disable_zswap_persists_drop_in_and_rollback_removes_it() {
        let _guard = test_lock();
        let data = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());

        let pending = PendingConfig {
            disable_zswap: true,
            ..Default::default()
        };
        let result = stage_then_apply(&pending).unwrap();
        let drop_in = etc.path().join(crate::zswap::ZSWAP_TMPFILES);
        let written = drop_in.exists();
        rollback().unwrap();
        let after_rollback = drop_in.exists();
        std::env::remove_var("XZRAM_DATA_DIR");
        std::env::remove_var("XZRAM_ETC_ROOT");

        assert!(result.messages.iter().any(|m| m.contains("zswap")));
        assert!(written);
        assert!(!after_rollback, "rollback should remove the zswap drop-in");
    }
}
