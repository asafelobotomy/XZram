use std::fs;
use std::path::Path;

use crate::apply::SwapfileConfig;
use crate::backend::SwapfileBackendTrait;
use crate::error::Result;
use crate::snapshot::paths::etc_path;
use crate::swapfile_btrfs;
use crate::validation::fstab;

fn fstab_path() -> std::path::PathBuf {
    etc_path("fstab")
}

pub struct SwapfileBackend;

impl SwapfileBackend {
    fn parse_fstab_swapfiles(&self) -> Result<Vec<SwapfileConfig>> {
        let content = fs::read_to_string(fstab_path())?;
        Ok(fstab::parse_swap_entries(&content)
            .into_iter()
            .filter(|e| e.spec.starts_with('/') && !e.spec.starts_with("/dev/"))
            .map(|e| SwapfileConfig {
                size_mb: fs::metadata(&e.spec)
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0),
                priority: parse_fstab_priority(&e.options),
                path: e.spec,
            })
            .collect())
    }

    fn priority_for_path(&self, path: &str) -> Result<i32> {
        for entry in self.parse_fstab_swapfiles()? {
            if entry.path == path {
                return Ok(entry.priority);
            }
        }
        Ok(10)
    }
}

pub fn parse_fstab_priority(options: &str) -> i32 {
    for opt in options.split(',') {
        if let Some(pri) = opt.strip_prefix("pri=") {
            return pri.parse().unwrap_or(10);
        }
    }
    10
}

impl crate::backend::SwapBackend for SwapfileBackend {
    fn name(&self) -> &'static str {
        "swapfile"
    }

    fn is_available(&self) -> bool {
        Path::new("/usr/sbin/mkswap").exists() || which_exists("mkswap")
    }
}

impl SwapfileBackendTrait for SwapfileBackend {
    fn list(&self) -> Result<Vec<SwapfileConfig>> {
        self.parse_fstab_swapfiles()
    }

    fn create(&self, config: &SwapfileConfig) -> Result<()> {
        let config = crate::validation::validate_swapfile_config(config)?;

        // Validation includes the root-owned-ancestor check (ensure_secure_swap_location).
        let path = Path::new(&config.path);
        // Builds at a staging sibling and renames into place; refuses active swap.
        swapfile_btrfs::create_allocated_swapfile(path, config.size_mb)?;

        crate::apply::run_command(
            "swapon",
            &["-p", &config.priority.to_string(), "--", &config.path],
        )?;

        add_fstab_entry(&config.path, config.priority)?;
        Ok(())
    }

    fn remove(&self, path: &str) -> Result<()> {
        crate::validation::validate_swapfile_remove_path(path)?;

        crate::apply::deactivate_swap_path(path)?;
        remove_fstab_entry(path)?;
        match fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            other => other?,
        }
        Ok(())
    }

    fn resize(&self, path: &str, size_mb: u64) -> Result<()> {
        crate::validation::validate_swapfile_resize_path(path, size_mb)?;
        let path_obj = Path::new(path);
        let priority = self.priority_for_path(path)?;
        // Allocates + formats the new file first; old swap stays active on failure.
        swapfile_btrfs::replace_swapfile(path_obj, size_mb, priority)
    }
}

/// Best-effort SELinux relabel when `restorecon` is available (Fedora/RHEL).
pub(crate) fn restorecon_if_present(path: &Path) {
    if !which_exists("restorecon") {
        return;
    }
    let _ = std::process::Command::new("restorecon")
        .args(["-F"])
        .arg(path)
        .status();
}

fn add_fstab_entry(path: &str, priority: i32) -> Result<()> {
    let fstab = fstab_path();
    let content = match fs::read_to_string(&fstab) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let updated = fstab::upsert_swap_entry(&content, path, priority);
    if updated != content {
        crate::fsutil::atomic_write(&fstab, updated.as_bytes(), 0o644)?;
    }
    Ok(())
}

fn remove_fstab_entry(path: &str) -> Result<()> {
    let fstab = fstab_path();
    let content = match fs::read_to_string(&fstab) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let updated = fstab::remove_swap_entry(&content, path);
    if updated != content {
        crate::fsutil::atomic_write(&fstab, updated.as_bytes(), 0o644)?;
    }
    Ok(())
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fstab_priority_parses_pri() {
        assert_eq!(parse_fstab_priority("defaults,pri=50"), 50);
        assert_eq!(parse_fstab_priority("defaults"), 10);
    }

    #[test]
    fn fstab_entries_upsert_and_remove_atomically() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        let fstab = etc.path().join("fstab");
        fs::write(
            &fstab,
            "# /swapfile was here\n/var/swapfile none swap sw,pri=5 0 0\n",
        )
        .unwrap();

        add_fstab_entry("/swapfile", 10).unwrap();
        add_fstab_entry("/swapfile", 40).unwrap();
        let listed = SwapfileBackend.parse_fstab_swapfiles().unwrap();
        let mine: Vec<_> = listed.iter().filter(|c| c.path == "/swapfile").collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].priority, 40);
        assert!(listed.iter().any(|c| c.path == "/var/swapfile"));

        remove_fstab_entry("/swapfile").unwrap();
        assert_eq!(
            fs::read_to_string(&fstab).unwrap(),
            "# /swapfile was here\n/var/swapfile none swap sw,pri=5 0 0\n"
        );
        assert_eq!(fs::read_dir(etc.path()).unwrap().count(), 1);

        std::env::remove_var("XZRAM_ETC_ROOT");
    }
}
