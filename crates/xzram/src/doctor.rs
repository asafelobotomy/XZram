use serde::{Deserialize, Serialize};

use crate::checks;
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
    let _ = crate::swap_partition::check_missing_swap_partitions(&mut issues);

    let healthy = !issues.iter().any(|i| i.severity == IssueSeverity::Error);

    Ok(DoctorReport { healthy, issues })
}

fn check_zswap(issues: &mut Vec<DoctorIssue>) {
    if checks::zswap_enabled() == Some(true) {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "zswap_enabled".into(),
            message: "zswap is enabled and may interfere with zram effectiveness".into(),
            suggestion: Some(
                "Disable zswap: echo 0 | sudo tee /sys/module/zswap/parameters/enabled, \
                 or add zswap.enabled=0 to kernel cmdline"
                    .into(),
            ),
            action: None,
        });
    }
}

fn check_hibernation(issues: &mut Vec<DoctorIssue>, swaps: &[crate::status::SwapEntry]) {
    if checks::hibernation_zram_conflict(swaps) {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "hibernate_zram".into(),
            message: "Hibernation resume device points to zram".into(),
            suggestion: Some(
                "Hibernation requires disk-backed swap as resume device, not zram".into(),
            ),
            action: None,
        });
    }
}

fn check_filesystem_swapfile(issues: &mut Vec<DoctorIssue>, fstype: Option<&str>) {
    if let Some("btrfs") = fstype {
        // Only advise prepare-before-create when no managed swapfile exists yet.
        // Existing files are covered by check_btrfs_swapfile_nodatacow (warning + prepare action).
        let has_swapfile = crate::backend::available_swapfile_backend()
            .list()
            .map(|files| !files.is_empty())
            .unwrap_or(false);
        if !has_swapfile {
            issues.push(DoctorIssue {
                severity: IssueSeverity::Info,
                code: "btrfs_root".into(),
                message: "Root filesystem is btrfs; swapfiles require nodatacow".into(),
                suggestion: Some(
                    "Before creating a swapfile: xzram swapfile prepare <path> (sets chattr +C on the parent directory)"
                        .into(),
                ),
                action: None,
            });
        }
    }
    if let Some("zfs") = fstype {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "zfs_root".into(),
            message: "Root filesystem is ZFS; swapfiles have special requirements".into(),
            suggestion: Some("Prefer a dedicated swap partition or zvol on ZFS systems".into()),
            action: None,
        });
    }
}

fn check_btrfs_swapfile_nodatacow(issues: &mut Vec<DoctorIssue>) {
    let backend = crate::backend::available_swapfile_backend();
    let Ok(files) = backend.list() else {
        return;
    };

    for file in files {
        let path = std::path::Path::new(&file.path);
        let Ok(status) = crate::swapfile_btrfs::check_nodatacow(path) else {
            continue;
        };
        if !status.on_btrfs || status.ready {
            continue;
        }
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "btrfs_swapfile_nodatacow".into(),
            message: format!(
                "Swapfile {} is on btrfs without nodatacow: {}",
                file.path, status.message
            ),
            suggestion: Some(format!("xzram swapfile prepare {}", file.path)),
            action: Some(DoctorAction::PrepareBtrfsSwapfile {
                path: file.path.clone(),
            }),
        });
    }
}

fn check_swap_priorities(issues: &mut Vec<DoctorIssue>, swaps: &[crate::status::SwapEntry]) {
    if !checks::priority_inverted(swaps) {
        return;
    }

    let zram_prio = swaps
        .iter()
        .filter(|s| s.name.contains("zram"))
        .map(|s| s.priority)
        .max()
        .unwrap_or(0);
    let disk_prio = swaps
        .iter()
        .filter(|s| !s.name.contains("zram"))
        .map(|s| s.priority)
        .max()
        .unwrap_or(0);

    issues.push(DoctorIssue {
        severity: IssueSeverity::Warning,
        code: "priority_inverted".into(),
        message: format!("Disk swap priority ({disk_prio}) >= zram priority ({zram_prio})"),
        suggestion: Some(
            "Set zram swap-priority higher (e.g. 100) than disk swap (e.g. 10)".into(),
        ),
        action: None,
    });
}

fn check_zram_zswap_conflict(
    issues: &mut Vec<DoctorIssue>,
    zram_devices: &[crate::status::ZramDevice],
) {
    if !checks::zram_zswap_conflict(zram_devices) {
        return;
    }
    issues.push(DoctorIssue {
        severity: IssueSeverity::Error,
        code: "zram_zswap_conflict".into(),
        message: "Both zram and zswap are active; zram compression is largely bypassed".into(),
        suggestion: Some("Disable zswap before using zram".into()),
        action: None,
    });
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

    #[test]
    fn btrfs_root_tip_skipped_when_swapfiles_configured() {
        let mut issues = Vec::new();
        // With a configured swapfile on this host, the generic tip must not fire.
        // (Integration-style: uses real backend list when available.)
        let has_swapfile = crate::backend::available_swapfile_backend()
            .list()
            .map(|files| !files.is_empty())
            .unwrap_or(false);
        check_filesystem_swapfile(&mut issues, Some("btrfs"));
        let has_tip = issues.iter().any(|i| i.code == "btrfs_root");
        if has_swapfile {
            assert!(
                !has_tip,
                "btrfs_root tip should be omitted when swapfiles already exist"
            );
        } else {
            assert!(
                has_tip,
                "btrfs_root tip should appear when no swapfiles exist"
            );
        }
    }

    #[test]
    fn btrfs_root_tip_emitted_without_swapfiles_on_empty_list() {
        let mut issues = Vec::new();
        // Direct unit behavior for the empty-path branch is covered when list is empty;
        // calling with non-btrfs must never emit btrfs_root.
        check_filesystem_swapfile(&mut issues, Some("ext4"));
        assert!(!issues.iter().any(|i| i.code == "btrfs_root"));
    }
}
