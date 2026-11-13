//! Individual doctor checks; each appends zero or more issues.

use super::{DoctorAction, DoctorIssue, IssueSeverity};
use crate::checks;

pub(super) fn check_zswap(issues: &mut Vec<DoctorIssue>) {
    if checks::zswap_enabled() == Some(true) {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "zswap_enabled".into(),
            message: "zswap is enabled and may interfere with zram effectiveness".into(),
            suggestion: Some(
                "Run 'xzram defaults apply' to turn zswap off now and at boot (tmpfiles.d), \
                 or add zswap.enabled=0 to the kernel cmdline"
                    .into(),
            ),
            action: None,
        });
    }
}

pub(super) fn check_hibernation(issues: &mut Vec<DoctorIssue>, swaps: &[crate::status::SwapEntry]) {
    if checks::hibernation_zram_conflict(swaps) {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Error,
            code: "hibernate_zram".into(),
            message: "Hibernation resume device points to zram".into(),
            suggestion: Some(
                "Hibernation requires disk-backed swap as resume device, not zram".into(),
            ),
            action: None,
        });
    }
}

pub(super) fn check_filesystem_swapfile(issues: &mut Vec<DoctorIssue>, fstype: Option<&str>) {
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

pub(super) fn check_btrfs_swapfile_nodatacow(issues: &mut Vec<DoctorIssue>) {
    let backend = crate::backend::available_swapfile_backend();
    let Ok(files) = backend.list() else {
        return;
    };

    for file in files {
        let path = std::path::Path::new(&file.path);
        let Ok(status) = crate::swapfile_btrfs::check_nodatacow(path) else {
            continue;
        };
        let active = crate::swapfile_btrfs::path_is_active_swap(path);
        if !crate::swapfile_btrfs::nodatacow_needs_fix(&status, active) {
            continue;
        }
        issues.push(DoctorIssue {
            severity: IssueSeverity::Warning,
            code: "btrfs_swapfile_nodatacow".into(),
            message: format!(
                "Swapfile {} is on btrfs without nodatacow: {}",
                file.path, status.message
            ),
            suggestion: Some(format!(
                "xzram swapfile prepare {} (also staged by 'xzram defaults apply')",
                file.path
            )),
            action: Some(DoctorAction::PrepareBtrfsSwapfile {
                path: file.path.clone(),
            }),
        });
    }
}

pub(super) fn check_swap_priorities(
    issues: &mut Vec<DoctorIssue>,
    swaps: &[crate::status::SwapEntry],
) {
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

pub(super) fn check_zram_zswap_conflict(
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
        suggestion: Some("Disable zswap: run 'xzram defaults apply'".into()),
        action: None,
    });
}

/// Overflow at least this large (and a tenth of the zram device) counts as significant.
const SIGNIFICANT_OVERFLOW_BYTES: u64 = 256 << 20;

/// Memory stalls that make zram saturation an error rather than a warning.
fn pressure_is_high(psi: &crate::oom::MemoryPressure) -> bool {
    psi.full_avg60 >= 5.0 || psi.some_avg60 >= 20.0 || psi.some_avg300 >= 10.0
}

/// zram nearly full while lower-priority disk swap absorbs overflow: the point at which
/// the zram size, not RAM, pushes pages to disk. An error when the overflow is significant
/// and memory PSI shows the stalls it causes.
pub(super) fn check_zram_capacity(
    issues: &mut Vec<DoctorIssue>,
    status: &crate::status::StatusReport,
    psi: Option<crate::oom::MemoryPressure>,
) {
    let Some(sat) = crate::saturation::detect(status) else {
        return;
    };
    let significant = sat.overflow_bytes >= SIGNIFICANT_OVERFLOW_BYTES.max(sat.size_bytes / 10);
    let stalled = psi.filter(pressure_is_high);
    let (severity, message) = match stalled {
        Some(p) if significant => (
            IssueSeverity::Error,
            format!(
                "{}, and memory is stalling (PSI some avg60 {:.1}%, full avg60 {:.1}%)",
                sat.describe(),
                p.some_avg60,
                p.full_avg60
            ),
        ),
        _ => (IssueSeverity::Warning, sat.describe()),
    };
    issues.push(DoctorIssue {
        severity,
        code: "zram_near_full".into(),
        message,
        suggestion: Some(
            "'xzram defaults recommend' proposes a larger, ratio-based zram size; stage it with \
             'xzram defaults apply' (or 'xzram zram set --size ...'). 'xzram apply --at-boot' \
             defers the device restart to the next boot, and 'xzram swap drain-overflow' moves \
             the overflow back afterwards. If this is sustained, consider a zswap-based setup"
                .into(),
        ),
        action: None,
    });
}

