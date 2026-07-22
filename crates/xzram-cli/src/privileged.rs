use std::process::Command;

use tracing::info;

use crate::dbus_client;

pub(crate) fn run_privileged_pkexec(action: &str, payload: &str) -> anyhow::Result<()> {
    let helper = find_helper()?;
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

pub(crate) fn find_helper() -> anyhow::Result<String> {
    if let Ok(dev) = std::env::var("XZRAM_DEV_HELPER") {
        if std::path::Path::new(&dev).exists() {
            return Ok(dev);
        }
    }

    for path in [
        "/usr/libexec/xzram-helper",
        "/usr/local/libexec/xzram-helper",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xzram-helper/../../target/release/xzram-helper"
        ),
    ] {
        if std::path::Path::new(path).exists() {
            return Ok(path.into());
        }
    }

    let local_libexec = format!(
        "{}/.local/libexec/xzram-helper",
        std::env::var("HOME").unwrap_or_else(|_| "/root".into())
    );
    if std::path::Path::new(&local_libexec).exists() {
        return Ok(local_libexec);
    }

    anyhow::bail!("xzram-helper not found; install xzram or set XZRAM_DEV_HELPER")
}
