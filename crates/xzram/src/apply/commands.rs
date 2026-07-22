use crate::error::{Result, XzramError};

pub fn run_systemctl(args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| XzramError::Command(format!("systemctl: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XzramError::Command(format!(
            "systemctl {}: {stderr}",
            args.join(" ")
        )));
    }
    Ok(())
}

pub fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| XzramError::Command(format!("{cmd}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XzramError::Command(format!(
            "{cmd} {}: {stderr}",
            args.join(" ")
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// True when `/proc/swaps` lists this device (e.g. `/dev/zram0` or `zram0`).
pub fn device_is_active_swap(device: &str) -> bool {
    let Ok(content) = std::fs::read_to_string("/proc/swaps") else {
        return false;
    };
    let needle = if device.starts_with('/') {
        device.to_string()
    } else {
        format!("/dev/{device}")
    };
    content.lines().skip(1).any(|line| {
        line.split_whitespace()
            .next()
            .is_some_and(|name| name == needle || name == device)
    })
}

/// Deactivate a zram device so zram-generator can reconfigure it.
///
/// Writing `comp_algorithm` / disksize fails with EBUSY while the device is still
/// swap-active. Call this before restarting `systemd-zram-setup@*.service`.
pub fn deactivate_zram_device(device_name: &str) -> Result<()> {
    let path = if device_name.starts_with('/') {
        device_name.to_string()
    } else {
        format!("/dev/{device_name}")
    };
    deactivate_swap_path(&path)
}

/// Swapoff `path` when it appears in `/proc/swaps`; no-op if inactive.
pub fn deactivate_swap_path(path: &str) -> Result<()> {
    if !device_is_active_swap(path) {
        return Ok(());
    }
    run_command("swapoff", &[path]).map_err(|e| {
        XzramError::Command(format!(
            "cannot modify {path} while it is active swap ({e}). \
             Free memory and retry"
        ))
    })?;
    Ok(())
}

fn reset_zram_device(name: &str) {
    let reset_path = format!("/sys/block/{name}/reset");
    if std::path::Path::new(&reset_path).exists() {
        let _ = std::fs::write(&reset_path, b"1");
    }
}

/// Deactivate, stop, and reset `systemd-zram-setup@<device>` without starting it.
pub fn stop_zram_setup_unit(device_name: &str) -> Result<()> {
    let name = device_name.trim_start_matches("/dev/");
    deactivate_zram_device(name)?;

    let unit = format!("systemd-zram-setup@{name}.service");
    let _ = run_systemctl(&["stop", &unit]);
    reset_zram_device(name);
    let _ = run_systemctl(&["reset-failed", &unit]);
    Ok(())
}

/// Stop + start `systemd-zram-setup@<device>` after deactivating the device.
pub fn restart_zram_setup_unit(device_name: &str) -> Result<()> {
    let name = device_name.trim_start_matches("/dev/");
    stop_zram_setup_unit(name)?;

    let unit = format!("systemd-zram-setup@{name}.service");
    run_systemctl(&["start", &unit]).map_err(|e| {
        XzramError::Command(format!(
            "failed to start {unit}: {e}. \
             Check: journalctl -xeu {unit}"
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_is_active_swap_does_not_panic() {
        let _ = device_is_active_swap("zram0");
        let _ = device_is_active_swap("/dev/zram0");
    }
}