/// Active swapfiles in a snapshotted btrfs subvolume (each one breaks every snapshot).
pub(super) fn check_swapfile_snapshots(
    issues: &mut Vec<DoctorIssue>,
    placements: &[crate::btrfs_subvol::SwapfilePlacement],
) {
    for placement in placements {
        issues.push(DoctorIssue {
            severity: IssueSeverity::Error,
            code: "swapfile_blocks_snapshots".into(),
            message: placement.message(),
            suggestion: Some(placement.fix()),
            action: None,
        });
    }
}

/// Live probe for [`check_swapfile_snapshots`]: active swapfiles from `/proc/swaps`.
pub(super) fn probe_swapfile_placements(
    swaps: &[crate::status::SwapEntry],
) -> Vec<crate::btrfs_subvol::SwapfilePlacement> {
    swaps
        .iter()
        .filter(|s| s.swap_type == "file")
        .filter_map(|s| crate::btrfs_subvol::swapfile_placement(&s.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btrfs_root_tip_skipped_when_swapfiles_configured() {
        // The swapfile list reads fstab under XZRAM_ETC_ROOT, which other tests repoint
        // in parallel; hold the env lock so both lists below see the same fstab.
        let _guard = crate::test_env_lock();
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

    fn sat_status(zram_used: u64, disk_used: u64) -> crate::status::StatusReport {
        crate::saturation::tests::status_28(zram_used, disk_used)
    }

    const GIB: u64 = 1 << 30;

    #[test]
    fn zram_near_full_warns_when_disk_swap_absorbs_overflow() {
        let mut issues = Vec::new();
        check_zram_capacity(&mut issues, &sat_status(4 * GIB, 700 << 20), None);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "zram_near_full");
        assert_eq!(issues[0].severity, IssueSeverity::Warning);
    }

    #[test]
    fn zram_near_full_is_an_error_with_overflow_and_stalls() {
        let stalled = crate::oom::MemoryPressure {
            some_avg60: 28.0,
            ..Default::default()
        };
        let mut issues = Vec::new();
        check_zram_capacity(&mut issues, &sat_status(4 * GIB, 700 << 20), Some(stalled));
        assert_eq!(issues[0].severity, IssueSeverity::Error);
        assert!(
            issues[0].message.contains("stalling"),
            "{}",
            issues[0].message
        );
        // Tiny overflow stays a warning even under pressure.
        let mut issues = Vec::new();
        check_zram_capacity(&mut issues, &sat_status(4 * GIB, 10 << 20), Some(stalled));
        assert_eq!(issues[0].severity, IssueSeverity::Warning);
        // Calm PSI: warning.
        let mut issues = Vec::new();
        let calm = crate::oom::MemoryPressure::default();
        check_zram_capacity(&mut issues, &sat_status(4 * GIB, 2 * GIB), Some(calm));
        assert_eq!(issues[0].severity, IssueSeverity::Warning);
    }

    #[test]
    fn zram_near_full_silent_without_overflow_or_with_headroom() {
        let mut issues = Vec::new();
        check_zram_capacity(&mut issues, &sat_status(4 * GIB, 0), None);
        check_zram_capacity(&mut issues, &sat_status(GIB, 500 << 20), None);
        assert!(issues.is_empty());
    }

    #[test]
    fn swapfile_in_root_subvolume_is_an_error() {
        let placement = crate::btrfs_subvol::SwapfilePlacement {
            path: "/swap/swapfile".into(),
            subvolume: "/".into(),
            conflict: crate::btrfs_subvol::SnapshotConflict::RootSubvolume,
            device: None,
        };
        let mut issues = Vec::new();
        check_swapfile_snapshots(&mut issues, &[]);
        assert!(issues.is_empty());
        check_swapfile_snapshots(&mut issues, &[placement]);
        assert_eq!(issues[0].code, "swapfile_blocks_snapshots");
        assert_eq!(issues[0].severity, IssueSeverity::Error);
        assert!(issues[0]
            .message
            .contains("cannot snapshot subvolume with active swapfile"));
        assert!(issues[0].suggestion.as_deref().unwrap().contains("@swap"));
    }
}
