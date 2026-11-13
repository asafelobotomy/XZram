//! Overflow swapfile staging for the recommendation builder.

use crate::apply::{PendingConfig, SwapfileResizeConfig};
use crate::status::StatusReport;

use super::engine_build::OverflowInputs;
use super::engine_notes::note_item;
use super::overflow::{
    available_bytes_near, decide_overflow_swapfile_scaled, has_active_disk_swap,
    probe_configured_disk_swap,
};
use super::scales::RecommendSizeScale;
use super::types::{
    OverflowDecision, RecommendationItem, OVERFLOW_FREE_SPACE_MARGIN_MB,
    OVERFLOW_SWAPFILE_HIGH_MAX_MB, OVERFLOW_SWAPFILE_MAX_MB, OVERFLOW_SWAPFILE_PATH,
};

pub(super) fn stage_overflow(
    items: &mut Vec<RecommendationItem>,
    pending: &mut PendingConfig,
    status: &StatusReport,
    overflow: Option<&OverflowInputs>,
    swap_scale: RecommendSizeScale,
) {
    // Live probes only; tests inject `overflow` and must not depend on host disks.
    if overflow.is_none() {
        push_rotational_warnings(items, status);
    }
    let (configured_disk_swap, configured_paths, available_bytes, placement) = match overflow {
        Some(o) => (
            o.configured_disk_swap,
            o.configured_paths.clone(),
            o.available_bytes,
            o.placement.clone(),
        ),
        None => {
            let (configured, paths) = probe_configured_disk_swap();
            (
                configured,
                paths,
                available_bytes_near(OVERFLOW_SWAPFILE_PATH),
                crate::btrfs_subvol::swapfile_placement(OVERFLOW_SWAPFILE_PATH),
            )
        }
    };

    let cap = match swap_scale {
        RecommendSizeScale::High => OVERFLOW_SWAPFILE_HIGH_MAX_MB,
        _ => OVERFLOW_SWAPFILE_MAX_MB,
    };

    let decision = decide_overflow_swapfile_scaled(
        status,
        configured_disk_swap,
        &configured_paths,
        available_bytes,
        swap_scale,
    );
    // Never create (or grow in place) a swapfile that would block btrfs snapshots.
    if let Some(placement) = placement.filter(|_| {
        matches!(
            decision,
            OverflowDecision::Stage(_) | OverflowDecision::Grow { .. }
        )
    }) {
        let exists = matches!(decision, OverflowDecision::Grow { .. });
        items.push(note_item(
            "Overflow swap file needs its own btrfs subvolume",
            format!(
                "{} {}. {}",
                if exists {
                    "Not growing it in place."
                } else {
                    "Not staged."
                },
                placement.message(),
                placement.fix()
            ),
            Some("btrfs-swap-subvolume"),
        ));
        return;
    }

    match decision {
        OverflowDecision::Stage(swapfile) => {
            items.push(RecommendationItem {
                category: "swapfile".into(),
                summary: format!(
                    "Create overflow swap file ({} MiB, priority {})",
                    swapfile.size_mb, swapfile.priority
                ),
                detail: format!(
                    "Disk-backed safety net at {} (capped at {} MiB). Btrfs nodatacow is prepared automatically on apply.",
                    swapfile.path, cap
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
        OverflowDecision::Grow {
            path,
            from_mb,
            to_mb,
        } => {
            items.push(RecommendationItem {
                category: "swapfile".into(),
                summary: format!("Grow overflow swap file ({from_mb} MiB -> {to_mb} MiB)"),
                detail: format!(
                    "{path} is much smaller than the recommended safety net (capped at {cap} MiB); once zram fills, the system has nowhere to page out. Resized in place on apply."
                ),
                will_stage: true,
                reference: Some("overflow-swapfile".into()),
            });
            pending.swapfile_resize = Some(SwapfileResizeConfig {
                path,
                size_mb: to_mb,
            });
        }
        OverflowDecision::SkipActiveDiskSwap | OverflowDecision::SkipZeroSize => {}
    }
}

/// Disk swap should live on the fastest device; flag swap on (or headed for) an HDD.
fn push_rotational_warnings(items: &mut Vec<RecommendationItem>, status: &StatusReport) {
    use crate::placement::{is_rotational_path, is_rotational_swap};
    let mut slow: Vec<String> = status
        .swaps
        .iter()
        .filter(|s| !s.name.contains("zram"))
        .filter(|s| is_rotational_swap(&s.name) == Some(true))
        .map(|s| s.name.clone())
        .collect();
    if !has_active_disk_swap(status) && is_rotational_path(OVERFLOW_SWAPFILE_PATH) == Some(true) {
        slow.push(OVERFLOW_SWAPFILE_PATH.into());
    }
    if !slow.is_empty() {
        items.push(note_item(
            "Disk swap is on a rotational drive (HDD)",
            format!(
                "{} sits on spinning media; paging there stalls the desktop. Move disk swap to an SSD/NVMe filesystem.",
                slow.join(", ")
            ),
            Some("overflow-swapfile"),
        ));
    }
}
