use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::apply::SwapfileConfig;
use crate::error::{Result, XzramError};
use crate::snapshot::paths::etc_path;

const BLOCKED_PREFIXES: &[&str] = &[
    "/boot",
    "/boot/efi",
    "/efi",
    "/dev",
    "/proc",
    "/sys",
    "/run",
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/root",
];

const ALLOWED_PREFIXES: &[&str] = &["/swap/", "/var/swap/", "/var/lib/swap/"];
const ALLOWED_EXACT: &[&str] = &["/swapfile"];

pub fn validate_swapfile_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        return Err(XzramError::Validation("swapfile path is empty".into()));
    }
    if path
        .chars()
        .any(|c| c.is_ascii_control() || c.is_whitespace())
    {
        return Err(XzramError::Validation(
            "swapfile path must not contain whitespace or control characters".into(),
        ));
    }
    if !path.starts_with('/') {
        return Err(XzramError::Validation(
            "swapfile path must be absolute".into(),
        ));
    }
    if path.contains("..") {
        return Err(XzramError::Validation(
            "swapfile path must not contain '..'".into(),
        ));
    }

    let parsed = Path::new(path);
    for component in parsed.components() {
        if matches!(component, Component::ParentDir) {
            return Err(XzramError::Validation(
                "swapfile path must not contain parent directory segments".into(),
            ));
        }
    }

    for prefix in BLOCKED_PREFIXES {
        if path == *prefix || path.starts_with(&format!("{prefix}/")) {
            return Err(XzramError::Validation(format!(
                "swapfile path must not be under {prefix}"
            )));
        }
    }

    Ok(PathBuf::from(path))
}

pub fn path_under_allowlist(path: &str) -> bool {
    ALLOWED_EXACT.contains(&path)
        || ALLOWED_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
}

pub fn ensure_swapfile_under_allowlist(path: &str) -> Result<()> {
    if path_under_allowlist(path) {
        Ok(())
    } else {
        Err(XzramError::Validation(format!(
            "swapfile path must be under /swap/, /var/swap/, /var/lib/swap/, or exactly /swapfile (got {path})"
        )))
    }
}

