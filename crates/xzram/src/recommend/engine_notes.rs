use crate::apply::ZramConfig;
use crate::checks;
use crate::detect::DetectionReport;
use crate::status::StatusReport;

use super::types::{RecommendationItem, OVERFLOW_SWAPFILE_PATH};

pub(super) fn note_item(
    summary: &str,
    detail: impl Into<String>,
    reference: Option<&str>,
) -> RecommendationItem {
    RecommendationItem {
        category: "note".into(),
        summary: summary.into(),
        detail: detail.into(),
        will_stage: false,
        reference: reference.map(str::to_string),
    }
}

pub(super) fn advisory_items(
    detection: &DetectionReport,
    status: &StatusReport,
    current_zram: Option<&ZramConfig>,
    staging_overflow_swapfile: bool,
) -> Vec<RecommendationItem> {
    let mut items = Vec::new();

    if checks::zswap_enabled() == Some(true) {
        let detail = if checks::zram_zswap_conflict(&status.zram_devices) {
            "Advisory only — Apply defaults does not disable zswap. Both zram and zswap are active; disable zswap before using zram (see Doctor tab)."
        } else {
            "Advisory only — Apply defaults does not disable zswap. Disable manually: echo 0 | sudo tee /sys/module/zswap/parameters/enabled, or add zswap.enabled=0 to the kernel cmdline."
        };
        items.push(note_item(
            "Disable zswap when using zram",
            detail,
            Some("known-conflicts"),
        ));
    }

    items.push(note_item(
        "When zswap may fit better",
        "Advisory only — Apply defaults does not configure zswap. If sustained swap use exceeds ~30% of RAM or is unpredictable on fast NVMe, consider a zswap-based setup instead of zram-only tuning.",
        Some("zswap-alternative"),
    ));

    if detection.root_filesystem.as_deref() == Some("zfs") {
        items.push(note_item(
            "ZFS root: swapfiles have special requirements",
            "Advisory only — prefer a dedicated swap partition or zvol on ZFS systems. Apply defaults will not create a ZFS-safe swap layout automatically.",
            Some("doctor-mapping"),
        ));
    }

    if detection.root_filesystem.as_deref() == Some("btrfs") && staging_overflow_swapfile {
        items.push(note_item(
            "Btrfs: nodatacow prepared on apply",
            format!(
                "Apply runs 'xzram swapfile prepare' before creating {}.",
                OVERFLOW_SWAPFILE_PATH
            ),
            Some("doctor-mapping"),
        ));
    }

    if let Some(configured) = current_zram.and_then(|z| z.compression_algorithm.as_deref()) {
        if let Some(active) = status.zram_devices.first() {
            if checks::algorithm_mismatch(configured, &active.algorithm) {
                items.push(note_item(
                    "ZRAM algorithm mismatch",
                    format!(
                        "Advisory only — generator config specifies '{configured}' but active device uses '{}'. Check the ZRAM tab or Doctor; Apply defaults may restage generator settings but cannot force a live algorithm change alone.",
                        active.algorithm
                    ),
                    Some("known-conflicts"),
                ));
            }
        }
    }

    if checks::priority_inverted(&status.swaps) {
        items.push(note_item(
            "Swap priority inversion detected",
            "Apply defaults stages zram priority 100 and disk swapfile priority 10 when those changes are in scope, to restore correct tiering.",
            Some("priority-tiers"),
        ));
    }

    if status.zram_devices.len() > 1 {
        items.push(note_item(
            "Multiple zram devices detected",
            "Advisory only — XZram manages swap on zram0 only. Additional zram devices (e.g. /tmp ramdisk) are not changed.",
            Some("multi-device"),
        ));
    }

    items.push(note_item(
        "Writeback device not used",
        "Advisory only — Apply defaults does not configure writeback-device. XZram uses a low-priority overflow swapfile instead (requires no separate daemon). See docs/RECOMMENDATIONS.md#writeback-device.",
        Some("writeback-device"),
    ));

    items
}
