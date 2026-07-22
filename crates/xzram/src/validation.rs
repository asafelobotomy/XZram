use std::path::{Component, Path, PathBuf};

use crate::apply::SwapfileConfig;
use crate::error::{Result, XzramError};

const BLOCKED_PREFIXES: &[&str] = &[
    "/boot",
    "/boot/efi",
    "/efi",
    "/dev",
    "/proc",
    "/sys",
    "/run",
];

pub fn validate_swapfile_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        return Err(XzramError::Validation("swapfile path is empty".into()));
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

pub fn validate_swapfile_config(config: &SwapfileConfig) -> Result<SwapfileConfig> {
    validate_swapfile_path(&config.path)?;
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
    fn rejects_boot_path() {
        assert!(validate_swapfile_path("/boot/swapfile").is_err());
    }

    #[test]
    fn accepts_swap_path() {
        assert!(validate_swapfile_path("/swap/swapfile").is_ok());
    }
}
