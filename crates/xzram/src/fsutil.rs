//! Crash-safe file writes for root-owned configuration and state.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::Result;

/// Best-effort `chmod`: explicit modes make the result independent of the caller's umask
/// (xzramd runs with `UMask=0077`).
pub fn set_mode(path: &Path, mode: u32) {
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

/// Mode of directories `atomic_write` has to create. They hold config that non-root readers
/// need (e.g. `/etc/systemd/user/session.slice.d/` is read by each user's systemd manager,
/// and `doctor` runs unprivileged), so the caller's umask (xzramd: `UMask=0077`) must not
/// make them root-only.
const NEW_DIR_MODE: u32 = 0o755;

/// `create_dir_all`, but every directory it creates gets [`NEW_DIR_MODE`] whatever the umask.
/// Directories that already exist are left as they are.
fn create_parent_dirs(dir: &Path) -> Result<()> {
    let missing: Vec<&Path> = dir
        .ancestors()
        .take_while(|p| !p.as_os_str().is_empty() && fs::symlink_metadata(p).is_err())
        .collect();
    for p in missing.into_iter().rev() {
        match fs::create_dir(p) {
            Ok(()) => set_mode(p, NEW_DIR_MODE),
            // Created concurrently by someone else: theirs, leave its mode alone.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Atomically replace `path` with `contents`.
///
/// Writes a sibling temp file (`O_EXCL`, never following symlinks), fsyncs it, renames it
/// over `path`, then fsyncs the parent directory. A crash leaves either the old or the new
/// file, never a truncated one. Keeps the existing file's mode when present, else `default_mode`.
pub fn atomic_write(path: &Path, contents: &[u8], default_mode: u32) -> Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    create_parent_dirs(parent)?;
    let mode = fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.file_type().is_file())
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(default_mode);

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = parent.join(format!(".{name}.xzram-tmp.{}", std::process::id()));
    let _ = fs::remove_file(&tmp);

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        file.write_all(contents)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xzramd runs with `UMask=0077`; directories atomic_write creates must still be 0755
    /// (and existing ones untouched). umask is per process, so this re-runs itself in a
    /// child instead of changing it under the other test threads.
    #[test]
    fn new_parent_dirs_are_readable_under_daemon_umask() {
        const CHILD: &str = "XZRAM_TEST_FSUTIL_UMASK_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "fsutil::tests::new_parent_dirs_are_readable_under_daemon_umask",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success(), "child test run failed: {status}");
            return;
        }
        // SAFETY: umask has no preconditions; this child process runs only this test.
        unsafe { libc::umask(0o077) };
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("systemd");
        fs::create_dir(&existing).unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o750)).unwrap();
        let p = existing.join("user/session.slice.d/10-xzram-oomd.conf");
        atomic_write(&p, b"[Slice]\n", 0o644).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&existing.join("user")), 0o755);
        assert_eq!(mode(&existing.join("user/session.slice.d")), 0o755);
        assert_eq!(mode(&p), 0o644);
        assert_eq!(
            mode(&existing),
            0o750,
            "existing directories keep their mode"
        );
    }

    #[test]
    fn writes_and_preserves_mode() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.conf");
        atomic_write(&p, b"one", 0o644).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "one");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        atomic_write(&p, b"two", 0o644).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "two");
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
