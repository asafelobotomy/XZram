use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::info;

use crate::dbus_client;

/// Polkit policy annotates only this path (`org.freedesktop.policykit.exec.path`).
const ANNOTATED_HELPER: &str = "/usr/libexec/xzram-helper";

pub(crate) fn run_privileged_pkexec(action: &str, payload: &str) -> anyhow::Result<()> {
    let helper = find_helper_for_pkexec()?;
    let status = Command::new("pkexec")
        .arg(&helper)
        .arg(action)
        .arg(payload)
        .status()?;

    if !status.success() {
        if let Some(err) = xzram::apply::read_last_error() {
            anyhow::bail!("{err}");
        }
        anyhow::bail!("privileged operation failed (pkexec exit {status})");
    }
    Ok(())
}

pub(crate) fn run_privileged(use_dbus: bool, action: &str, payload: &str) -> anyhow::Result<()> {
    if use_dbus {
        match run_via_dbus(action, payload) {
            Ok(()) => return Ok(()),
            Err(e) if dbus_unavailable(&e) => {
                info!(error = %e, "D-Bus unavailable, falling back to pkexec");
            }
            Err(e) => return Err(e),
        }
    }

    run_privileged_pkexec(action, payload)
}

fn dbus_unavailable(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("xzramd not running")
        || msg.contains("name has no owner")
        || msg.contains("service unknown")
        || msg.contains("disconnected")
        || msg.contains("failed to connect")
        || msg.contains("no such name")
}

fn run_via_dbus(action: &str, payload: &str) -> anyhow::Result<()> {
    if !dbus_client::is_available() {
        anyhow::bail!("xzramd not running");
    }
    dbus_client::call(action, payload)
}

/// Helper path for pkexec — must match the polkit `exec.path` annotation unless
/// explicitly opted into a development binary via `XZRAM_ALLOW_DEV_HELPER=1`.
pub(crate) fn find_helper_for_pkexec() -> anyhow::Result<String> {
    if std::env::var_os("XZRAM_ALLOW_DEV_HELPER").is_some() {
        if let Ok(dev) = std::env::var("XZRAM_DEV_HELPER") {
            let path = PathBuf::from(&dev);
            if path.is_absolute() && path.exists() {
                info!(?path, "using XZRAM_DEV_HELPER (XZRAM_ALLOW_DEV_HELPER set)");
                return Ok(dev);
            }
            anyhow::bail!("XZRAM_DEV_HELPER must be an absolute existing path");
        }
    }

    if Path::new(ANNOTATED_HELPER).exists() {
        return Ok(ANNOTATED_HELPER.into());
    }

    anyhow::bail!(
        "xzram-helper not found at {ANNOTATED_HELPER}; install the xzram package \
         (or set XZRAM_ALLOW_DEV_HELPER=1 and XZRAM_DEV_HELPER for development)"
    )
}
