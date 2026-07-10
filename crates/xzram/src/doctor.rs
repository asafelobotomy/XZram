use serde::{Deserialize, Serialize};

use crate::detect::{detect, ZramBackend};
use crate::error::Result;
use crate::status::status;

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
        });
    }

    if detection.zram_backend == ZramBackend::None && status.zram_devices.is_empty() {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Info,
            code: "no_zram".into(),
            message: "No zram backend detected and no active zram devices".into(),
            suggestion: Some("Install zram-generator and run 'xzram zram set'".into()),
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
        });
    }

    check_zswap(&mut issues);
    check_hibernation(&mut issues);
    check_filesystem_swapfile(&mut issues, detection.root_filesystem.as_deref());
    check_swap_priorities(&mut issues, &status.swaps);
    check_zram_zswap_conflict(&mut issues, &status.zram_devices);

    let healthy = !issues.iter().any(|i| i.severity == IssueSeverity::Error);

    Ok(DoctorReport { healthy, issues })
}

fn check_zswap(issues: &mut Vec<DoctorIssue>) {
    let enabled_path = std::path::Path::new("/sys/module/zswap/parameters/enabled");
    if let Ok(content) = std::fs::read_to_string(enabled_path) {
        let enabled = content.trim();
        if enabled == "Y" || enabled == "1" {
            issues.push(DoctorIssue {
                severity: IssueSeverity::Warning,
                code: "zswap_enabled".into(),
                message: "zswap is enabled and may interfere with zram effectiveness".into(),
                suggestion: Some(
                    "Disable zswap: echo 0 | sudo tee /sys/module/zswap/parameters/enabled, \
                     or add zswap.enabled=0 to kernel cmdline"
                        .into(),
                ),
            });
        }
    }
}

fn check_hibernation(issues: &mut Vec<DoctorIssue>) {
    let resume_path = std::path::Path::new("/sys/power/resume");
    if let Ok(resume) = std::fs::read_to_string(resume_path) {
        let resume = resume.trim();
        if !resume.is_empty() && resume != "0:0" {
            let stat = status().ok();
            if let Some(stat) = stat {
                for swap in &stat.swaps {
                    if swap.name.contains("zram") {
                        issues.push(DoctorIssue {
                            severity: IssueSeverity::Warning,
                            code: "hibernate_zram".into(),
                            message: "Hibernation is configured but zram swap is active".into(),
                            suggestion: Some(
                                "Hibernation requires disk-backed swap as resume device, not zram"
                                    .into(),
                            ),
                        });
                        break;
                    }
                }
            }
        }
    }
}

fn check_filesystem_swapfile(issues: &mut Vec<DoctorIssue>, fstype: Option<&str>) {
    if let Some("btrfs") = fstype {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Info,
            code: "btrfs_root".into(),
            message: "Root filesystem is btrfs; swapfiles require nodatacow".into(),
            suggestion: Some(
                "Before creating swapfile: chattr +C <dir> and ensure file is not copy-on-write"
                    .into(),
            ),
        });
    }
    if let Some("zfs") = fstype {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "zfs_root".into(),
            message: "Root filesystem is ZFS; swapfiles have special requirements".into(),
            suggestion: Some("Prefer a dedicated swap partition or zvol on ZFS systems".into()),
        });
    }
}

fn check_swap_priorities(issues: &mut Vec<DoctorIssue>, swaps: &[crate::status::SwapEntry]) {
    if swaps.len() < 2 {
        return;
    }

    let zram_swaps: Vec<_> = swaps.iter().filter(|s| s.name.contains("zram")).collect();
    let disk_swaps: Vec<_> = swaps.iter().filter(|s| !s.name.contains("zram")).collect();

    if !zram_swaps.is_empty() && !disk_swaps.is_empty() {
        let zram_prio = zram_swaps.iter().map(|s| s.priority).max().unwrap_or(0);
        let disk_prio = disk_swaps.iter().map(|s| s.priority).max().unwrap_or(0);
        if disk_prio >= zram_prio {
            issues.push(DoctorIssue {
                severity: IssueSeverity::Warning,
                code: "priority_inverted".into(),
                message: format!("Disk swap priority ({disk_prio}) >= zram priority ({zram_prio})"),
                suggestion: Some(
                    "Set zram swap-priority higher (e.g. 100) than disk swap (e.g. 10)".into(),
                ),
            });
        }
    }
}

fn check_zram_zswap_conflict(
    issues: &mut Vec<DoctorIssue>,
    zram_devices: &[crate::status::ZramDevice],
) {
    if zram_devices.is_empty() {
        return;
    }
    let zswap_path = std::path::Path::new("/sys/module/zswap/parameters/enabled");
    if let Ok(content) = std::fs::read_to_string(zswap_path) {
        if content.trim() == "Y" {
            issues.push(DoctorIssue {
                severity: IssueSeverity::Error,
                code: "zram_zswap_conflict".into(),
                message: "Both zram and zswap are active; zram compression is largely bypassed"
                    .into(),
                suggestion: Some("Disable zswap before using zram".into()),
            });
        }
    }
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
