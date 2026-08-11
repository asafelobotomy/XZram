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
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
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
