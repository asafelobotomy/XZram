//! Core recommendation builder (testable without live detect/status).

use crate::apply::{PendingConfig, ZramConfig};
use crate::checks;
use crate::detect::{DetectionReport, ZramBackend};
use crate::status::{self, StatusReport};
use crate::sysctl::{self, SysctlValues};

use super::engine_notes::{advisory_items, note_item};
use super::overflow::{
    available_bytes_near, decide_overflow_swapfile, has_active_disk_swap,
    probe_configured_disk_swap,
};
use super::profile::{build_recommended_zram, pick_profile, profile_reference};
use super::staging::{sysctl_needs_update, zram_for_staging, zram_needs_update};
use super::types::{
    OverflowDecision, RecommendProfile, RecommendationItem, RecommendedDefaults, SystemContext,
    OVERFLOW_FREE_SPACE_MARGIN_MB, OVERFLOW_SWAPFILE_MAX_MB, OVERFLOW_SWAPFILE_PATH,
};

/// Optional overrides for overflow probing (tests inject free space / disk-swap flags).
#[derive(Debug, Clone, Default)]
pub(crate) struct OverflowInputs {
    pub configured_disk_swap: bool,
    pub configured_paths: Vec<String>,
    pub available_bytes: Option<u64>,
}

pub(crate) fn recommend_from_context(
    detection: &DetectionReport,
    status: &StatusReport,
    current_sysctl: Option<SysctlValues>,
    current_zram: Option<ZramConfig>,
    overflow: Option<OverflowInputs>,
) -> RecommendedDefaults {
    let profile = pick_profile(detection, status);
    let mut items = Vec::new();
    let mut pending = PendingConfig::default();
    let staging_blocked = !detection.etc_writable || detection.immutable_os;
    let hibernate_blocks_zram = checks::hibernation_zram_conflict(&status.swaps);

    if !detection.etc_writable {
        items.push(note_item(
            "Read-only /etc: cannot stage configuration",
            "XZram cannot stage zram, sysctl, or swapfile changes on a read-only /etc. Use distro layering or a writable /etc overlay.",
            Some("doctor-mapping"),
        ));
    }

    if detection.immutable_os {
        items.push(note_item(
            "Immutable OS: cannot stage configuration",
            "This system looks immutable (NixOS, ostree, Silverblue, or similar). Configure swap via distro layering or the Doctor tab — Apply recommended defaults will not stage changes.",
            Some("doctor-mapping"),
        ));
    }

    if detection.zram_backend == ZramBackend::ZramTools {
        items.push(note_item(
            "Legacy zram-tools configuration detected",
            "Advisory only — Apply defaults does not migrate backends. Run 'xzram zram migrate' first if you are switching to systemd-zram-generator.",
            Some("doctor-mapping"),
        ));
    }

    if detection.zram_backend == ZramBackend::None && status.zram_devices.is_empty() {
        let pkg = crate::detect::zram_generator_package_name(detection.package_manager);
        items.push(note_item(
            "No zram backend detected",
            format!(
                "Install {pkg} and apply defaults to create a zram swap device. Apply defaults does not install packages."
            ),
            Some("doctor-mapping"),
        ));
    }

    let (configured_disk_swap, _) = match &overflow {
        Some(o) => (o.configured_disk_swap, ()),
        None => {
            let (configured, _) = probe_configured_disk_swap();
            (configured, ())
        }
    };

    if !staging_blocked {
        stage_zram(
            &mut items,
            &mut pending,
            profile,
            detection,
            status,
            current_zram.as_ref(),
            hibernate_blocks_zram,
        );
        stage_sysctl(&mut items, &mut pending, current_sysctl.as_ref());
        stage_overflow(&mut items, &mut pending, status, overflow.as_ref());
    }

    items.extend(advisory_items(
        detection,
        status,
        current_zram.as_ref(),
        pending.swapfile.is_some(),
    ));

    if !staging_blocked && items.iter().all(|i| !i.will_stage) {
        items.insert(
            0,
            note_item(
                "System already matches recommended defaults",
                "No configuration changes will be staged.",
                None,
            ),
        );
    }

    let active_disk = has_active_disk_swap(status);

    let context = SystemContext {
        mem_total_bytes: status.memory.mem_total_kb * 1024,
        mem_available_bytes: status.memory.mem_available_kb * 1024,
        has_active_zram: !status.zram_devices.is_empty(),
        has_disk_swap: active_disk || configured_disk_swap,
        distro: detection
            .distro
            .pretty_name
            .clone()
            .unwrap_or(detection.distro.id.clone()),
        root_filesystem: detection.root_filesystem.clone(),
        zram_backend: format!("{:?}", detection.zram_backend).to_lowercase(),
        profile: format!("{profile:?}").to_lowercase(),
        immutable_os: detection.immutable_os,
        etc_writable: detection.etc_writable,
    };

    RecommendedDefaults {
        pending,
        items,
        context,
    }
}

