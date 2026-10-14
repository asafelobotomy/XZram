use serde::{Deserialize, Serialize};

mod memory;
mod rules;

use crate::detect::{detect, ZramBackend};
use crate::error::Result;
use crate::status::status;
use rules::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorIssue {
    pub severity: IssueSeverity,
    pub code: String,
    pub message: String,
    pub suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<DoctorAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum DoctorAction {
    PrepareBtrfsSwapfile { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub issues: Vec<DoctorIssue>,
}

pub fn doctor() -> Result<DoctorReport> {
    let mut issues = Vec::new();
    let detection = detect()?;
    let status = status()?;

    if detection.init_system != "systemd" {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "non_systemd".into(),
            message: format!("Init system is '{}', not systemd", detection.init_system),
            suggestion: Some("XZram v1 targets systemd-based distros".into()),
            action: None,
        });
    }

    if !detection.etc_writable || detection.immutable_os {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "immutable_etc".into(),
            message: "/etc is read-only or this OS uses an immutable layout".into(),
            suggestion: Some(
                "On immutable distros, layer packages with rpm-ostree or use a writable overlay. Privileged apply still uses polkit/xzram-helper when /etc is writable for root."
                    .into(),
            ),
            action: None,
        });
    }

    if detection.zram_backend == ZramBackend::None && status.zram_devices.is_empty() {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Info,
            code: "no_zram".into(),
            message: "No zram backend detected and no active zram devices".into(),
            suggestion: Some("Install zram-generator and run 'xzram zram set'".into()),
            action: None,
        });
    }

    if !detection.zram_generator_installed
        && detection.zram_backend == ZramBackend::SystemdZramGenerator
    {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "zram_generator_missing".into(),
            message: "zram-generator config exists but package may not be installed".into(),
            suggestion: Some(format!(
                "Install package: {}",
                crate::detect::zram_generator_package_name(detection.package_manager)
            )),
            action: None,
        });
    }

    if detection.zram_backend == ZramBackend::ZramTools {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "zram_tools_legacy".into(),
            message: "Legacy zram-tools configuration detected".into(),
            suggestion: Some(
                "Run 'xzram zram migrate' to stage zram-generator config, then 'xzram apply'"
                    .into(),
            ),
            action: None,
        });
    }

    if crate::migrate::zramswap_service_active() {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Info,
            code: "zramswap_service_active".into(),
            message: "Legacy zramswap.service is still active".into(),
            suggestion: Some(
                "Run 'xzram zram migrate' and apply to disable zram-tools in favor of zram-generator"
                    .into(),
            ),
            action: None,
        });
    }

    check_zswap(&mut issues);
    check_hibernation(&mut issues, &status.swaps);
    check_filesystem_swapfile(&mut issues, detection.root_filesystem.as_deref());
    check_btrfs_swapfile_nodatacow(&mut issues);
    check_swap_priorities(&mut issues, &status.swaps);
    check_zram_zswap_conflict(&mut issues, &status.zram_devices);
    check_zram_capacity(&mut issues, &status.swaps);
    let _ = crate::swap_partition::check_missing_swap_partitions(&mut issues);

    memory::check_oom_protection(&mut issues, &crate::oom::probe());
    memory::check_oom_kills(
        &mut issues,
        crate::oom::oom_kills_this_boot(),
        crate::oom::oom_kills_previous_boot(),
    );
    let psi = std::fs::read_to_string("/proc/pressure/memory").ok();
    memory::check_memory_pressure(
        &mut issues,
        psi.as_deref().and_then(crate::oom::parse_psi_memory),
    );
    memory::check_swap_headroom(&mut issues, &status);
    memory::check_overflow_size(&mut issues, &status);
    memory::check_swap_placement(&mut issues, &status);
    memory::check_thp_mode(
        &mut issues,
        crate::mm_tunables::thp_enabled_mode().as_deref(),
        status.memory.mem_total_kb * 1024,
    );

    let healthy = !issues.iter().any(|i| i.severity == IssueSeverity::Error);

    Ok(DoctorReport { healthy, issues })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_report_serializes() {
        let report = DoctorReport {
            healthy: true,
            issues: vec![],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("healthy"));
    }
}
