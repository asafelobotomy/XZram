//! Crash-safe file writes for root-owned configuration and state.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::Result;

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
    fs::create_dir_all(parent)?;
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