fn stage_zram(
    items: &mut Vec<RecommendationItem>,
    pending: &mut PendingConfig,
    profile: RecommendProfile,
    detection: &DetectionReport,
    status: &StatusReport,
    current_zram: Option<&ZramConfig>,
    hibernate_blocks_zram: bool,
) {
    let recommended_zram = build_recommended_zram(profile, detection, status);
    let ram_mb = status.memory.mem_total_kb / 1024;

    if hibernate_blocks_zram {
        items.push(note_item(
            "Hibernation conflict: zram staging skipped",
            "Hibernation resume device points to zram. Disk-backed swap is required as the resume device.",
            Some("known-conflicts"),
        ));
    } else if zram_needs_update(current_zram, &recommended_zram, ram_mb) {
        let staged_zram = zram_for_staging(current_zram, &recommended_zram, ram_mb);
        let algo = staged_zram
            .compression_algorithm
            .as_deref()
            .unwrap_or("zstd");
        let size = staged_zram
            .zram_size
            .as_deref()
            .unwrap_or("min(ram / 2, 4096)");
        let pri = staged_zram.swap_priority.unwrap_or(100);
        let resident = staged_zram
            .zram_resident_limit
            .as_deref()
            .map(|r| format!(", resident-limit {r}"))
            .unwrap_or_default();
        items.push(RecommendationItem {
            category: "zram".into(),
            summary: format!("Configure ZRAM ({profile:?}, {algo}, priority {pri})"),
            detail: format!(
                "Device {} with size formula '{size}'{resident} based on {} RAM",
                staged_zram.device,
                status::format_bytes(status.memory.mem_total_kb * 1024)
            ),
            will_stage: true,
            reference: Some(profile_reference(profile).unwrap().into()),
        });
        pending.zram = Some(staged_zram);
    } else if status.zram_devices.is_empty() {
        items.push(note_item(
            "ZRAM configuration already matches recommendations",
            "No zram generator changes needed.",
            profile_reference(profile),
        ));
    } else {
        items.push(note_item(
            "Active ZRAM already matches recommended settings",
            "Current generator config aligns with hardware-based defaults (vendor size is kept when already large enough).",
            profile_reference(profile),
        ));
    }

    if profile == RecommendProfile::Performance {
        items.push(note_item(
            "Performance profile resident-limit",
            "zram-resident-limit = ram / 2 caps RAM used for compressed pages when zram-size = ram. See docs/RECOMMENDATIONS.md#resident-limit.",
            Some("resident-limit"),
        ));
    }
}

fn stage_sysctl(
    items: &mut Vec<RecommendationItem>,
    pending: &mut PendingConfig,
    current_sysctl: Option<&SysctlValues>,
) {
    let recommended_sysctl = sysctl::zram_tuning_defaults();
    if sysctl_needs_update(current_sysctl, &recommended_sysctl) {
        items.push(RecommendationItem {
            category: "sysctl".into(),
            summary: "Apply zram sysctl tuning defaults".into(),
            detail: "vm.swappiness=180, vm.watermark_boost_factor=0, vm.watermark_scale_factor=125, vm.page-cluster=0".into(),
            will_stage: true,
            reference: Some("sysctl-tuning".into()),
        });
        pending.sysctl = Some(recommended_sysctl);
    } else {
        items.push(note_item(
            "Sysctl values already match zram tuning defaults",
            "No vm.* changes needed.",
            Some("sysctl-tuning"),
        ));
    }
}

fn stage_overflow(
    items: &mut Vec<RecommendationItem>,
    pending: &mut PendingConfig,
    status: &StatusReport,
    overflow: Option<&OverflowInputs>,
) {
    let (configured_disk_swap, configured_paths, available_bytes) = match overflow {
        Some(o) => (
            o.configured_disk_swap,
            o.configured_paths.clone(),
            o.available_bytes,
        ),
        None => {
            let (configured, paths) = probe_configured_disk_swap();
            (
                configured,
                paths,
                available_bytes_near(OVERFLOW_SWAPFILE_PATH),
            )
        }
    };

    match decide_overflow_swapfile(
        status,
        configured_disk_swap,
        &configured_paths,
        available_bytes,
    ) {
        OverflowDecision::Stage(swapfile) => {
            items.push(RecommendationItem {
                category: "swapfile".into(),
                summary: format!(
                    "Create overflow swap file ({} MiB, priority {})",
                    swapfile.size_mb, swapfile.priority
                ),
                detail: format!(
                    "Disk-backed safety net at {} (capped at {} MiB). Btrfs nodatacow is prepared automatically on apply.",
                    swapfile.path, OVERFLOW_SWAPFILE_MAX_MB
                ),
                will_stage: true,
                reference: Some("overflow-swapfile".into()),
            });
            pending.swapfile = Some(swapfile);
            items.push(note_item(
                "Dual-tier swap tradeoff",
                "Advisory only — Apply defaults stages overflow as a safety net. If swap regularly exceeds ~30% of RAM, zswap may fit better — see docs/RECOMMENDATIONS.md#dual-tier-tradeoff.",
                Some("dual-tier-tradeoff"),
            ));
        }
        OverflowDecision::SkipConfiguredDiskSwap { paths } => {
            items.push(note_item(
                "Configured disk swap already present",
                format!(
                    "Overflow swapfile not staged because fstab (or managed inventory) already lists non-zram swap: {}. Enable it with swapon or remove stale entries before adding {}.",
                    paths.join(", "),
                    OVERFLOW_SWAPFILE_PATH
                ),
                Some("overflow-swapfile"),
            ));
        }
        OverflowDecision::SkipInsufficientSpace {
            required_mb,
            available_mb,
        } => {
            items.push(note_item(
                "Insufficient free disk space for overflow swapfile",
                format!(
                    "Need about {required_mb} MiB free near {} (including {} MiB margin); about {available_mb} MiB available. Free space or choose a smaller swapfile manually.",
                    OVERFLOW_SWAPFILE_PATH, OVERFLOW_FREE_SPACE_MARGIN_MB
                ),
                Some("overflow-swapfile"),
            ));
        }
        OverflowDecision::SkipActiveDiskSwap | OverflowDecision::SkipZeroSize => {}
    }
}