/// Reject if any existing path component is a symlink; leaf must be a regular file if present.
pub fn ensure_no_symlink_components(path: &Path) -> Result<()> {
    let mut accumulated = PathBuf::new();
    for component in path.components() {
        accumulated.push(component);
        let meta = match fs::symlink_metadata(&accumulated) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if meta.file_type().is_symlink() {
            return Err(XzramError::Validation(format!(
                "swapfile path must not contain symlinks ({})",
                accumulated.display()
            )));
        }
    }
    if path.exists() {
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(XzramError::Validation(format!(
                "swapfile path must not be a symlink ({})",
                path.display()
            )));
        }
        if !meta.is_file() {
            return Err(XzramError::Validation(format!(
                "swapfile path must be a regular file ({})",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn is_known_swapfile(path: &str) -> bool {
    fstab_lists_swap_file(path) || proc_swaps_lists_file(path)
}

fn fstab_lists_swap_file(path: &str) -> bool {
    let Ok(content) = fs::read_to_string(etc_path("fstab")) else {
        return false;
    };
    content.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        parts.len() >= 3 && parts[0] == path && parts[2] == "swap"
    })
}

fn proc_swaps_lists_file(path: &str) -> bool {
    let Ok(content) = fs::read_to_string("/proc/swaps") else {
        return false;
    };
    content.lines().skip(1).any(|line| {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            return false;
        };
        let typ = parts.next().unwrap_or("");
        name == path && typ.eq_ignore_ascii_case("file")
    })
}

pub fn ensure_removable_swapfile(path: &str) -> Result<()> {
    if is_known_swapfile(path) {
        Ok(())
    } else {
        Err(XzramError::Validation(format!(
            "refusing to remove {path}: not listed as a swap file in fstab or active swaps"
        )))
    }
}

pub fn validate_swapfile_config(config: &SwapfileConfig) -> Result<SwapfileConfig> {
    validate_swapfile_path(&config.path)?;
    ensure_swapfile_under_allowlist(&config.path)?;
    ensure_no_symlink_components(Path::new(&config.path))?;
    if config.size_mb == 0 {
        return Err(XzramError::Validation(
            "swapfile size must be greater than 0 MiB".into(),
        ));
    }
    if config.priority < -1 || config.priority > 32767 {
        return Err(XzramError::Validation(
            "swap priority must be between -1 and 32767".into(),
        ));
    }
    Ok(config.clone())
}

pub fn validate_swapfile_prepare_path(path: &str) -> Result<PathBuf> {
    let pb = validate_swapfile_path(path)?;
    ensure_swapfile_under_allowlist(path)?;
    ensure_no_symlink_components(Path::new(path))?;
    Ok(pb)
}

pub fn validate_swapfile_remove_path(path: &str) -> Result<PathBuf> {
    let pb = validate_swapfile_path(path)?;
    ensure_no_symlink_components(Path::new(path))?;
    ensure_removable_swapfile(path)?;
    Ok(pb)
}

pub fn validate_swapfile_resize_path(path: &str, size_mb: u64) -> Result<()> {
    validate_swapfile_path(path)?;
    if !path_under_allowlist(path) && !is_known_swapfile(path) {
        return Err(XzramError::Validation(format!(
            "swapfile resize path must be under the allowlist or an existing swap file (got {path})"
        )));
    }
    ensure_no_symlink_components(Path::new(path))?;
    if size_mb == 0 {
        return Err(XzramError::Validation(
            "swapfile size must be greater than 0 MiB".into(),
        ));
    }
    Ok(())
}

pub fn validate_swap_device(device: &str) -> Result<()> {
    if device.is_empty() {
        return Err(XzramError::Validation("swap device is empty".into()));
    }
    if device.starts_with('-') {
        return Err(XzramError::Validation(
            "swap device must not start with '-' (refusing flag injection)".into(),
        ));
    }
    if device
        .chars()
        .any(|c| c.is_ascii_control() || c.is_whitespace())
    {
        return Err(XzramError::Validation(
            "swap device must not contain whitespace or control characters".into(),
        ));
    }
    if device.contains("..") {
        return Err(XzramError::Validation(
            "swap device must not contain '..' path components".into(),
        ));
    }
    let ok = device.starts_with("/dev/")
        || device.starts_with("UUID=")
        || device.starts_with("PARTUUID=")
        || device.starts_with("LABEL=");
    if !ok {
        return Err(XzramError::Validation(
            "swap device must be /dev/... or UUID=/PARTUUID=/LABEL=".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_path() {
        assert!(validate_swapfile_path("swap/swapfile").is_err());
    }

    #[test]
    fn rejects_parent_dir() {
        assert!(validate_swapfile_path("/swap/../etc/passwd").is_err());
    }

    #[test]
    fn rejects_boot_and_etc() {
        assert!(validate_swapfile_path("/boot/swapfile").is_err());
        assert!(validate_swapfile_path("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_newline_path() {
        assert!(validate_swapfile_path("/swap/swap\nfile").is_err());
        assert!(validate_swapfile_path("/swap/swap file").is_err());
    }

    #[test]
    fn accepts_allowlisted_paths() {
        assert!(validate_swapfile_path("/swap/swapfile").is_ok());
        assert!(ensure_swapfile_under_allowlist("/swap/swapfile").is_ok());
        assert!(ensure_swapfile_under_allowlist("/swapfile").is_ok());
        assert!(ensure_swapfile_under_allowlist("/home/user/swap").is_err());
    }

    #[test]
    fn rejects_flag_device() {
        assert!(validate_swap_device("-a").is_err());
        assert!(validate_swap_device("/dev/../sda1").is_err());
        assert!(validate_swap_device("/dev/sda1").is_ok());
        assert!(validate_swap_device("UUID=abc").is_ok());
    }

    #[test]
    fn rejects_symlink_components() {
        let _guard = crate::test_env_lock();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        fs::write(&target, b"x").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(ensure_no_symlink_components(&link).is_err());
    }

    #[test]
    fn removable_requires_known_swap() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        fs::write(
            etc.path().join("fstab"),
            "/swap/swapfile none swap sw,pri=10 0 0\n",
        )
        .unwrap();

        assert!(ensure_removable_swapfile("/swap/swapfile").is_ok());
        assert!(ensure_removable_swapfile("/swap/other").is_err());
        // Exact /swapfile stays allowlisted for create even when not removable yet.
        assert!(ensure_swapfile_under_allowlist("/swapfile").is_ok());

        std::env::remove_var("XZRAM_ETC_ROOT");
    }
}
