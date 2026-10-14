//! Userspace OOM protection via systemd-oomd.
//!
//! When memory and swap run out, the kernel OOM killer only fires after the desktop has
//! thrashed for minutes. systemd-oomd acts earlier on PSI pressure and swap usage, but it
//! only watches units with `ManagedOOM*=kill`. Fedora and Ubuntu ship those drop-ins; Arch
//! and Manjaro do not, so enabling the service alone does nothing. Apply writes both
//! drop-ins (same policy as Fedora's systemd-oomd-defaults) and enables the service;
//! removing the drop-ins and disabling the service (snapshot restore) undoes it.
//!
//! The user@.service pressure policy picks a cgroup inside the user manager to kill. Ubuntu
//! 26.10 dropped it after it took down whole sessions, so a user-manager drop-in marks
//! `session.slice` (compositor, shell, core session services) `ManagedOOMPreference=avoid`:
//! apps in `app.slice` are killed first.

mod probe;

pub use probe::{
    oom_kills_previous_boot, oom_kills_this_boot, parse_psi_memory, probe, MemoryPressure, OomProbe,
};

use crate::apply::commands::{run_command, run_systemctl};
use crate::error::Result;
use crate::snapshot::paths::etc_path;

pub(crate) const OOMD_SERVICE: &str = "systemd-oomd.service";
pub(crate) const OOMD_ROOT_DROPIN: &str = "systemd/system/-.slice.d/10-xzram-oomd.conf";
pub(crate) const OOMD_USER_DROPIN: &str = "systemd/system/user@.service.d/10-xzram-oomd.conf";
/// User-manager unit drop-in (applies to every user's `session.slice`).
pub(crate) const OOMD_SESSION_DROPIN: &str = "systemd/user/session.slice.d/10-xzram-oomd.conf";

const ROOT_CONTENT: &str = "# Managed by xzram: when swap is nearly exhausted, kill the \
                            cgroup using the most swap\n[Slice]\nManagedOOMSwap=kill\n";
const USER_CONTENT: &str = "# Managed by xzram: kill runaway user apps under sustained \
                            memory pressure instead of freezing the desktop\n[Service]\n\
                            ManagedOOMMemoryPressure=kill\nManagedOOMMemoryPressureLimit=50%\n";
const SESSION_CONTENT: &str = "# Managed by xzram: kill apps before the compositor, shell \
                               and core session services\n[Slice]\nManagedOOMPreference=avoid\n";

/// Write the ManagedOOM drop-ins, then enable and start systemd-oomd.
pub fn enable_oomd() -> Result<Vec<String>> {
    let mut messages = Vec::new();
    for (relative, content) in [
        (OOMD_ROOT_DROPIN, ROOT_CONTENT),
        (OOMD_USER_DROPIN, USER_CONTENT),
        (OOMD_SESSION_DROPIN, SESSION_CONTENT),
    ] {
        let path = etc_path(relative);
        crate::fsutil::atomic_write(&path, content.as_bytes(), 0o644)?;
        messages.push(format!("Wrote systemd-oomd policy {}", path.display()));
    }

    // Skip live systemd changes under hermetic etc roots (unit tests / fixtures).
    if std::env::var_os("XZRAM_ETC_ROOT").is_none() {
        run_systemctl(&["daemon-reload"])?;
        run_systemctl(&["enable", "--now", OOMD_SERVICE])?;
        messages.push("Enabled and started systemd-oomd".into());
        reload_user_managers(&mut messages);
    }
    Ok(messages)
}

/// Make running user managers pick up (or drop) the session.slice drop-in now rather than
/// at next login. Best effort: a user without a running manager simply loads it later.
pub(crate) fn reload_user_managers(messages: &mut Vec<String>) {
    let Ok(users) = run_command("loginctl", &["list-users", "--no-legend"]) else {
        return;
    };
    let mut reloaded = 0;
    for name in users.lines().filter_map(|l| l.split_whitespace().nth(1)) {
        let machine = format!("{name}@.host");
        if run_command("systemctl", &["--user", "-M", &machine, "daemon-reload"]).is_ok() {
            reloaded += 1;
        }
    }
    if reloaded > 0 {
        messages.push(format!("Reloaded {reloaded} user manager(s)"));
    }
}

/// `Some(true)` when systemd-oomd is enabled at boot; `None` when not installed / unknown.
pub fn oomd_enabled() -> Option<bool> {
    let output = std::process::Command::new("systemctl")
        .args(["is-enabled", OOMD_SERVICE])
        .output()
        .ok()?;
    match String::from_utf8_lossy(&output.stdout).trim() {
        "enabled" | "enabled-runtime" | "alias" => Some(true),
        "disabled" | "masked" | "static" | "indirect" => Some(false),
        _ => None,
    }
}

/// Disable systemd-oomd again when the snapshot had it disabled. Call after daemon-reload.
pub(crate) fn restore_runtime(snapshot_enabled: Option<bool>, messages: &mut Vec<String>) {
    if snapshot_enabled == Some(false) && oomd_enabled() == Some(true) {
        let _ = run_command(
            "systemctl",
            &["disable", "--now", OOMD_SERVICE, "systemd-oomd.socket"],
        );
        messages.push("Disabled systemd-oomd (disabled in snapshot)".into());
    }
    reload_user_managers(messages);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_writes_both_drop_ins_under_etc_root() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());

        let messages = enable_oomd().unwrap();
        let root = std::fs::read_to_string(etc.path().join(OOMD_ROOT_DROPIN)).unwrap();
        let user = std::fs::read_to_string(etc.path().join(OOMD_USER_DROPIN)).unwrap();
        let session = std::fs::read_to_string(etc.path().join(OOMD_SESSION_DROPIN)).unwrap();
        std::env::remove_var("XZRAM_ETC_ROOT");

        assert!(root.contains("[Slice]\nManagedOOMSwap=kill"));
        assert!(user.contains("[Service]\nManagedOOMMemoryPressure=kill"));
        assert!(user.contains("ManagedOOMMemoryPressureLimit=50%"));
        assert!(session.contains("[Slice]\nManagedOOMPreference=avoid"));
        assert_eq!(messages.len(), 3, "hermetic run must not touch systemd");
    }
}
