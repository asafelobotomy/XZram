use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, XzramError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysctlValues {
    pub swappiness: Option<u32>,
    pub watermark_boost_factor: Option<u32>,
    pub watermark_scale_factor: Option<u32>,
    pub page_cluster: Option<u32>,
}

const SYSCTL_PATH: &str = "/etc/sysctl.d/99-xzram.conf";

pub fn show() -> Result<SysctlValues> {
    Ok(SysctlValues {
        swappiness: read_sysctl("vm.swappiness"),
        watermark_boost_factor: read_sysctl("vm.watermark_boost_factor"),
        watermark_scale_factor: read_sysctl("vm.watermark_scale_factor"),
        page_cluster: read_sysctl("vm.page-cluster"),
    })
}

fn read_sysctl(key: &str) -> Option<u32> {
    let path = format!("/proc/sys/{key}");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn set(values: &SysctlValues) -> Result<()> {
    let mut lines = Vec::new();
    if let Some(v) = values.swappiness {
        lines.push(format!("vm.swappiness = {v}"));
    }
    if let Some(v) = values.watermark_boost_factor {
        lines.push(format!("vm.watermark_boost_factor = {v}"));
    }
    if let Some(v) = values.watermark_scale_factor {
        lines.push(format!("vm.watermark_scale_factor = {v}"));
    }
    if let Some(v) = values.page_cluster {
        lines.push(format!("vm.page-cluster = {v}"));
    }

    if lines.is_empty() {
        return Err(XzramError::Validation("No sysctl values provided".into()));
    }

    let content = format!("{}\n", lines.join("\n"));
    std::fs::write(SYSCTL_PATH, content)?;

    crate::apply::run_command("sysctl", &["--system"])?;
    Ok(())
}

pub fn parse_set_args(args: &HashMap<String, String>) -> Result<SysctlValues> {
    let parse = |key: &str| -> Result<Option<u32>> {
        match args.get(key) {
            Some(v) => v
                .parse()
                .map(Some)
                .map_err(|_| XzramError::Parse(format!("invalid value for {key}"))),
            None => Ok(None),
        }
    };

    Ok(SysctlValues {
        swappiness: parse("swappiness")?,
        watermark_boost_factor: parse("watermark-boost-factor")?,
        watermark_scale_factor: parse("watermark-scale-factor")?,
        page_cluster: parse("page-cluster")?,
    })
}

pub fn zram_tuning_defaults() -> SysctlValues {
    SysctlValues {
        swappiness: Some(180),
        watermark_boost_factor: Some(0),
        watermark_scale_factor: Some(125),
        page_cluster: Some(0),
    }
}

pub fn apply_zram_tuning() -> Result<()> {
    set(&zram_tuning_defaults())
}
