use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::apply;
use crate::error::{Result, XzramError};

mod alloc;
mod inplace;
pub use alloc::{
    chmod_swapfile_nofollow, create_allocated_swapfile, path_is_active_swap, replace_swapfile,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodatacowStatus {
    pub swapfile_path: String,
    pub parent_directory: String,
    pub filesystem: String,
    pub on_btrfs: bool,
    pub parent_exists: bool,
    /// `None` means the attribute could not be probed (e.g. permission denied).
    pub parent_nodatacow: Option<bool>,
    pub file_exists: bool,
    /// `None` when the file is absent or the attribute could not be probed.
    pub file_nodatacow: Option<bool>,
    pub ready: bool,
    pub message: String,
}

pub fn check_nodatacow(path: &Path) -> Result<NodatacowStatus> {
    build_status(path, false)
}

pub fn prepare_nodatacow(path: &Path, mkdir_parents: bool) -> Result<NodatacowStatus> {
    let parent = parent_directory(path)?;

    if mkdir_parents && !parent.exists() {
        std::fs::create_dir_all(&parent)?;
    }

    let fstype = filesystem_for_path(&parent)?;
    if fstype != "btrfs" {
        return build_status(path, false);
    }

    if parent.exists() && !path_has_nodatacow(&parent)? {
        apply::run_command("chattr", &["+C", &parent.to_string_lossy()])?;
    }

    if path.exists() && !path_has_nodatacow(path)? {
        apply::run_command("chattr", &["+C", &path.to_string_lossy()])?;
    }

    build_status(path, false)
}

fn build_status(path: &Path, _preparing: bool) -> Result<NodatacowStatus> {
    let swapfile_path = path.to_string_lossy().into_owned();
    let parent = parent_directory(path)?;
    let parent_directory = parent.to_string_lossy().into_owned();
    let parent_exists = parent.exists();
    let file_exists = path.exists();

    let fstype = if parent_exists {
        filesystem_for_path(&parent)?
    } else {
        filesystem_for_path(&parent).unwrap_or_else(|_| String::from("unknown"))
    };

    let on_btrfs = fstype == "btrfs";
    let parent_nodatacow = if !on_btrfs {
        Some(true)
    } else if parent_exists {
        path_has_nodatacow(&parent).ok()
    } else {
        None
    };

    let file_nodatacow = if file_exists && on_btrfs {
        path_has_nodatacow(path).ok()
    } else {
        None
    };

    // ready is false only when we know a required nodatacow check failed.
    let ready = !on_btrfs
        || (parent_exists
            && parent_nodatacow != Some(false)
            && !(file_exists && file_nodatacow == Some(false)));

    let message = status_message(
        on_btrfs,
        parent_exists,
        parent_nodatacow,
        file_exists,
        file_nodatacow,
        &parent_directory,
    );

    Ok(NodatacowStatus {
        swapfile_path,
        parent_directory,
        filesystem: fstype,
        on_btrfs,
        parent_exists,
        parent_nodatacow,
        file_exists,
        file_nodatacow,
        ready,
        message,
    })
}

fn status_message(
    on_btrfs: bool,
    parent_exists: bool,
    parent_nodatacow: Option<bool>,
    file_exists: bool,
    file_nodatacow: Option<bool>,
    parent_directory: &str,
) -> String {
    if !on_btrfs {
        return "Filesystem does not require btrfs nodatacow".into();
    }
    if !parent_exists {
        return format!(
            "Parent directory {parent_directory} does not exist; create it and run prepare"
        );
    }
    if parent_nodatacow == Some(false) {
        return format!("Parent directory {parent_directory} is missing nodatacow (chattr +C)");
    }
    if parent_nodatacow.is_none() {
        return format!(
            "Could not verify nodatacow on {parent_directory} (insufficient privileges)"
        );
    }
    if file_exists && file_nodatacow == Some(false) {
        return "Swap file exists but is not nodatacow; it will be recreated on apply (or run prepare / remove first)".into();
    }
    if file_exists && file_nodatacow.is_none() {
        return "Could not verify swapfile nodatacow (insufficient privileges)".into();
    }
    if file_exists {
        return "Swap file and parent directory are nodatacow-ready".into();
    }
    "Parent directory is nodatacow-ready for a new swap file".into()
}

fn parent_directory(path: &Path) -> Result<PathBuf> {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or_else(|| XzramError::Validation("invalid swapfile path".into()))
}

fn filesystem_for_path(path: &Path) -> Result<String> {
    Ok(
        apply::run_command("findmnt", &["-no", "FSTYPE", "-T", &path.to_string_lossy()])?
            .trim()
            .to_string(),
    )
}

fn path_has_nodatacow(path: &Path) -> Result<bool> {
    let output = apply::run_command("lsattr", &["-d", &path.to_string_lossy()])?;
    Ok(lsattr_has_nodatacow(&output))
}

pub fn lsattr_has_nodatacow(output: &str) -> bool {
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .is_some_and(|attrs| attrs.contains('C'))
}

pub fn ensure_ready_for_swapfile(path: &Path) -> Result<()> {
    let status = check_nodatacow(path)?;
    if status.ready {
        return Ok(());
    }
    Err(XzramError::Validation(format!(
        "{}. Run: xzram swapfile prepare {}",
        status.message, status.swapfile_path
    )))
}

pub fn is_btrfs_path(path: &Path) -> bool {
    filesystem_for_path(path)
        .map(|fstype| fstype == "btrfs")
        .unwrap_or(false)
}

fn btrfs_mkswapfile_available() -> bool {
    std::process::Command::new("btrfs")
        .arg("filesystem")
        .arg("mkswapfile")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsattr_detects_nodatacow_flag() {
        assert!(!lsattr_has_nodatacow("--------------e-- /tmp/swap\n"));
        assert!(lsattr_has_nodatacow("----C--------e-- /tmp/swap\n"));
    }

    #[test]
    fn status_message_non_btrfs() {
        let msg = status_message(false, true, Some(true), false, None, "/tmp");
        assert!(msg.contains("does not require"));
    }

    #[test]
    fn status_message_unknown_file_attrs() {
        let msg = status_message(true, true, Some(true), true, None, "/swap");
        assert!(msg.contains("Could not verify swapfile nodatacow"));
    }

    #[test]
    fn status_message_known_missing_file_attrs() {
        let msg = status_message(true, true, Some(true), true, Some(false), "/swap");
        assert!(msg.contains("not nodatacow"));
    }
}
