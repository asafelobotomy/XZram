//! Where the helper records its failure message.
//!
//! The CLI and GUI invoke the helper via pkexec (which clears the environment) and read
//! the global `/var/lib/xzram/last_error`. xzramd instead runs the helper through
//! `systemd-run --setenv=XZRAM_ERROR_FILE=<unique path>` so concurrent runs can't
//! overwrite each other's error and the daemon never reports someone else's failure.

use std::path::{Component, Path, PathBuf};

use tracing::warn;
use xzram::apply;

pub const ERROR_FILE_ENV: &str = "XZRAM_ERROR_FILE";
/// Directories a per-run error file may live in (both root-owned, not world-writable).
const ALLOWED_DIRS: [&str; 2] = ["/run/xzram", "/var/lib/xzram"];
const MAX_NAME_LEN: usize = 128;

pub enum ErrorSink {
    /// Per-run error file supplied by xzramd.
    PerRun(PathBuf),
    /// Global `last_error` (pkexec / CLI / GUI path).
    Global,
}

impl ErrorSink {
    pub fn from_env() -> Self {
        let Some(raw) = std::env::var_os(ERROR_FILE_ENV) else {
            return Self::Global;
        };
        let path = PathBuf::from(raw);
        match validate_error_file(&path).and_then(|()| check_parent_dir(&path)) {
            Ok(()) => Self::PerRun(path),
            Err(e) => {
                warn!(path = %path.display(), "ignoring {ERROR_FILE_ENV}: {e}");
                Self::Global
            }
        }
    }

    /// Remove any stale error from a previous run before doing work.
    pub fn clear(&self) {
        match self {
            Self::PerRun(path) => {
                let _ = std::fs::remove_file(path);
            }
            Self::Global => apply::clear_last_error(),
        }
    }

    pub fn write(&self, message: &str) {
        match self {
            Self::PerRun(path) => {
                if let Err(e) = xzram::fsutil::atomic_write(path, message.as_bytes(), 0o600) {
                    warn!(path = %path.display(), "could not write per-run error file: {e}");
                    apply::write_last_error(message);
                }
            }
            Self::Global => apply::write_last_error(message),
        }
    }
}

/// Pure shape check: absolute, no `.`/`..`, directly inside an allowed directory, and a
/// plain file name (`[A-Za-z0-9._-]`, not hidden).
pub fn validate_error_file(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("path must be absolute".into());
    }
    // `Path::components` silently normalizes `.` and `//`, so check the raw text too.
    let raw = path.to_str().ok_or("path must be UTF-8")?;
    if raw
        .split('/')
        .skip(1)
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("path must be normalized (no '.', '..' or empty segments)".into());
    }
    let parent = path.parent().ok_or("path has no parent")?;
    if !ALLOWED_DIRS.iter().any(|d| parent == Path::new(d)) {
        return Err(format!("parent must be one of {ALLOWED_DIRS:?}"));
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid file name")?;
    let name_ok = !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !name_ok {
        return Err("file name must be [A-Za-z0-9._-] and not hidden".into());
    }
    Ok(())
}

/// The parent must already exist as a real directory (not a symlink).
fn check_parent_dir(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("path has no parent")?;
    let meta = std::fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err("parent is not a directory".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_files_in_allowed_dirs() {
        assert!(validate_error_file(Path::new("/run/xzram/helper-error-1-2-3")).is_ok());
        assert!(validate_error_file(Path::new("/var/lib/xzram/helper-error-1")).is_ok());
    }

    #[test]
    fn rejects_other_locations_and_names() {
        for bad in [
            "relative/x",
            "/tmp/x",
            "/etc/passwd",
            "/run/xzram/../../etc/passwd",
            "/run/xzram/./x",
            "/run/xzram/sub/x",
            "/run/xzram/.hidden",
            "/run/xzram/bad name",
            "/run/xzram",
        ] {
            assert!(validate_error_file(Path::new(bad)).is_err(), "{bad}");
        }
    }
}
