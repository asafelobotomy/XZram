//! RAM / swap health checks: OOM protection, past OOM kills, memory pressure, swap
//! headroom, disk overflow size behind zram, and disk swap placement.
//!
//! Each check takes already-probed data so it is testable without a live host.

use super::{DoctorIssue, IssueSeverity};
use crate::oom::{MemoryPressure, OomProbe};
use crate::status::{format_bytes, StatusReport};

const GIB: u64 = 1024 * 1024 * 1024;
/// Total swap use at which a further allocation burst can exhaust it.
const SWAP_NEARLY_FULL_RATIO: f64 = 0.8;
/// Disk overflow behind zram smaller than min(RAM / 2, this) is flagged.
const OVERFLOW_SMALL_CAP_BYTES: u64 = 4 * GIB;

fn issue(severity: IssueSeverity, code: &str, message: String, suggestion: &str) -> DoctorIssue {
    DoctorIssue {
        severity,
        code: code.into(),
        message,
        suggestion: Some(suggestion.into()),
        action: None,
    }
}

pub(super) fn check_oom_protection(issues: &mut Vec<DoctorIssue>, probe: &OomProbe) {
    if probe.session_at_risk() {
        issues.push(issue(
            IssueSeverity::Warning,
            "oomd_session_unprotected",
            "systemd-oomd can kill inside user@.service but session.slice is not shielded; \
             a pressure spike can end the whole desktop session"
                .into(),
            "Run 'xzram defaults apply' to set ManagedOOMPreference=avoid on session.slice",
        ));
    }
    if probe.protected() {
        return;
    }
    if probe.oomd_active {
        issues.push(issue(
            IssueSeverity::Warning,
            "oomd_no_policies",
            "systemd-oomd is running but no unit has a ManagedOOM kill policy, so it never acts"
                .into(),
            "Run 'xzram defaults apply' to add ManagedOOMSwap / ManagedOOMMemoryPressure drop-ins",
        ));
        return;
    }
    let suggestion = if !probe.psi_available {
        "The kernel lacks PSI (needed by systemd-oomd); install and enable earlyoom"
    } else if !probe.oomd_installed {
        "Install systemd-oomd (or earlyoom), then run 'xzram defaults apply'"
    } else {
        "Run 'xzram defaults apply' to enable systemd-oomd with ManagedOOM kill policies"
    };
    issues.push(issue(
        IssueSeverity::Warning,
        "no_oom_protection",
        "No userspace OOM killer is active; when RAM and swap run out the desktop freezes \
         until the kernel OOM killer fires"
            .into(),
        suggestion,
    ));
}

pub(super) fn check_oom_kills(
    issues: &mut Vec<DoctorIssue>,
    this_boot: Option<u64>,
    previous_boot: Option<usize>,
) {
    let (now, prev) = (this_boot.unwrap_or(0), previous_boot.unwrap_or(0));
    if now == 0 && prev == 0 {
        return;
    }
    issues.push(issue(
        IssueSeverity::Warning,
        "oom_kills",
        format!("Kernel OOM killer fired {now} time(s) this boot and {prev} in the previous boot"),
        "RAM and swap were exhausted. Grow disk overflow swap and enable an early OOM killer: \
         'xzram defaults apply' (add --swap-scale high for a larger swapfile)",
    ));
}

pub(super) fn check_memory_pressure(issues: &mut Vec<DoctorIssue>, psi: Option<MemoryPressure>) {
    let Some(psi) = psi else {
        return;
    };
    if psi.full_avg60 >= 10.0 {
        issues.push(issue(
            IssueSeverity::Warning,
            "memory_pressure_high",
            format!(
                "All tasks were stalled on memory {:.1}% of the last minute (PSI full avg60)",
                psi.full_avg60
            ),
            "The system is thrashing; close memory-heavy apps. Persistent pressure means RAM is \
             undersized for this workload",
        ));
    } else if psi.some_avg300 >= 10.0 {
        issues.push(issue(
            IssueSeverity::Info,
            "memory_pressure_elevated",
            format!(
                "Some tasks were stalled on memory {:.1}% of the last 5 minutes (PSI some avg300)",
                psi.some_avg300
            ),
            "Expect occasional stutter; check zram/swap headroom with 'xzram status'",
        ));
    }
}

