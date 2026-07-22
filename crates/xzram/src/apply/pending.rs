use std::path::PathBuf;

use tracing::info;

use super::types::PendingConfig;
use crate::error::{Result, XzramError};
use crate::sysctl::SysctlValues;

pub fn data_dir() -> PathBuf {
    std::env::var("XZRAM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/xzram"))
}

/// Path to the last privileged-helper error (survives systemd-run swallowing stderr).
pub fn last_error_path() -> PathBuf {
    data_dir().join("last_error")
}

pub fn write_last_error(message: &str) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(last_error_path(), message);
}

pub fn clear_last_error() {
    let _ = std::fs::remove_file(last_error_path());
}

pub fn read_last_error() -> Option<String> {
    std::fs::read_to_string(last_error_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

pub(crate) fn write_pending(config: &PendingConfig) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::test_lock;
    use crate::apply::types::ZramConfig;

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
}
