//! StrictModes-style location checks before mutating a swapfile as root.
//!
//! A one-shot "no symlinks" check is racy if an unprivileged user can write to any ancestor
//! directory: they can swap a component for a symlink (e.g. to `/etc/shadow`) between the
//! check and `fallocate`/`mkswap`/`unlink`. Requiring every existing ancestor to be a real
//! directory owned by root and not group/other-writable closes that window, since only root
//! can then rename or replace entries along the path.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use crate::error::{Result, XzramError};

/// Ownership/mode checks only make sense (and only pass) when running as real root on the
/// live system. Unprivileged runs cannot mutate root paths anyway, and the hermetic
/// `XZRAM_ETC_ROOT` mode (tests, dry runs) operates on user-owned temp directories.
pub fn ownership_checks_enforced() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    euid == 0 && std::env::var_os("XZRAM_ETC_ROOT").is_none()
}

/// Whether a directory with this owner and mode is safe to hold a root-managed swapfile.
pub fn directory_mode_is_secure(uid: u32, mode: u32) -> bool {
    uid == 0 && mode & 0o022 == 0
}

/// Verify every existing ancestor of `path` is a non-symlink directory; when `enforce_owner`
/// is set, also require root ownership and no group/other write permission.
pub fn check_secure_ancestors(path: &Path, enforce_owner: bool) -> Result<()> {
    let mut ancestors: Vec<&Path> = path
        .ancestors()
        .skip(1)
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    ancestors.reverse();
    for dir in ancestors {
        let meta = match fs::symlink_metadata(dir) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if meta.file_type().is_symlink() {
            return Err(XzramError::Validation(format!(
                "swapfile path must not contain symlinks ({})",
                dir.display()
            )));
        }
        if !meta.is_dir() {
            return Err(XzramError::Validation(format!(
                "swapfile parent {} is not a directory",
                dir.display()
            )));
        }
        let mode = meta.permissions().mode();
        if enforce_owner && !directory_mode_is_secure(meta.uid(), mode) {
            return Err(XzramError::Validation(format!(
                "refusing to modify swapfile: directory {} must be owned by root and not \
                 group/other-writable (owner uid {}, mode {:o}); fix with \
                 `chown root: {0}` and `chmod go-w {0}`",
                dir.display(),
                meta.uid(),
                mode & 0o7777
            )));
        }
    }
    Ok(())
}

/// Full pre-mutation check for create/resize/remove: no symlinks anywhere on the path,
/// the leaf (if present) is a regular file, and (as root on the live system) every ancestor
/// is root-owned and not group/other-writable.
pub fn ensure_secure_swap_location(path: &Path) -> Result<()> {
    secure_location(path, ownership_checks_enforced())
}

fn secure_location(path: &Path, enforce_owner: bool) -> Result<()> {
    check_secure_ancestors(path, enforce_owner)?;
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(XzramError::Validation(format!(
            "swapfile path must not be a symlink ({})",
            path.display()
        ))),
        Ok(meta) if !meta.is_file() => Err(XzramError::Validation(format!(
            "swapfile path must be a regular file ({})",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_rules() {
        assert!(directory_mode_is_secure(0, 0o40755));
        assert!(!directory_mode_is_secure(1000, 0o40755));
        assert!(!directory_mode_is_secure(0, 0o40775));
        assert!(!directory_mode_is_secure(0, 0o41777));
    }

    #[test]
    fn user_owned_ancestor_rejected_when_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let swap = dir.path().join("swapfile");
        assert!(check_secure_ancestors(&swap, false).is_ok());
        // tempdirs are never root-owned + 0755 for an unprivileged test run.
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } != 0 {
            let err = check_secure_ancestors(&swap, true).unwrap_err();
            assert!(err.to_string().contains("owned by root"), "{err}");
        }
    }

    #[test]
    fn symlinked_ancestor_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(check_secure_ancestors(&link.join("swapfile"), false).is_err());
        assert!(secure_location(&link, false).is_err());
    }

    #[test]
    fn leaf_must_be_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        assert!(secure_location(&sub, false).is_err());
        let f = dir.path().join("swapfile");
        assert!(secure_location(&f, false).is_ok());
        fs::write(&f, b"x").unwrap();
        assert!(secure_location(&f, false).is_ok());
    }

    #[test]
    fn root_level_default_paths_pass() {
        // `/` is root-owned and 0755 on any sane system; /swapfile's only ancestor.
        assert!(check_secure_ancestors(Path::new("/swapfile"), true).is_ok());
    }
}
