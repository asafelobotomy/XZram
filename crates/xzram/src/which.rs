//! Look a program up on `$PATH` without running `which`, which minimal installs
//! (containers, Debian/Fedora base images) don't ship.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Full path of the first executable file named `cmd` on `$PATH`, if any.
pub fn find_in_path(cmd: &str) -> Option<PathBuf> {
    find_in(cmd, &std::env::var_os("PATH")?)
}

fn find_in(cmd: &str, path: &OsStr) -> Option<PathBuf> {
    if cmd.is_empty() || cmd.contains('/') {
        return None;
    }
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(cmd))
        .find(|p| is_executable_file(p))
}

/// Whether `cmd` is an executable on `$PATH`.
pub fn command_exists(cmd: &str) -> bool {
    find_in_path(cmd).is_some()
}

fn is_executable_file(p: &Path) -> bool {
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_executables_only() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("tool");
        let plain = dir.path().join("data");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::write(&plain, "x").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let path = std::ffi::OsString::from(format!("relative:{}", dir.path().display()));
        assert_eq!(find_in("tool", &path), Some(exe));
        for cmd in ["data", "subdir", "missing", "", "../tool"] {
            assert_eq!(find_in(cmd, &path), None, "{cmd}");
        }
    }
}
