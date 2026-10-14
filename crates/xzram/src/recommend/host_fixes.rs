//! Host fixes staged with recommended defaults: turn zswap off when zram is the compressed
//! tier, and set btrfs nodatacow on the parent of existing swapfiles.
//!
//! These probe live sysfs / lsattr, so they run after the pure builder
//! (`recommend_from_context_scaled`) and take injected [`HostProbes`] in tests.

use crate::apply::pending_is_empty;
use crate::checks;
use crate::status::StatusReport;

use super::engine_notes::note_item;
use super::types::{RecommendationItem, RecommendedDefaults};

const ALREADY_MATCHES: &str = "System already matches recommended defaults";

#[derive(Debug, Clone, Default)]
pub(crate) struct HostProbes {
    pub zswap_enabled: Option<bool>,
    /// Swapfiles on btrfs whose parent directory lacks nodatacow (prepare-path validated).
    pub swapfiles_missing_nodatacow: Vec<String>,
    pub oom: crate::oom::OomProbe,
    /// Kernel OOM kills in this boot plus the previous one.
    pub recent_oom_kills: u64,
}

pub(crate) fn probe_host() -> HostProbes {
    let backend = crate::backend::available_swapfile_backend();
    let swapfiles_missing_nodatacow = backend
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|file| file.path)
        .filter(|path| crate::validation::validate_swapfile_prepare_path(path).is_ok())
        .filter(|path| {
            crate::swapfile_btrfs::check_nodatacow(std::path::Path::new(path))
                .is_ok_and(|s| s.on_btrfs && s.parent_nodatacow == Some(false))
        })
        .collect();
    let recent_oom_kills = crate::oom::oom_kills_this_boot().unwrap_or(0)
        + crate::oom::oom_kills_previous_boot().unwrap_or(0) as u64;
    HostProbes {
        zswap_enabled: checks::zswap_enabled(),
        swapfiles_missing_nodatacow,
        oom: crate::oom::probe(),
        recent_oom_kills,
    }
}

pub(crate) fn stage_host_fixes(
    report: &mut RecommendedDefaults,
    status: &StatusReport,
    probes: &HostProbes,
) {
    let staging_blocked = !report.context.etc_writable || report.context.immutable_os;
    let mut items = Vec::new();

    let zram_in_use = !status.zram_devices.is_empty() || report.pending.zram.is_some();
    if zram_in_use && probes.zswap_enabled == Some(true) {
        if staging_blocked {
            items.push(note_item(
                "Disable zswap when using zram",
                "zswap compresses pages before they reach zram, so zram is largely bypassed. Staging is blocked here; add zswap.enabled=0 to the kernel cmdline instead.",
                Some("known-conflicts"),
            ));
        } else {
            items.push(RecommendationItem {
                category: "zswap".into(),
                summary: "Disable zswap (zram is the compressed swap tier)".into(),
                detail: format!(
                    "zswap compresses pages before they reach zram, so zram is largely bypassed. Apply turns zswap off now and keeps it off at boot via /etc/{} (no bootloader changes).",
                    crate::zswap::ZSWAP_TMPFILES
                ),
                will_stage: true,
                reference: Some("known-conflicts".into()),
            });
            report.pending.disable_zswap = true;
        }
    }

    if !staging_blocked {
        for path in &probes.swapfiles_missing_nodatacow {
            items.push(RecommendationItem {
                category: "swapfile".into(),
                summary: format!("Set btrfs nodatacow on the parent of {path}"),
                detail: format!(
                    "{path} is on btrfs but its directory lacks nodatacow (chattr +C), so a recreated or resized swapfile would fail to activate. Apply runs 'xzram swapfile prepare {path}'."
                ),
                will_stage: true,
                reference: Some("doctor-mapping".into()),
            });
            report.pending.prepare_swapfiles.push(path.clone());
        }
    }

    super::oom_fixes::stage_oom_fixes(
        &mut items,
        &mut report.pending,
        &probes.oom,
        probes.recent_oom_kills,
        staging_blocked,
    );

    if !pending_is_empty(&report.pending) {
        report.items.retain(|i| i.summary != ALREADY_MATCHES);
    }
    report.items.splice(0..0, items);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::PendingConfig;
    use crate::recommend::scales::{build_size_scales, RecommendScales};
    use crate::recommend::types::{RecommendProfile, SystemContext};
    use crate::status::{MemoryInfo, ZramDevice};

    fn report(etc_writable: bool) -> RecommendedDefaults {
        RecommendedDefaults {
            pending: PendingConfig::default(),
            items: vec![note_item(ALREADY_MATCHES, "", None)],
            context: SystemContext {
                mem_total_bytes: 16 << 30,
                mem_available_bytes: 8 << 30,
                has_active_zram: true,
                has_disk_swap: true,
                distro: "Test".into(),
                root_filesystem: Some("btrfs".into()),
                zram_backend: "systemdzramgenerator".into(),
                profile: "conservative".into(),
                immutable_os: false,
                etc_writable,
            },
            size_scales: build_size_scales(
                RecommendProfile::Conservative,
                16 * 1024 * 1024,
                RecommendScales::default(),
                false,
            ),
        }
    }

    fn status_with_zram() -> StatusReport {
        StatusReport {
            swaps: vec![],
            zram_devices: vec![ZramDevice {
                name: "zram0".into(),
                algorithm: "zstd".into(),
                disk_size_bytes: 8 << 30,
                data_bytes: 0,
                compressed_bytes: 0,
                total_bytes: 0,
                mem_used_max_bytes: 0,
                mount_point: "[SWAP]".into(),
            }],
            memory: MemoryInfo {
                mem_total_kb: 16 * 1024 * 1024,
                mem_available_kb: 8 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        }
    }

    #[test]
    fn stages_zswap_and_nodatacow_and_drops_already_matches() {
        let mut r = report(true);
        let probes = HostProbes {
            zswap_enabled: Some(true),
            swapfiles_missing_nodatacow: vec!["/swap/swapfile".into()],
            ..Default::default()
        };
        stage_host_fixes(&mut r, &status_with_zram(), &probes);
        assert!(r.pending.disable_zswap);
        assert_eq!(r.pending.prepare_swapfiles, vec!["/swap/swapfile"]);
        assert!(r.items.iter().all(|i| i.summary != ALREADY_MATCHES));
        assert_eq!(r.items.iter().filter(|i| i.will_stage).count(), 2);
    }

    #[test]
    fn zswap_ignored_without_zram_or_when_already_off() {
        let mut r = report(true);
        let mut status = status_with_zram();
        status.zram_devices.clear();
        let on = HostProbes {
            zswap_enabled: Some(true),
            ..Default::default()
        };
        stage_host_fixes(&mut r, &status, &on);
        assert!(!r.pending.disable_zswap);

        let off = HostProbes {
            zswap_enabled: Some(false),
            ..Default::default()
        };
        stage_host_fixes(&mut r, &status_with_zram(), &off);
        assert!(!r.pending.disable_zswap);
        assert!(r.items.iter().any(|i| i.summary == ALREADY_MATCHES));
    }

    #[test]
    fn blocked_staging_only_adds_advice() {
        let mut r = report(false);
        let probes = HostProbes {
            zswap_enabled: Some(true),
            swapfiles_missing_nodatacow: vec!["/swap/swapfile".into()],
            ..Default::default()
        };
        stage_host_fixes(&mut r, &status_with_zram(), &probes);
        assert!(pending_is_empty(&r.pending));
        assert!(r
            .items
            .iter()
            .any(|i| i.summary.contains("zswap") && !i.will_stage));
    }
}