pub(super) fn check_swap_headroom(issues: &mut Vec<DoctorIssue>, status: &StatusReport) {
    let total: u64 = status.swaps.iter().map(|s| s.size_bytes).sum();
    let used: u64 = status.swaps.iter().map(|s| s.used_bytes).sum();
    if total == 0 || (used as f64) < total as f64 * SWAP_NEARLY_FULL_RATIO {
        return;
    }
    issues.push(issue(
        IssueSeverity::Warning,
        "swap_nearly_full",
        format!(
            "Swap is {:.0}% used ({} / {}); a further burst can exhaust it and trigger OOM kills",
            used as f64 / total as f64 * 100.0,
            format_bytes(used),
            format_bytes(total)
        ),
        "Grow disk overflow swap ('xzram defaults apply --swap-scale high') or reduce memory use",
    ));
}

/// zram lives in RAM; once it fills, only disk swap stands between the system and OOM.
pub(super) fn check_overflow_size(issues: &mut Vec<DoctorIssue>, status: &StatusReport) {
    let zram_active = status.swaps.iter().any(|s| s.name.contains("zram"));
    if !zram_active {
        return;
    }
    let disk: u64 = status
        .swaps
        .iter()
        .filter(|s| !s.name.contains("zram"))
        .map(|s| s.size_bytes)
        .sum();
    let ram = status.memory.mem_total_kb * 1024;
    let floor = (ram / 2).min(OVERFLOW_SMALL_CAP_BYTES);
    if disk == 0 {
        issues.push(issue(
            IssueSeverity::Info,
            "no_disk_overflow",
            "No disk-backed swap behind zram; when zram fills, the next step is the OOM killer"
                .into(),
            "Run 'xzram defaults apply' to add a low-priority overflow swapfile",
        ));
    } else if disk < floor {
        issues.push(issue(
            IssueSeverity::Warning,
            "disk_overflow_small",
            format!(
                "Only {} of disk swap behind zram ({} RAM); it fills quickly once zram is full",
                format_bytes(disk),
                format_bytes(ram)
            ),
            "Run 'xzram defaults apply' to grow /swap/swapfile (add --swap-scale high for up to \
             16 GiB)",
        ));
    }
}

/// THP `always` backs sparse anonymous mappings with 2 MiB pages, inflating RSS; on
/// smaller machines that memory is better left to zram and the page cache.
pub(super) fn check_thp_mode(issues: &mut Vec<DoctorIssue>, mode: Option<&str>, ram_bytes: u64) {
    const THP_ALWAYS_RAM_LIMIT: u64 = 16 * GIB;
    if mode != Some("always") || ram_bytes >= THP_ALWAYS_RAM_LIMIT {
        return;
    }
    issues.push(issue(
        IssueSeverity::Info,
        "thp_always_low_ram",
        format!(
            "Transparent huge pages are 'always' on a {} RAM system; sparse mappings get \
             2 MiB pages and use more memory",
            format_bytes(ram_bytes)
        ),
        "Consider transparent_hugepage=madvise on the kernel cmdline (apps that benefit opt \
         in with MADV_HUGEPAGE)",
    ));
}

