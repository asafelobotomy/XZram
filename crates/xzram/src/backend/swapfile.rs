use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::apply::SwapfileConfig;
use crate::backend::SwapfileBackendTrait;
use crate::error::Result;
use crate::swapfile_btrfs;

const FSTAB_PATH: &str = "/etc/fstab";

pub struct SwapfileBackend;

impl SwapfileBackend {
    fn parse_fstab_swapfiles(&self) -> Result<Vec<SwapfileConfig>> {
        let file = fs::File::open(FSTAB_PATH)?;
        let reader = BufReader::new(file);
        let mut swapfiles = Vec::new();

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 && parts[2] == "swap" {
                let path = parts[0].to_string();
                if path.starts_with('/') && !path.starts_with("/dev/") {
                    let priority =
                        parse_fstab_priority(parts.get(3).copied().unwrap_or("defaults"));
                    let size_mb = fs::metadata(&path)
                        .map(|m| m.len() / (1024 * 1024))
                        .unwrap_or(0);
                    swapfiles.push(SwapfileConfig {
                        path,
                        size_mb,
                        priority,
                    });
                }
            }
        }
        Ok(swapfiles)
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

        let path = Path::new(&config.path);

        if let Ok(status) = swapfile_btrfs::check_nodatacow(path) {
            if status.on_btrfs && !status.ready {
                swapfile_btrfs::prepare_nodatacow(path, true)?;
            }
        }
        swapfile_btrfs::ensure_ready_for_swapfile(path)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let size_bytes = config.size_mb * 1024 * 1024;
        create_swapfile(path, size_bytes)?;

        fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;

        crate::apply::run_command("mkswap", &[&config.path])?;
        crate::apply::run_command(
            "swapon",
            &["-p", &config.priority.to_string(), &config.path],
        )?;

        add_fstab_entry(&config.path, config.priority)?;
        Ok(())
    }

    fn remove(&self, path: &str) -> Result<()> {
        crate::validation::validate_swapfile_path(path)?;

        crate::apply::run_command("swapoff", &[path])?;
        remove_fstab_entry(path)?;
        fs::remove_file(path)?;
        Ok(())
    }

    fn resize(&self, path: &str, size_mb: u64) -> Result<()> {
        if size_mb == 0 {
            return Err(crate::error::XzramError::Validation(
                "swapfile size must be greater than 0 MiB".into(),
            ));
        }
        crate::validation::validate_swapfile_path(path)?;
        let priority = self.priority_for_path(path)?;
        crate::apply::run_command("swapoff", &[path])?;
        let size_bytes = size_mb * 1024 * 1024;
        create_swapfile(Path::new(path), size_bytes)?;
        fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        crate::apply::run_command("mkswap", &[path])?;
        crate::apply::run_command("swapon", &["-p", &priority.to_string(), path])?;
        Ok(())
    }
}

fn create_swapfile(path: &Path, size_bytes: u64) -> Result<()> {
    let output = std::process::Command::new("fallocate")
        .args(["-l", &size_bytes.to_string(), &path.to_string_lossy()])
        .output();

    match output {
        Ok(o) if o.status.success() => return Ok(()),
        _ => {}
    }

    let count_mb = size_bytes / (1024 * 1024);
    crate::apply::run_command(
        "dd",
        &[
            "if=/dev/zero",
            &format!("of={}", path.display()),
            "bs=1M",
            &format!("count={count_mb}"),
            "status=progress",
        ],
    )?;
    Ok(())
}

fn add_fstab_entry(path: &str, priority: i32) -> Result<()> {
    let mut content = fs::read_to_string(FSTAB_PATH)?;
    let entry = format!("\n{path} none swap sw,pri={priority} 0 0\n");
    if !content.contains(path) {
        content.push_str(&entry);
        fs::write(FSTAB_PATH, content)?;
    }
    Ok(())
}

fn remove_fstab_entry(path: &str) -> Result<()> {
    let content = fs::read_to_string(FSTAB_PATH)?;
    let filtered: String = content
        .lines()
        .filter(|line| !fstab_line_matches_swapfile(line, path))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(FSTAB_PATH, format!("{filtered}\n"))?;
    Ok(())
}

fn fstab_line_matches_swapfile(line: &str, path: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    let Some(first) = trimmed.split_whitespace().next() else {
        return false;
    };
    first == path
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
    fn fstab_line_match_is_exact() {
        assert!(fstab_line_matches_swapfile(
            "/swap/swapfile none swap sw,pri=10 0 0",
            "/swap/swapfile"
        ));
        assert!(!fstab_line_matches_swapfile(
            "/swap/swapfile2 none swap sw,pri=10 0 0",
            "/swap/swapfile"
        ));
        assert!(!fstab_line_matches_swapfile(
            "# /swap/swapfile none swap sw,pri=10 0 0",
            "/swap/swapfile"
        ));
    }
}
