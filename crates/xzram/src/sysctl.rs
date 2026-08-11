use crate::apply::{self};
use crate::error::{Result, XzramError};
use crate::snapshot::paths::{etc_path, SYSCTL_FILE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysctlValues {
    pub swappiness: Option<u32>,
    pub watermark_boost_factor: Option<u32>,
    pub watermark_scale_factor: Option<u32>,
    pub page_cluster: Option<u32>,
}

pub fn show() -> Result<SysctlValues> {
    Ok(SysctlValues {
        swappiness: read_sysctl("vm.swappiness"),
        watermark_boost_factor: read_sysctl("vm.watermark_boost_factor"),
        watermark_scale_factor: read_sysctl("vm.watermark_scale_factor"),
        page_cluster: read_sysctl("vm.page-cluster"),
    })
}

/// Map a sysctl dotted name to its `/proc/sys` path (`vm.page-cluster` → `vm/page-cluster`).
pub fn proc_sys_path(key: &str) -> String {
    format!("/proc/sys/{}", key.replace('.', "/"))
}

fn read_sysctl(key: &str) -> Option<u32> {
    std::fs::read_to_string(proc_sys_path(key))
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Parse known keys from an existing `/etc/sysctl.d/99-xzram.conf` drop-in.
fn read_drop_in(path: &std::path::Path) -> SysctlValues {
    let mut values = SysctlValues {
        swappiness: None,
        watermark_boost_factor: None,
        watermark_scale_factor: None,
        page_cluster: None,
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return values;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let Ok(v) = raw.trim().parse::<u32>() else {
            continue;
        };
        match key {
            "vm.swappiness" => values.swappiness = Some(v),
            "vm.watermark_boost_factor" => values.watermark_boost_factor = Some(v),
            "vm.watermark_scale_factor" => values.watermark_scale_factor = Some(v),
            "vm.page-cluster" => values.page_cluster = Some(v),
            _ => {}
        }
    }
    values
}

fn merge_sysctl_overlay(base: SysctlValues, overlay: &SysctlValues) -> SysctlValues {
    SysctlValues {
        swappiness: overlay.swappiness.or(base.swappiness),
        watermark_boost_factor: overlay
            .watermark_boost_factor
            .or(base.watermark_boost_factor),
        watermark_scale_factor: overlay
            .watermark_scale_factor
            .or(base.watermark_scale_factor),
        page_cluster: overlay.page_cluster.or(base.page_cluster),
    }
}

/// Validate sysctl values against kernel-aligned ranges (matches GUI spin caps).
pub fn validate_sysctl_values(values: &SysctlValues) -> Result<()> {
    if let Some(v) = values.swappiness {
        if v > 200 {
            return Err(XzramError::Validation(format!(
                "vm.swappiness must be 0–200 (got {v})"
            )));
        }
    }
    if let Some(v) = values.watermark_boost_factor {
        if v > 10000 {
            return Err(XzramError::Validation(format!(
                "vm.watermark_boost_factor must be 0–10000 (got {v})"
            )));
        }
    }
    if let Some(v) = values.watermark_scale_factor {
        if v > 10000 {
            return Err(XzramError::Validation(format!(
                "vm.watermark_scale_factor must be 0–10000 (got {v})"
            )));
        }
    }
    if let Some(v) = values.page_cluster {
        if v > 8 {
            return Err(XzramError::Validation(format!(
                "vm.page-cluster must be 0–8 (got {v})"
            )));
        }
    }
    Ok(())
}

pub fn set(values: &SysctlValues) -> Result<()> {
    validate_sysctl_values(values)?;
    let path = etc_path(SYSCTL_FILE);
    let merged = merge_sysctl_overlay(read_drop_in(&path), values);
    validate_sysctl_values(&merged)?;

    let mut lines = Vec::new();
    if let Some(v) = merged.swappiness {
        lines.push(format!("vm.swappiness = {v}"));
    }
    if let Some(v) = merged.watermark_boost_factor {
        lines.push(format!("vm.watermark_boost_factor = {v}"));
    }
    if let Some(v) = merged.watermark_scale_factor {
        lines.push(format!("vm.watermark_scale_factor = {v}"));
    }
    if let Some(v) = merged.page_cluster {
        lines.push(format!("vm.page-cluster = {v}"));
    }

    if lines.is_empty() {
        return Err(XzramError::Validation("No sysctl values provided".into()));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!("{}\n", lines.join("\n"));
    std::fs::write(&path, content)?;

    // Skip live reload under hermetic etc roots (unit tests / fixtures).
    if std::env::var_os("XZRAM_ETC_ROOT").is_none() {
        apply::run_command("sysctl", &["--system"])?;
    }
    Ok(())
}

pub fn zram_tuning_defaults() -> SysctlValues {
    SysctlValues {
        swappiness: Some(180),
        watermark_boost_factor: Some(0),
        watermark_scale_factor: Some(125),
        page_cluster: Some(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_sys_path_maps_dots_to_slashes() {
        assert_eq!(proc_sys_path("vm.swappiness"), "/proc/sys/vm/swappiness");
        assert_eq!(
            proc_sys_path("vm.page-cluster"),
            "/proc/sys/vm/page-cluster"
        );
        assert_eq!(
            proc_sys_path("vm.watermark_scale_factor"),
            "/proc/sys/vm/watermark_scale_factor"
        );
    }

    #[test]
    fn set_merges_existing_drop_in_keys() {
        let _guard = crate::test_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let etc = dir.path().join("etc");
        std::fs::create_dir_all(etc.join("sysctl.d")).unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", &etc);
        let path = etc.join("sysctl.d/99-xzram.conf");
        std::fs::write(
            &path,
            "vm.swappiness = 60\nvm.page-cluster = 0\nvm.watermark_scale_factor = 125\n",
        )
        .unwrap();

        set(&SysctlValues {
            swappiness: Some(180),
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: None,
        })
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("vm.swappiness = 180"));
        assert!(content.contains("vm.page-cluster = 0"));
        assert!(content.contains("vm.watermark_scale_factor = 125"));
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn validate_sysctl_accepts_gui_ranges() {
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: Some(200),
            watermark_boost_factor: Some(10000),
            watermark_scale_factor: Some(0),
            page_cluster: Some(8),
        })
        .is_ok());
        assert!(validate_sysctl_values(&zram_tuning_defaults()).is_ok());
    }

    #[test]
    fn validate_sysctl_rejects_out_of_range() {
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: Some(201),
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: None,
        })
        .is_err());
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: None,
            watermark_boost_factor: Some(10001),
            watermark_scale_factor: None,
            page_cluster: None,
        })
        .is_err());
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: None,
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: Some(9),
        })
        .is_err());
    }
}
