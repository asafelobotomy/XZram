use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::OwnedMutexGuard;

const ANNOTATED_HELPER: &str = "/usr/libexec/xzram-helper";
/// Hard ceiling enforced by systemd on the transient helper unit. Large swapfile
/// creation on slow disks can take minutes, so this is deliberately generous; when it
/// fires systemd kills the helper and `systemd-run --wait` returns only after that.
const HELPER_RUNTIME_MAX: &str = "RuntimeMaxSec=1h";
/// Per-run error files live here (xzramd's `RuntimeDirectory=xzram`); fallback below.
const RUNTIME_ERROR_DIR: &str = "/run/xzram";
const STATE_ERROR_DIR: &str = "/var/lib/xzram";

static RUN_SEQ: AtomicU64 = AtomicU64::new(0);

/// Run a privileged xzram-helper action outside the xzramd systemd sandbox.
///
/// `ProtectSystem=strict` allows writes only under `/var/lib/xzram`, but apply/restore
/// must modify `/etc`, swapfiles, and sysctl. Spawning a transient unit with sandboxing
/// disabled keeps xzramd hardened while letting the helper perform those writes.
///
/// The caller's `gate` guard is moved into the blocking task and only dropped once
/// `systemd-run --wait` has returned, i.e. after the helper unit has fully stopped. There
/// is no daemon-side timeout: returning early would release the gate while the helper is
/// still mutating system state. The gate only orders calls within this daemon instance; the
/// helper's own run lock (`xzram::apply::lock_helper_run`) also serializes against pkexec
/// helpers and against helpers left running by a previous xzramd instance after a restart.
pub async fn run_helper(
    gate: OwnedMutexGuard<()>,
    action: &str,
    payload: &str,
) -> zbus::fdo::Result<Vec<String>> {
    let action = action.to_string();
    let payload = payload.to_string();
    let join = tokio::task::spawn_blocking(move || {
        let result = run_helper_blocking(&action, &payload);
        drop(gate);
        result
    });
    match join.await {
        Ok(result) => result,
        Err(e) => Err(zbus::fdo::Error::Failed(format!("helper task failed: {e}"))),
    }
}

/// Unique per-call error file path the helper writes to via `XZRAM_ERROR_FILE`.
fn error_file_path() -> PathBuf {
    let dir = if std::fs::create_dir_all(RUNTIME_ERROR_DIR).is_ok() {
        RUNTIME_ERROR_DIR
    } else {
        STATE_ERROR_DIR
    };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    Path::new(dir).join(format!("helper-error-{}-{seq}-{nanos}", std::process::id()))
}

fn take_error_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok();
    let _ = std::fs::remove_file(path);
    text.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn run_helper_blocking(action: &str, payload: &str) -> zbus::fdo::Result<Vec<String>> {
    let helper = locate_helper()?;
    let error_file = error_file_path();
    let _ = std::fs::remove_file(&error_file);
    let setenv = format!("--setenv=XZRAM_ERROR_FILE={}", error_file.display());
    let output = Command::new("systemd-run")
        .args([
            "--wait",
            "--collect",
            "--pipe",
            "--expand-environment=no",
            &setenv,
            "-p",
            "ProtectSystem=no",
            "-p",
            "ProtectHome=no",
            "-p",
            "ProtectKernelTunables=no",
            "-p",
            HELPER_RUNTIME_MAX,
            &helper,
            action,
            payload,
        ])
        .output();
    let helper_error = take_error_file(&error_file);
    let output = output
        .map_err(|e| zbus::fdo::Error::Failed(format!("failed to spawn privileged helper: {e}")))?;

    if !output.status.success() {
        if let Some(err) = helper_error {
            return Err(zbus::fdo::Error::Failed(err));
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(zbus::fdo::Error::Failed(failure_message(
            action, &stderr, &stdout,
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn failure_message(action: &str, stderr: &str, stdout: &str) -> String {
    if stderr.contains("xzram-helper:") {
        stderr.to_string()
    } else if stdout.contains("xzram-helper:") {
        stdout.to_string()
    } else if !stderr.is_empty() && !stderr.contains("Running as unit") {
        stderr.to_string()
    } else if !stdout.is_empty()
        && !stdout.contains("Finished with result")
        && !stdout.contains("Running as unit")
    {
        stdout.to_string()
    } else {
        format!("helper action '{action}' failed (check: journalctl -t xzram-helper -n 20)")
    }
}

fn locate_helper() -> zbus::fdo::Result<String> {
    // Must match polkit `exec.path` annotation in data/io.github.xzram.policy.
    if Path::new(ANNOTATED_HELPER).exists() {
        return Ok(ANNOTATED_HELPER.to_string());
    }
    Err(zbus::fdo::Error::Failed(
        "xzram-helper not found at /usr/libexec/xzram-helper; install the xzram package".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_file_paths_are_unique_and_scoped() {
        let a = error_file_path();
        let b = error_file_path();
        assert_ne!(a, b);
        for p in [&a, &b] {
            let parent = p.parent().unwrap();
            assert!(parent == Path::new(RUNTIME_ERROR_DIR) || parent == Path::new(STATE_ERROR_DIR));
        }
    }

    #[test]
    fn failure_message_prefers_helper_prefix() {
        assert_eq!(
            failure_message("apply", "xzram-helper: boom", ""),
            "xzram-helper: boom"
        );
        assert!(failure_message("apply", "Running as unit x", "").contains("journalctl"));
    }
}
