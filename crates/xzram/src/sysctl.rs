use serde::{Deserialize, Serialize};

use crate::apply::{self, SYSCTL_PATH};
use crate::error::{Result, XzramError};

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

    apply::run_command("sysctl", &["--system"])?;
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
}
