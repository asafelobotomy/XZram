use tracing::{error, info, warn};

use super::guard::{self, ApplyOptions};
use super::pending::{clear_pending, load_pending, pending_is_empty};
use super::types::{ApplyRequest, ApplyResult, PendingConfig};
use crate::backend::{available_swapfile_backend, available_zram_backend, SwapfileBackendTrait};
use crate::error::{Result, XzramError};
use crate::migrate;
use crate::snapshot::{self, SnapshotTrigger};
use crate::sysctl;

pub fn apply_pending() -> Result<ApplyResult> {
    apply_pending_with(ApplyOptions::default())
}

/// Apply pending.json. A zram restart is memory-checked first ([`guard`]); a refusal
/// leaves everything, pending.json included, untouched.
pub fn apply_pending_with(opts: ApplyOptions) -> Result<ApplyResult> {
    let pending = load_pending()?
        .ok_or_else(|| XzramError::NotFound("No pending configuration to apply".into()))?;
    if pending_is_empty(&pending) {
        return Err(XzramError::Validation(
            "Pending configuration is empty".into(),
        ));
    }

    let warnings = if pending.zram.is_some() || pending.disable_zram {
        guard::precheck_zram_restart(opts)?
    } else {
        Vec::new()
    };
    let pre_apply = snapshot::create_snapshot(
        SnapshotTrigger::PreApply,
        Some(&snapshot::label_from_pending(&pending)),
        Some(&pending),
    )?;
    let mut result = match apply_from_pending(&pending, opts) {
        Ok(result) => result,
        Err(err) => return Err(rollback_after_failure(err, &pre_apply.id)),
    };
    result.messages.splice(0..0, warnings);
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
    stage_then_apply_with(pending, ApplyOptions::default())
}

pub fn stage_then_apply_with(pending: &PendingConfig, opts: ApplyOptions) -> Result<ApplyResult> {
    crate::validation::validate_staged_pending(pending)?;
    super::pending::stage(pending)?;
    apply_pending_with(opts)
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

pub(crate) fn apply_from_pending(
    pending: &PendingConfig,
    opts: ApplyOptions,
) -> Result<ApplyResult> {
    let request = ApplyRequest {
        zram: pending.zram.clone(),
        swapfile: pending.swapfile.clone(),
        disable_zram: pending.disable_zram,
        remove_swapfile: pending.remove_swapfile.clone(),
    };
    let mut result = apply_unchecked(&request, opts)?;

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

    if pending.enable_oomd {
        result.messages.extend(crate::oom::enable_oomd()?);
    }

    if let Some(mm) = pending.mm.as_ref().filter(|m| !m.is_empty()) {
        result.messages.extend(crate::mm_tunables::apply(mm)?);
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
    apply_with(request, ApplyOptions::default())
}

/// Legacy direct request, memory-checked like [`apply_pending_with`].
pub fn apply_with(request: &ApplyRequest, opts: ApplyOptions) -> Result<ApplyResult> {
    let warnings = if request.zram.is_some() || request.disable_zram {
        guard::precheck_zram_restart(opts)?
    } else {
        Vec::new()
    };
    let mut result = apply_unchecked(request, opts)?;
    result.messages.splice(0..0, warnings);
    Ok(result)
}

fn apply_unchecked(request: &ApplyRequest, opts: ApplyOptions) -> Result<ApplyResult> {
    let mut messages = Vec::new();

    if request.disable_zram {
        let backend = available_zram_backend()?;
        ensure_backend_available(backend.as_ref())?;
        messages.extend(guard::disable_zram(backend.as_ref(), opts)?);
    } else if let Some(ref zram) = request.zram {
        let backend = available_zram_backend()?;
        ensure_backend_available(backend.as_ref())?;
        messages.extend(guard::configure_zram(backend.as_ref(), zram, opts)?);
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