pub(super) fn check_swap_placement(issues: &mut Vec<DoctorIssue>, status: &StatusReport) {
    for swap in status.swaps.iter().filter(|s| !s.name.contains("zram")) {
        if crate::placement::is_rotational_swap(&swap.name) != Some(true) {
            continue;
        }
        issues.push(issue(
            IssueSeverity::Warning,
            "swap_on_rotational",
            format!("Disk swap {} is on a rotational drive (HDD)", swap.name),
            "Paging to spinning media stalls the desktop; move disk swap to an SSD/NVMe \
             filesystem",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{MemoryInfo, SwapEntry};

    fn status(swaps: &[(&str, u64, u64)]) -> StatusReport {
        StatusReport {
            swaps: swaps
                .iter()
                .map(|(name, size, used)| SwapEntry {
                    name: (*name).into(),
                    swap_type: "file".into(),
                    size_bytes: *size,
                    used_bytes: *used,
                    priority: 0,
                })
                .collect(),
            zram_devices: vec![],
            memory: MemoryInfo {
                mem_total_kb: 16 * 1024 * 1024,
                mem_available_kb: 8 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        }
    }

    fn codes(issues: &[DoctorIssue]) -> Vec<&str> {
        issues.iter().map(|i| i.code.as_str()).collect()
    }

    #[test]
    fn oom_protection_variants() {
        let mut issues = Vec::new();
        let base = OomProbe {
            oomd_installed: true,
            psi_available: true,
            ..Default::default()
        };
        check_oom_protection(&mut issues, &base);
        check_oom_protection(
            &mut issues,
            &OomProbe {
                oomd_active: true,
                ..base.clone()
            },
        );
        check_oom_protection(
            &mut issues,
            &OomProbe {
                oomd_active: true,
                oomd_has_policies: true,
                ..base
            },
        );
        assert_eq!(codes(&issues), ["no_oom_protection", "oomd_no_policies"]);
    }

    #[test]
    fn oom_kills_reported_from_either_boot() {
        let mut issues = Vec::new();
        check_oom_kills(&mut issues, Some(0), Some(0));
        check_oom_kills(&mut issues, None, None);
        assert!(issues.is_empty());
        check_oom_kills(&mut issues, Some(0), Some(1));
        assert_eq!(codes(&issues), ["oom_kills"]);
        assert!(issues[0].message.contains("1 in the previous boot"));
    }

    #[test]
    fn memory_pressure_thresholds() {
        let mut issues = Vec::new();
        let calm = MemoryPressure::default();
        check_memory_pressure(&mut issues, Some(calm));
        check_memory_pressure(&mut issues, None);
        assert!(issues.is_empty());
        check_memory_pressure(
            &mut issues,
            Some(MemoryPressure {
                some_avg300: 15.0,
                full_avg60: 2.0,
            }),
        );
        check_memory_pressure(
            &mut issues,
            Some(MemoryPressure {
                some_avg300: 40.0,
                full_avg60: 25.0,
            }),
        );
        assert_eq!(
            codes(&issues),
            ["memory_pressure_elevated", "memory_pressure_high"]
        );
    }

    #[test]
    fn swap_headroom_warns_above_80_percent() {
        let mut issues = Vec::new();
        check_swap_headroom(&mut issues, &status(&[("/dev/zram0", 100, 70)]));
        check_swap_headroom(&mut issues, &status(&[]));
        assert!(issues.is_empty());
        check_swap_headroom(
            &mut issues,
            &status(&[("/dev/zram0", 100, 100), ("/swap/swapfile", 100, 70)]),
        );
        assert_eq!(codes(&issues), ["swap_nearly_full"]);
    }

    #[test]
    fn overflow_size_behind_zram() {
        let mut issues = Vec::new();
        // No zram: not our concern.
        check_overflow_size(&mut issues, &status(&[("/swap/swapfile", 512 << 20, 0)]));
        // zram + adequately sized disk overflow.
        check_overflow_size(
            &mut issues,
            &status(&[("/dev/zram0", 8 * GIB, 0), ("/swap/swapfile", 8 * GIB, 0)]),
        );
        assert!(issues.is_empty());

        check_overflow_size(&mut issues, &status(&[("/dev/zram0", 8 * GIB, 0)]));
        check_overflow_size(
            &mut issues,
            &status(&[("/dev/zram0", 8 * GIB, 0), ("/swap/swapfile", 512 << 20, 0)]),
        );
        assert_eq!(codes(&issues), ["no_disk_overflow", "disk_overflow_small"]);
        assert_eq!(issues[1].severity, IssueSeverity::Warning);
    }

    #[test]
    fn thp_always_flagged_only_below_16_gib() {
        let mut issues = Vec::new();
        check_thp_mode(&mut issues, Some("madvise"), 8 * GIB);
        check_thp_mode(&mut issues, Some("always"), 32 * GIB);
        check_thp_mode(&mut issues, None, 8 * GIB);
        assert!(issues.is_empty());
        check_thp_mode(&mut issues, Some("always"), 15 * GIB);
        assert_eq!(codes(&issues), ["thp_always_low_ram"]);
    }
}
