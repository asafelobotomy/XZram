//! Swapfile allocation that never destroys a working swap before its replacement exists.
//!
//! New swap is always built at a hidden sibling (`.<name>.xzram-new`) in the same directory —
//! so it inherits btrfs nodatacow from the parent and can be `rename(2)`d atomically — and
//! only then swapped in. Files are created with `O_EXCL | O_NOFOLLOW` and chmodded through
//! their fd so a planted symlink can never redirect the write.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use super::{btrfs_mkswapfile_available, ensure_ready_for_swapfile, is_btrfs_path};
use super::{parent_directory, prepare_nodatacow};
use crate::apply;
use crate::error::{Result, XzramError};
use crate::validation::fstab::proc_swaps_lists_path;

/// True when `/proc/swaps` lists `path` (escape-decoded) as active swap.
pub fn path_is_active_swap(path: &Path) -> bool {
    fs::read_to_string("/proc/swaps")
        .map(|c| proc_swaps_lists_path(&c, &path.to_string_lossy()))
        .unwrap_or(false)
}

/// `chmod 0600` through an fd opened with `O_NOFOLLOW`; refuses symlinks and non-files.
pub fn chmod_swapfile_nofollow(path: &Path) -> Result<()> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(XzramError::Validation(format!(
            "swapfile path must be a regular file ({})",
            path.display()
        )));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Create a new swapfile at `path` (not activated).
///
/// Refuses when `path` is currently active swap (use resize) or exists as anything other than
/// a regular file. An inactive regular file is atomically replaced.
pub fn create_allocated_swapfile(path: &Path, size_mb: u64) -> Result<()> {
    ensure_nonzero(size_mb)?;
    if path_is_active_swap(path) {
        return Err(XzramError::Validation(format!(
            "{} is already active swap; use `swapfile resize` to change its size",
            path.display()
        )));
    }
    ensure_regular_or_absent(path)?;
    let staged = build_staged(path, size_mb)?;
    if let Err(e) = fs::rename(&staged, path) {
        let _ = fs::remove_file(&staged);
        return Err(e.into());
    }
    sync_parent(path);
    Ok(())
}

/// Resize `path` to `size_mb` and activate it with `priority`.
///
/// The replacement is fully allocated and formatted before the old swap is touched. If that
/// fails (e.g. ENOSPC), the staging file is removed and the old swap stays active. Needs
/// free space for both files at once.
pub fn replace_swapfile(path: &Path, size_mb: u64, priority: i32) -> Result<()> {
    ensure_nonzero(size_mb)?;
    ensure_regular_or_absent(path)?;
    let path_str = path.to_string_lossy().into_owned();
    let was_active = path_is_active_swap(path);
    let staged = build_staged(path, size_mb)?;

    if let Err(e) = apply::deactivate_swap_path(&path_str) {
        let _ = fs::remove_file(&staged);
        return Err(e);
    }
    if let Err(e) = fs::rename(&staged, path) {
        let _ = fs::remove_file(&staged);
        if was_active {
            let _ = apply::run_command("swapon", &["-p", &priority.to_string(), "--", &path_str]);
        }
        return Err(e.into());
    }
    sync_parent(path);
    apply::run_command("swapon", &["-p", &priority.to_string(), "--", &path_str])?;
    Ok(())
}

fn ensure_nonzero(size_mb: u64) -> Result<()> {
    if size_mb == 0 {
        return Err(XzramError::Validation(
            "swapfile size must be greater than 0 MiB".into(),
        ));
    }
    Ok(())
}

