use tracing::info;

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

    snapshot::create_snapshot(
        SnapshotTrigger::PreApply,
        Some(&snapshot::label_from_pending(&pending)),
        Some(&pending),
    )?;
    let result = apply_from_pending(&pending)?;
    clear_pending()?;
    info!("applied pending configuration");
    Ok(result)
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

    if std::env::var_os("XZRAM_ETC_ROOT").is_none() && migrate::zramswap_config_exists() {
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
    use super::super::pending::write_pending;
    use super::*;
    use crate::apply::test_lock;
    use crate::sysctl::SysctlValues;
    use std::fs;

    #[test]
    fn apply_pending_empty_errors() {
        let _guard = test_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", dir.path());
        write_pending(&PendingConfig::default()).unwrap();
        let err = apply_pending().unwrap_err().to_string();
        assert!(err.contains("empty"));
        std::env::remove_var("XZRAM_DATA_DIR");
    }

    #[test]
    fn apply_from_pending_sysctl_under_etc_root() {
        let _guard = test_lock().lock().unwrap();
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
}
