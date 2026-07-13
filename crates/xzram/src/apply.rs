use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::backend::{
    available_swapfile_backend, available_zram_backend, SwapfileBackendTrait, ZramBackendTrait,
};
use crate::error::{Result, XzramError};
use crate::migrate;
use crate::snapshot::{self, SnapshotTrigger};
use crate::sysctl::{self, SysctlValues};

pub const PENDING_PATH: &str = "/var/lib/xzram/pending.json";
pub const SYSCTL_PATH: &str = "/etc/sysctl.d/99-xzram.conf";

pub fn data_dir() -> PathBuf {
    std::env::var("XZRAM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/xzram"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramConfig {
    pub device: String,
    pub zram_size: Option<String>,
    pub zram_resident_limit: Option<String>,
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
pub struct SwapfileResizeConfig {
    pub path: String,
    pub size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub zram: Option<ZramConfig>,
    pub swapfile: Option<SwapfileConfig>,
    pub disable_zram: bool,
    pub remove_swapfile: Option<String>,
}

/// Staged configuration merged into pending.json before `apply`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingConfig {
    pub zram: Option<ZramConfig>,
    pub disable_zram: bool,
    pub swapfile: Option<SwapfileConfig>,
    pub swapfile_resize: Option<SwapfileResizeConfig>,
    pub remove_swapfile: Option<String>,
    pub sysctl: Option<SysctlValues>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub success: bool,
    pub messages: Vec<String>,
}

fn pending_path() -> PathBuf {
    data_dir().join("pending.json")
}

pub fn load_pending() -> Result<Option<PendingConfig>> {
    let path = pending_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let config: PendingConfig = serde_json::from_str(&content)
        .map_err(|e| XzramError::Parse(format!("pending config: {e}")))?;
    Ok(Some(config))
}

pub fn stage(partial: &PendingConfig) -> Result<()> {
    let mut current = load_pending()?.unwrap_or_default();
    merge_pending(&mut current, partial);
    write_pending(&current)?;
    info!("staged configuration change");
    Ok(())
}

fn merge_pending(current: &mut PendingConfig, partial: &PendingConfig) {
    if partial.disable_zram {
        current.disable_zram = true;
        current.zram = None;
    }
    if let Some(ref zram) = partial.zram {
        current.disable_zram = false;
        current.zram = Some(zram.clone());
    }
    if let Some(ref swapfile) = partial.swapfile {
        current.swapfile = Some(swapfile.clone());
        current.swapfile_resize = None;
        current.remove_swapfile = None;
    }
    if let Some(ref resize) = partial.swapfile_resize {
        current.swapfile_resize = Some(resize.clone());
        current.swapfile = None;
        current.remove_swapfile = None;
    }
    if let Some(ref path) = partial.remove_swapfile {
        current.remove_swapfile = Some(path.clone());
        current.swapfile = None;
        current.swapfile_resize = None;
    }
    if let Some(ref sysctl) = partial.sysctl {
        current.sysctl = Some(merge_sysctl(current.sysctl.as_ref(), sysctl));
    }
}

fn merge_sysctl(existing: Option<&SysctlValues>, incoming: &SysctlValues) -> SysctlValues {
    let mut merged = existing.cloned().unwrap_or(SysctlValues {
        swappiness: None,
        watermark_boost_factor: None,
        watermark_scale_factor: None,
        page_cluster: None,
    });
    if incoming.swappiness.is_some() {
        merged.swappiness = incoming.swappiness;
    }
    if incoming.watermark_boost_factor.is_some() {
        merged.watermark_boost_factor = incoming.watermark_boost_factor;
    }
    if incoming.watermark_scale_factor.is_some() {
        merged.watermark_scale_factor = incoming.watermark_scale_factor;
    }
    if incoming.page_cluster.is_some() {
        merged.page_cluster = incoming.page_cluster;
    }
    merged
}

fn write_pending(config: &PendingConfig) -> Result<()> {
    let path = pending_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content =
        serde_json::to_string_pretty(config).map_err(|e| XzramError::Parse(e.to_string()))?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn clear_pending() -> Result<()> {
    let path = pending_path();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn pending_is_empty(config: &PendingConfig) -> bool {
    !config.disable_zram
        && config.zram.is_none()
        && config.swapfile.is_none()
        && config.swapfile_resize.is_none()
        && config.remove_swapfile.is_none()
        && config.sysctl.is_none()
}

pub fn apply_pending() -> Result<ApplyResult> {
    let pending = load_pending()?
        .ok_or_else(|| XzramError::NotFound("No pending configuration to apply".into()))?;
    if pending_is_empty(&pending) {
        return Err(XzramError::Validation(
            "Pending configuration is empty".into(),
        ));
    }

    snapshot::create_snapshot(
        SnapshotTrigger::PreApply,
        Some(&snapshot::label_from_pending(&pending)),
        Some(&pending),
    )?;
    let result = apply_from_pending(&pending)?;
    clear_pending()?;
    info!("applied pending configuration");
    Ok(result)
}

fn apply_from_pending(pending: &PendingConfig) -> Result<ApplyResult> {
    let request = ApplyRequest {
        zram: pending.zram.clone(),
        swapfile: pending.swapfile.clone(),
        disable_zram: pending.disable_zram,
        remove_swapfile: pending.remove_swapfile.clone(),
    };
    let mut result = apply(&request)?;

    if let Some(ref resize) = pending.swapfile_resize {
        let backend = available_swapfile_backend();
        ensure_backend_available(backend.as_ref())?;
        SwapfileBackendTrait::resize(backend.as_ref(), &resize.path, resize.size_mb)?;
        result
            .messages
            .push(format!("Resized swapfile {}", resize.path));
    }

    if let Some(ref sysctl) = pending.sysctl {
        sysctl::set(sysctl)?;
        result.messages.push("Applied sysctl values".into());
    }

    if migrate::zramswap_config_exists() {
        let migrate_msgs = migrate::finalize_zram_tools_migration()?;
        result.messages.extend(migrate_msgs);
    }

    Ok(result)
}

pub fn apply(request: &ApplyRequest) -> Result<ApplyResult> {
    let mut messages = Vec::new();

    if request.disable_zram {
        let backend = available_zram_backend()?;
        ensure_backend_available(backend.as_ref())?;
        ZramBackendTrait::disable(backend.as_ref())?;
        messages.push("Disabled zram configuration".into());
    } else if let Some(ref zram) = request.zram {
        let backend = available_zram_backend()?;
        ensure_backend_available(backend.as_ref())?;
        ZramBackendTrait::configure(backend.as_ref(), zram)?;
        ZramBackendTrait::apply(backend.as_ref())?;
        messages.push(format!("Applied zram config for {}", zram.device));
    }

    if let Some(ref path) = request.remove_swapfile {
        let backend = available_swapfile_backend();
        ensure_backend_available(backend.as_ref())?;
        SwapfileBackendTrait::remove(backend.as_ref(), path)?;
        messages.push(format!("Removed swapfile {path}"));
    } else if let Some(ref swapfile) = request.swapfile {
        let backend = available_swapfile_backend();
        ensure_backend_available(backend.as_ref())?;
        SwapfileBackendTrait::create(backend.as_ref(), swapfile)?;
        messages.push(format!("Created swapfile {}", swapfile.path));
    }

    Ok(ApplyResult {
        success: true,
        messages,
    })
}

fn ensure_backend_available(backend: &dyn crate::backend::SwapBackend) -> Result<()> {
    if !backend.is_available() {
        return Err(XzramError::Backend(format!(
            "backend '{}' is not available on this system",
            backend.name()
        )));
    }
    Ok(())
}

pub fn rollback() -> Result<ApplyResult> {
    snapshot::rollback()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn pending_merge_zram() {
        let mut current = PendingConfig::default();
        let partial = PendingConfig {
            zram: Some(ZramConfig {
                device: "zram0".into(),
                zram_size: Some("512".into()),
                zram_resident_limit: None,
                compression_algorithm: Some("zstd".into()),
                swap_priority: Some(100),
                fs_type: None,
                mount_point: None,
            }),
            ..Default::default()
        };
        merge_pending(&mut current, &partial);
        assert!(current.zram.is_some());
        assert!(!current.disable_zram);
    }

    #[test]
    fn pending_disable_clears_zram() {
        let mut current = PendingConfig {
            zram: Some(ZramConfig {
                device: "zram0".into(),
                zram_size: None,
                zram_resident_limit: None,
                compression_algorithm: None,
                swap_priority: None,
                fs_type: None,
                mount_point: None,
            }),
            ..Default::default()
        };
        merge_pending(
            &mut current,
            &PendingConfig {
                disable_zram: true,
                ..Default::default()
            },
        );
        assert!(current.disable_zram);
        assert!(current.zram.is_none());
    }

    #[test]
    fn pending_is_empty_default() {
        assert!(pending_is_empty(&PendingConfig::default()));
    }

    #[test]
    fn merge_sysctl_values() {
        let merged = merge_sysctl(
            Some(&SysctlValues {
                swappiness: Some(60),
                watermark_boost_factor: None,
                watermark_scale_factor: None,
                page_cluster: None,
            }),
            &SysctlValues {
                swappiness: None,
                watermark_boost_factor: Some(0),
                watermark_scale_factor: Some(125),
                page_cluster: None,
            },
        );
        assert_eq!(merged.swappiness, Some(60));
        assert_eq!(merged.watermark_boost_factor, Some(0));
        assert_eq!(merged.watermark_scale_factor, Some(125));
    }

    #[test]
    fn stage_and_load_pending() {
        let _guard = test_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", dir.path());

        let partial = PendingConfig {
            zram: Some(ZramConfig {
                device: "zram0".into(),
                zram_size: Some("1024".into()),
                zram_resident_limit: None,
                compression_algorithm: Some("zstd".into()),
                swap_priority: Some(100),
                fs_type: None,
                mount_point: None,
            }),
            ..Default::default()
        };
        stage(&partial).unwrap();
        let loaded = load_pending().unwrap().expect("pending should exist");
        assert_eq!(loaded.zram.as_ref().unwrap().device, "zram0");

        clear_pending().unwrap();
        assert!(load_pending().unwrap().is_none());

        std::env::remove_var("XZRAM_DATA_DIR");
    }

    #[test]
    fn apply_pending_empty_errors() {
        let _guard = test_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", dir.path());
        write_pending(&PendingConfig::default()).unwrap();
        let err = apply_pending().unwrap_err().to_string();
        assert!(err.contains("empty"));
        std::env::remove_var("XZRAM_DATA_DIR");
    }
}
