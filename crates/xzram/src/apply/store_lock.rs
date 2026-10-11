//! Advisory flock for pending.json and snapshot index mutations.

use std::fs::OpenOptions;
use std::path::PathBuf;

use fs2::FileExt;

use super::pending::data_dir;
use crate::error::Result;

fn store_lock_path() -> PathBuf {
    data_dir().join("store.lock")
}

/// Run `f` while holding an exclusive advisory lock on `{XZRAM_DATA_DIR}/store.lock`.
pub fn with_store_lock<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store_lock_path())?;
    file.lock_exclusive()?;
    let result = f();
    drop(file);
    result
}

/// Held for the whole lifetime of one privileged helper run.
///
/// Serializes every mutation regardless of how the helper was launched (pkexec from the
/// CLI/GUI, or a transient unit from xzramd) and survives xzramd restarts, since the lock
/// belongs to the helper process and is released by the kernel when it exits.
pub struct HelperRunLock {
    _file: std::fs::File,
}

/// Take the helper run lock, waiting (with a notice on stderr) while another run holds it.
pub fn lock_helper_run() -> Result<HelperRunLock> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("helper.lock"))?;
    if file.try_lock_exclusive().is_err() {
        eprintln!("xzram-helper: waiting for another xzram operation to finish...");
        file.lock_exclusive()?;
    }
    Ok(HelperRunLock { _file: file })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_lock_is_exclusive_and_released_on_drop() {
        let _env = crate::test_env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", dir.path());
        let held = lock_helper_run().unwrap();
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("helper.lock"))
            .unwrap();
        assert!(other.try_lock_exclusive().is_err());
        drop(held);
        assert!(other.try_lock_exclusive().is_ok());
        std::env::remove_var("XZRAM_DATA_DIR");
    }
}
