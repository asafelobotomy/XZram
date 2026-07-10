use serde::{Deserialize, Serialize};

use crate::backend::{SwapfileBackendTrait, ZramBackendTrait};
use crate::error::{Result, XzramError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramConfig {
    pub device: String,
    pub zram_size: Option<String>,
    pub compression_algorithm: Option<String>,
    pub swap_priority: Option<i32>,
    pub fs_type: Option<String>,
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapfileConfig {
    pub path: String,
    pub size_mb: u64,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub zram: Option<ZramConfig>,
    pub swapfile: Option<SwapfileConfig>,
    pub disable_zram: bool,
    pub remove_swapfile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub success: bool,
    pub messages: Vec<String>,
}

pub fn apply(request: &ApplyRequest) -> Result<ApplyResult> {
    let mut messages = Vec::new();

    if request.disable_zram {
        let backend = crate::backend::zram_generator::ZramGeneratorBackend;
        backend.disable()?;
        messages.push("Disabled zram configuration".into());
    } else if let Some(ref zram) = request.zram {
        let backend = crate::backend::zram_generator::ZramGeneratorBackend;
        backend.configure(zram)?;
        backend.apply()?;
        messages.push(format!("Applied zram config for {}", zram.device));
    }

    if let Some(ref path) = request.remove_swapfile {
        let backend = crate::backend::swapfile::SwapfileBackend;
        backend.remove(path)?;
        messages.push(format!("Removed swapfile {path}"));
    } else if let Some(ref swapfile) = request.swapfile {
        let backend = crate::backend::swapfile::SwapfileBackend;
        backend.create(swapfile)?;
        messages.push(format!("Created swapfile {}", swapfile.path));
    }

    Ok(ApplyResult {
        success: true,
        messages,
    })
}

pub fn rollback() -> Result<ApplyResult> {
    let backup_dir = backup_dir();
    if !backup_dir.exists() {
        return Err(XzramError::NotFound("No backup found for rollback".into()));
    }

    let mut messages = Vec::new();

    let zram_backup = backup_dir.join("zram-generator.conf");
    let zram_target = std::path::Path::new("/etc/systemd/zram-generator.conf");
    if zram_backup.exists() {
        std::fs::copy(&zram_backup, zram_target)?;
        messages.push("Restored zram-generator.conf".into());
    }

    let fstab_backup = backup_dir.join("fstab");
    let fstab_target = std::path::Path::new("/etc/fstab");
    if fstab_backup.exists() {
        std::fs::copy(&fstab_backup, fstab_target)?;
        messages.push("Restored /etc/fstab".into());
    }

    run_systemctl(&["daemon-reload"])?;
    messages.push("Reloaded systemd".into());

    Ok(ApplyResult {
        success: true,
        messages,
    })
}

pub fn backup_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("/var/lib/xzram/backup")
}

pub fn create_backup() -> Result<()> {
    let dir = backup_dir();
    std::fs::create_dir_all(&dir)?;

    let zram_src = std::path::Path::new("/etc/systemd/zram-generator.conf");
    if zram_src.exists() {
        std::fs::copy(zram_src, dir.join("zram-generator.conf"))?;
    }

    let fstab_src = std::path::Path::new("/etc/fstab");
    if fstab_src.exists() {
        std::fs::copy(fstab_src, dir.join("fstab"))?;
    }

    Ok(())
}

pub fn run_systemctl(args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| XzramError::Command(format!("systemctl: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XzramError::Command(format!(
            "systemctl {}: {stderr}",
            args.join(" ")
        )));
    }
    Ok(())
}

pub fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| XzramError::Command(format!("{cmd}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XzramError::Command(format!(
            "{cmd} {}: {stderr}",
            args.join(" ")
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