fn ensure_regular_or_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.file_type().is_file() => Err(XzramError::Validation(format!(
            "{} exists and is not a regular file; refusing to replace it",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn staging_path(path: &Path) -> Result<PathBuf> {
    let parent = parent_directory(path)?;
    let name = path
        .file_name()
        .ok_or_else(|| XzramError::Validation("invalid swapfile path".into()))?;
    Ok(parent.join(format!(".{}.xzram-new", name.to_string_lossy())))
}

/// Allocate, format, chmod, and relabel a swapfile at the staging sibling of `path`.
fn build_staged(path: &Path, size_mb: u64) -> Result<PathBuf> {
    let staged = staging_path(path)?;
    // Leftover from an interrupted run; never follow or descend into it.
    match fs::symlink_metadata(&staged) {
        Ok(m) if m.is_dir() => {
            return Err(XzramError::Validation(format!(
                "staging path {} is a directory",
                staged.display()
            )))
        }
        Ok(_) => fs::remove_file(&staged)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if let Err(e) = build_swapfile(&staged, size_mb) {
        let _ = fs::remove_file(&staged);
        return Err(e);
    }
    Ok(staged)
}

fn build_swapfile(target: &Path, size_mb: u64) -> Result<()> {
    // Target is absent, so this only creates/marks the parent; a new file in a +C directory
    // inherits nodatacow before any data is written.
    prepare_nodatacow(target, true)?;
    ensure_ready_for_swapfile(target)?;
    let target_str = target.to_string_lossy();

    if is_btrfs_path(target) && btrfs_mkswapfile_available() {
        let size = format!("{size_mb}M");
        apply::run_command(
            "btrfs",
            &["filesystem", "mkswapfile", "--size", &size, &target_str],
        )?;
    } else {
        allocate_swapfile_bytes(target, size_mb)?;
        apply::run_command("mkswap", &[&target_str])?;
    }
    chmod_swapfile_nofollow(target)?;
    crate::backend::swapfile::restorecon_if_present(target);
    Ok(())
}

fn allocate_swapfile_bytes(path: &Path, size_mb: u64) -> Result<()> {
    let size_bytes = size_mb
        .checked_mul(1024 * 1024)
        .and_then(|b| libc::off_t::try_from(b).ok())
        .ok_or_else(|| XzramError::Validation("swapfile size too large".into()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    // SAFETY: valid open fd; fallocate does not touch user memory.
    let rc = unsafe { libc::fallocate(file.as_raw_fd(), 0, 0, size_bytes) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EOPNOTSUPP) | Some(libc::ENOSYS) => write_zeros(&mut file, size_mb)?,
            _ => return Err(err.into()),
        }
    }
    file.sync_all()?;
    Ok(())
}

fn write_zeros(file: &mut File, size_mb: u64) -> Result<()> {
    let chunk = vec![0u8; 1024 * 1024];
    for _ in 0..size_mb {
        file.write_all(&chunk)?;
    }
    Ok(())
}

fn sync_parent(path: &Path) {
    if let Some(dir) = path.parent().and_then(|p| File::open(p).ok()) {
        let _ = dir.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_is_hidden_sibling() {
        assert_eq!(
            staging_path(Path::new("/swap/swapfile")).unwrap(),
            PathBuf::from("/swap/.swapfile.xzram-new")
        );
        assert_eq!(
            staging_path(Path::new("/swapfile")).unwrap(),
            PathBuf::from("/.swapfile.xzram-new")
        );
    }

    #[test]
    fn create_refuses_non_regular_existing() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("swapdir");
        fs::create_dir(&sub).unwrap();
        let err = create_allocated_swapfile(&sub, 1).unwrap_err();
        assert!(err.to_string().contains("not a regular file"), "{err}");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("/etc/hostname", &link).unwrap();
        assert!(create_allocated_swapfile(&link, 1).is_err());
        assert!(create_allocated_swapfile(&dir.path().join("x"), 0).is_err());
    }

    #[test]
    fn allocate_writes_exact_size_without_following_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("swap");
        allocate_swapfile_bytes(&p, 2).unwrap();
        let meta = fs::metadata(&p).unwrap();
        assert_eq!(meta.len(), 2 * 1024 * 1024);
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        // Existing path (or symlink) is never opened for allocation.
        assert!(allocate_swapfile_bytes(&p, 1).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(allocate_swapfile_bytes(&link, 1).is_err());
        assert!(chmod_swapfile_nofollow(&link).is_err());
        assert!(chmod_swapfile_nofollow(&p).is_ok());
    }
}
