use xzram::apply::{pending_is_empty, ApplyOptions};

use crate::privileged::{run_privileged, run_privileged_pkexec};
use crate::store_read::load_pending_readable;

/// `{}` for a plain apply, else `{"options": {...}}` (helper `apply` bundle).
pub(crate) fn options_payload(opts: ApplyOptions) -> anyhow::Result<String> {
    if opts == ApplyOptions::default() {
        return Ok("{}".into());
    }
    Ok(serde_json::to_string(
        &serde_json::json!({ "options": opts }),
    )?)
}

pub(crate) fn apply(dbus: bool, opts: ApplyOptions) -> anyhow::Result<()> {
    match load_pending_readable()? {
        None => {
            println!("No pending configuration to apply");
            Ok(())
        }
        Some(pending) if pending_is_empty(&pending) => {
            println!("Pending configuration is empty; nothing to apply");
            Ok(())
        }
        Some(_) => run_privileged(dbus, "apply", &options_payload(opts)?),
    }
}

pub(crate) fn rollback(dbus: bool) -> anyhow::Result<()> {
    run_privileged(dbus, "rollback", "{}")
}

pub(crate) fn pending_clear(dbus: bool) -> anyhow::Result<()> {
    run_privileged(dbus, "pending.clear", "{}")
}

pub(crate) fn daemon_start() -> anyhow::Result<()> {
    run_privileged_pkexec("daemon.start", "{}")
}
