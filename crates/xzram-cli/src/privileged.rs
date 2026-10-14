use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::SystemTime;

use tracing::info;

use crate::dbus_client::{self, ServiceUnavailable};
use crate::helper_version::ensure_helper_supports_client;

/// Polkit policy annotates only this path (`org.freedesktop.policykit.exec.path`).
const ANNOTATED_HELPER: &str = "/usr/libexec/xzram-helper";

/// pkexec exit code when the user dismissed the authentication dialog.
const PKEXEC_DISMISSED: i32 = 126;
/// pkexec exit code when the caller is not authorized (or auth could not be obtained).
const PKEXEC_NOT_AUTHORIZED: i32 = 127;

pub(crate) fn run_privileged_pkexec(action: &str, payload: &str) -> anyhow::Result<()> {
    let helper = find_helper_for_pkexec()?;
    ensure_helper_supports_client(&helper)?;
    let spawned_at = SystemTime::now();
    let status = Command::new("pkexec")
        .arg(&helper)
        .arg(action)
        .arg(payload)
        .status()?;

    if !status.success() {
        return Err(pkexec_failure(
            status,
            spawned_at,
            None,
            "privileged operation",
        ));
    }
    Ok(())
}

/// Build the error for a failed pkexec invocation.
///
/// Authorization failures (126/127) never consult `last_error`: the helper did not run,
/// so that file can only hold a stale message from an earlier invocation. Otherwise
/// `last_error` is used only when it was written after `spawned_at`.
pub(crate) fn pkexec_failure(
    status: ExitStatus,
    spawned_at: SystemTime,
    stderr: Option<&str>,
    what: &str,
) -> anyhow::Error {
    match status.code() {
        Some(PKEXEC_DISMISSED) => {
            return anyhow::anyhow!("Authorization cancelled (authentication dialog dismissed)")
        }
        Some(PKEXEC_NOT_AUTHORIZED) => {
            return anyhow::anyhow!("Authorization denied (not authorized for this action)")
        }
        _ => {}
    }
    if let Some(err) = read_fresh_last_error(spawned_at) {
        return anyhow::anyhow!("{err}");
    }
    if let Some(stderr) = stderr.map(str::trim).filter(|s| !s.is_empty()) {
        return anyhow::anyhow!("{stderr}");
    }
    anyhow::anyhow!("{what} failed (pkexec exit {status})")
}

/// `last_error` contents, only if the file was modified at or after `since`.
fn read_fresh_last_error(since: SystemTime) -> Option<String> {
    let modified = std::fs::metadata(xzram::apply::last_error_path())
        .and_then(|m| m.modified())
        .ok()?;
    if !is_fresh(modified, since) {
        return None;
    }
    xzram::apply::read_last_error()
}

fn is_fresh(modified: SystemTime, since: SystemTime) -> bool {
    modified >= since
}

pub(crate) fn run_privileged(use_dbus: bool, action: &str, payload: &str) -> anyhow::Result<()> {
    // Compound payloads need helper argv that D-Bus methods do not yet forward.
    let force_pkexec = matches!(action, "apply" | "stage" | "zram.migrate")
        && payload_needs_helper_passthrough(action, payload);
    if use_dbus && !force_pkexec {
        // xzramd forwards to the same installed helper; check it before going via D-Bus.
        ensure_helper_supports_client(&find_helper_for_pkexec()?)?;
        match run_via_dbus(action, payload) {
            Ok(()) => return Ok(()),
            // Only safe to retry via pkexec when the request never reached xzramd.
            Err(e) if e.downcast_ref::<ServiceUnavailable>().is_some() => {
                info!(error = %e, "D-Bus unavailable, falling back to pkexec");
            }
            Err(e) => return Err(e),
        }
    }

    run_privileged_pkexec(action, payload)
}

fn payload_needs_helper_passthrough(action: &str, payload: &str) -> bool {
    let trimmed = payload.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return false;
    }
    match action {
        "apply" => trimmed.contains("\"pending\""),
        "stage" => trimmed.contains("\"prepare_swapfile\""),
        "zram.migrate" => trimmed.contains("\"apply_now\""),
        _ => false,
    }
}

fn run_via_dbus(action: &str, payload: &str) -> anyhow::Result<()> {
    if !dbus_client::is_available() {
        return Err(ServiceUnavailable("xzramd not running".into()).into());
    }
    dbus_client::call(action, payload)
}

/// Helper path for pkexec — always the polkit `exec.path` annotation in release builds.
/// Debug builds additionally honour `XZRAM_ALLOW_DEV_HELPER=1` + `XZRAM_DEV_HELPER`.
pub(crate) fn find_helper_for_pkexec() -> anyhow::Result<String> {
    if cfg!(debug_assertions) {
        if let Some(dev) = dev_helper_override()? {
            return Ok(dev);
        }
    }

    if Path::new(ANNOTATED_HELPER).exists() {
        return Ok(ANNOTATED_HELPER.into());
    }

    if cfg!(debug_assertions) {
        anyhow::bail!(
            "xzram-helper not found at {ANNOTATED_HELPER}; install the xzram package \
             (or set XZRAM_ALLOW_DEV_HELPER=1 and XZRAM_DEV_HELPER for development)"
        )
    }
    anyhow::bail!("xzram-helper not found at {ANNOTATED_HELPER}; install the xzram package")
}

/// Development-only helper override (never consulted in release builds).
fn dev_helper_override() -> anyhow::Result<Option<String>> {
    if std::env::var_os("XZRAM_ALLOW_DEV_HELPER").is_none() {
        return Ok(None);
    }
    let Ok(dev) = std::env::var("XZRAM_DEV_HELPER") else {
        return Ok(None);
    };
    let path = Path::new(&dev);
    if path.is_absolute() && path.exists() {
        info!(
            ?path,
            "using XZRAM_DEV_HELPER (debug build, XZRAM_ALLOW_DEV_HELPER set)"
        );
        return Ok(Some(dev));
    }
    anyhow::bail!("XZRAM_DEV_HELPER must be an absolute existing path")
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::time::Duration;

    use super::*;

    fn exit(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn pkexec_dismissed_reports_cancelled() {
        let err = pkexec_failure(exit(126), SystemTime::UNIX_EPOCH, Some("stale"), "op");
        assert!(err.to_string().contains("Authorization cancelled"));
    }

    #[test]
    fn pkexec_not_authorized_reports_denied() {
        let err = pkexec_failure(exit(127), SystemTime::UNIX_EPOCH, Some("stale"), "op");
        assert!(err.to_string().contains("Authorization denied"));
    }

    #[test]
    fn other_failures_fall_back_to_stderr() {
        // A far-future spawn time guarantees any on-disk last_error is treated as stale.
        let future = SystemTime::now() + Duration::from_secs(3600);
        let err = pkexec_failure(exit(1), future, Some("boom"), "op");
        assert_eq!(err.to_string(), "boom");
    }

    #[test]
    fn last_error_freshness() {
        let now = SystemTime::now();
        assert!(is_fresh(now, now));
        assert!(is_fresh(now + Duration::from_millis(5), now));
        assert!(!is_fresh(now - Duration::from_secs(1), now));
    }
}
