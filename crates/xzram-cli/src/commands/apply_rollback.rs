use crate::privileged::{run_privileged, run_privileged_pkexec};

pub(crate) fn apply(dbus: bool) -> anyhow::Result<()> {
    run_privileged(dbus, "apply", "{}")
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
