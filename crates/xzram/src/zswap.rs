//! Keep zswap off while zram is the compressed swap tier.
//!
//! zswap is normally built into the kernel, so a modprobe.d option cannot reach it. A
//! tmpfiles.d `w` line writes the sysfs parameter early in boot on any systemd distro, without
//! editing the bootloader; removing the drop-in (snapshot restore) undoes it.

use std::path::Path;

use crate::error::Result;
use crate::snapshot::paths::etc_path;

pub(crate) const ZSWAP_TMPFILES: &str = "tmpfiles.d/xzram-zswap.conf";
const ZSWAP_ENABLED_PARAM: &str = "/sys/module/zswap/parameters/enabled";
const TMPFILES_CONTENT: &str = "# Managed by xzram: keep zswap off so swapped pages reach zram\n\
                                w- /sys/module/zswap/parameters/enabled - - - - N\n";

/// Write the boot-time drop-in, then turn zswap off for the running kernel.
///
/// Pages zswap already holds stay put until faulted back in; the kernel allows this.
pub fn disable() -> Result<Vec<String>> {
    let path = etc_path(ZSWAP_TMPFILES);
    crate::fsutil::atomic_write(&path, TMPFILES_CONTENT.as_bytes(), 0o644)?;
    let mut messages = vec![format!("Persisted zswap off via {}", path.display())];

    // Skip live sysfs writes under hermetic etc roots (unit tests / fixtures).
    if std::env::var_os("XZRAM_ETC_ROOT").is_none() && Path::new(ZSWAP_ENABLED_PARAM).exists() {
        set_runtime(false)?;
        messages.push("Disabled zswap for the running kernel".into());
    }
    Ok(messages)
}

pub(crate) fn set_runtime(enabled: bool) -> Result<()> {
    std::fs::write(ZSWAP_ENABLED_PARAM, if enabled { "Y" } else { "N" })?;
    Ok(())
}

/// Re-enable zswap when the snapshot had it on and nothing persistent keeps it off now.
pub(crate) fn restore_runtime(
    snapshot_enabled: Option<bool>,
    messages: &mut Vec<String>,
) -> Result<()> {
    if snapshot_enabled == Some(true)
        && crate::checks::zswap_enabled() == Some(false)
        && !persistently_disabled()
    {
        set_runtime(true)?;
        messages.push("Re-enabled zswap (enabled in snapshot)".into());
    }
    Ok(())
}

/// Whether the xzram drop-in or the kernel cmdline keeps zswap off across reboots.
pub fn persistently_disabled() -> bool {
    etc_path(ZSWAP_TMPFILES).exists()
        || std::fs::read_to_string("/proc/cmdline")
            .map(|c| cmdline_disables_zswap(&c))
            .unwrap_or(false)
}

/// Last `zswap.enabled=` wins, matching kernel parameter parsing.
pub fn cmdline_disables_zswap(cmdline: &str) -> bool {
    cmdline
        .split_whitespace()
        .rev()
        .find_map(|arg| arg.strip_prefix("zswap.enabled="))
        .is_some_and(|v| matches!(v, "0" | "n" | "N" | "off"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmdline_parsing_uses_last_value() {
        assert!(cmdline_disables_zswap("quiet zswap.enabled=0"));
        assert!(cmdline_disables_zswap("zswap.enabled=1 zswap.enabled=N"));
        assert!(!cmdline_disables_zswap("zswap.enabled=0 zswap.enabled=1"));
        assert!(!cmdline_disables_zswap("quiet splash"));
    }

    #[test]
    fn disable_writes_tmpfiles_drop_in_under_etc_root() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());

        let messages = disable().unwrap();
        let content = std::fs::read_to_string(etc.path().join(ZSWAP_TMPFILES)).unwrap();
        let persisted = persistently_disabled();
        std::env::remove_var("XZRAM_ETC_ROOT");

        assert!(content.contains("w- /sys/module/zswap/parameters/enabled - - - - N"));
        assert_eq!(messages.len(), 1, "hermetic run must not touch sysfs");
        assert!(persisted);
    }
}
