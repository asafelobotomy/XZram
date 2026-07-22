use std::path::Path;
use std::process::Command;

/// Run a privileged xzram-helper action outside the xzramd systemd sandbox.
///
/// `ProtectSystem=strict` allows writes only under `/var/lib/xzram`, but apply/restore
/// must modify `/etc`, swapfiles, and sysctl. Spawning a transient unit with sandboxing
/// disabled keeps xzramd hardened while letting the helper perform those writes.
pub fn run_helper(action: &str, payload: &str) -> zbus::fdo::Result<Vec<String>> {
    let helper = locate_helper()?;
    let output = Command::new("systemd-run")
        .args([
            "--wait",
            "--collect",
            "--pipe",
            "-p",
            "ProtectSystem=no",
            "-p",
            "ProtectHome=no",
            "-p",
            "ProtectKernelTunables=no",
            &helper,
            action,
            payload,
        ])
        .output()
        .map_err(|e| zbus::fdo::Error::Failed(format!("failed to spawn privileged helper: {e}")))?;

    if !output.status.success() {
        if let Some(err) = xzram::apply::read_last_error() {
            return Err(zbus::fdo::Error::Failed(err));
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.contains("xzram-helper:") {
            stderr
        } else if stdout.contains("xzram-helper:") {
            stdout
        } else if !stderr.is_empty() && !stderr.contains("Running as unit") {
            stderr
        } else if !stdout.is_empty()
            && !stdout.contains("Finished with result")
            && !stdout.contains("Running as unit")
        {
            stdout
        } else {
            format!("helper action '{action}' failed (check: journalctl -t xzram-helper -n 20)")
        };
        return Err(zbus::fdo::Error::Failed(message));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn locate_helper() -> zbus::fdo::Result<String> {
    const CANDIDATES: &[&str] = &[
        "/usr/libexec/xzram-helper",
        "/usr/local/libexec/xzram-helper",
    ];
    for path in CANDIDATES {
        if Path::new(path).exists() {
            return Ok((*path).to_string());
        }
    }
    Err(zbus::fdo::Error::Failed(
        "xzram-helper not found; install the xzram package".into(),
    ))
}
