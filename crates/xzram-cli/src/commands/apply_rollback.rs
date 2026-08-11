use xzram::apply::pending_is_empty;

use crate::privileged::{run_privileged, run_privileged_pkexec};
use crate::store_read::load_pending_readable;

pub(crate) fn apply(dbus: bool) -> anyhow::Result<()> {
    match load_pending_readable()? {
        None => {
            println!("No pending configuration to apply");
            Ok(())
        }
        Some(pending) if pending_is_empty(&pending) => {
            println!("Pending configuration is empty; nothing to apply");
            Ok(())
        }
        Some(_) => run_privileged(dbus, "apply", "{}"),
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
