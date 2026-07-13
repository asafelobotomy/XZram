use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::apply;
use crate::error::{Result, XzramError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodatacowStatus {
    pub swapfile_path: String,
    pub parent_directory: String,
    pub filesystem: String,
    pub on_btrfs: bool,
    pub parent_exists: bool,
    pub parent_nodatacow: bool,
    pub file_exists: bool,
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
    let parent_nodatacow = if parent_exists && on_btrfs {
        path_has_nodatacow(&parent).unwrap_or(false)
    } else {
        !on_btrfs
    };

    let file_nodatacow = if file_exists && on_btrfs {
        Some(path_has_nodatacow(path).unwrap_or(false))
    } else {
        None
    };

    let ready = if !on_btrfs {
        true
    } else if !parent_exists {
        false
    } else if !parent_nodatacow {
        false
    } else if file_exists {
        file_nodatacow.unwrap_or(false)
    } else {
        true
    };

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
    parent_nodatacow: bool,
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
    if !parent_nodatacow {
        return format!("Parent directory {parent_directory} is missing nodatacow (chattr +C)");
    }
    if file_exists && file_nodatacow == Some(false) {
        return "Swap file exists but is not nodatacow; run prepare or recreate the file".into();
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
        let msg = status_message(false, true, true, false, None, "/tmp");
        assert!(msg.contains("does not require"));
    }
}
