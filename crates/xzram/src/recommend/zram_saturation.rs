//! Feed the doctor's zram saturation finding into the recommended zram size (issue #28).

use crate::apply::ZramConfig;
use crate::saturation::{self, propose_size};
use crate::status::StatusReport;

use super::scales::{zram_size_formula, RecommendSizeScale};
use super::size_expr::eval_zram_size_mb;
use super::types::RecommendProfile;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum SaturationOutcome {
    /// Not saturated (or the user picked the Low tier).
    None,
    /// The recommended size grows to absorb the overflow; the reason for the item detail.
    Grow(String),
    /// Saturated, but nothing larger is recommended; advisory detail.
    AtLimit(String),
}

/// Smallest growth worth a device restart: 10% over the current size.
fn worth_growing(proposed_mb: u64, current_mb: u64) -> bool {
    proposed_mb > current_mb.saturating_add(current_mb / 10)
}

/// When zram is saturated and spilling to disk, raise `recommended.zram_size` to the
/// ratio-based size (or the next tier). Never proposes less than the current device.
pub(super) fn grow_for_saturation(
    recommended: &mut ZramConfig,
    status: &StatusReport,
    current: Option<&ZramConfig>,
    profile: RecommendProfile,
    scale: RecommendSizeScale,
) -> SaturationOutcome {
    if scale == RecommendSizeScale::Low {
        return SaturationOutcome::None;
    }
    let Some(sat) = saturation::detect(status) else {
        return SaturationOutcome::None;
    };
    let ram_mb = status.memory.mem_total_kb / 1024;
    let mem_gb = status.memory.mem_total_kb as f64 / (1024.0 * 1024.0);
    let next_tier = zram_size_formula(profile, mem_gb, RecommendSizeScale::High);

    let live_mb = sat.size_bytes / (1024 * 1024);
    let configured_mb = current
        .and_then(|c| c.zram_size.as_deref())
        .and_then(|f| eval_zram_size_mb(f, ram_mb))
        .unwrap_or(0);
    let current_mb = live_mb.max(configured_mb);
    let recommended_mb = recommended
        .zram_size
        .as_deref()
        .and_then(|f| eval_zram_size_mb(f, ram_mb))
        .unwrap_or(0);

    if let Some(p) = propose_size(&sat, ram_mb, &next_tier, eval_zram_size_mb) {
        if p.approx_mib > recommended_mb && worth_growing(p.approx_mib, current_mb) {
            recommended.zram_size = Some(p.formula);
            return SaturationOutcome::Grow(p.reason);
        }
    }
    if worth_growing(recommended_mb, current_mb) {
        let formula = recommended.zram_size.clone().unwrap_or_default();
        return SaturationOutcome::Grow(format!(
            "{}; the recommended '{formula}' holds it in RAM instead",
            sat.describe()
        ));
    }
    SaturationOutcome::AtLimit(format!(
        "{}. A larger device would cost more than half of RAM at the measured compression \
         ratio. If this is sustained, consider a zswap-based setup or more RAM, and keep the \
         disk overflow swap; 'xzram swap drain-overflow' moves overflow back once load drops.",
        sat.describe()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saturation::tests::status_28;

    const GIB: u64 = 1 << 30;

    fn zram(size: &str) -> ZramConfig {
        ZramConfig {
            device: "zram0".into(),
            zram_size: Some(size.into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        }
    }

    #[test]
    fn saturated_device_grows_to_ratio_size() {
        let status = status_28(4 * GIB, 700 << 20);
        let current = zram("min(ram / 2, 4096)");
        let mut rec = zram("min(ram, 16384)");
        let out = grow_for_saturation(
            &mut rec,
            &status,
            Some(&current),
            RecommendProfile::Conservative,
            RecommendSizeScale::Default,
        );
        assert!(
            matches!(out, SaturationOutcome::Grow(ref r) if r.contains("3.2x")),
            "{out:?}"
        );
        assert_eq!(rec.zram_size.as_deref(), Some("min(ram * 1.5, 65536)"));
    }

    #[test]
    fn no_change_without_saturation_or_on_low_tier() {
        let mut rec = zram("min(ram, 16384)");
        let calm = status_28(2 * GIB, 0);
        let out = grow_for_saturation(
            &mut rec,
            &calm,
            None,
            RecommendProfile::Conservative,
            RecommendSizeScale::Default,
        );
        assert_eq!(out, SaturationOutcome::None);
        let out = grow_for_saturation(
            &mut rec,
            &status_28(4 * GIB, 1 << 30),
            None,
            RecommendProfile::Conservative,
            RecommendSizeScale::Low,
        );
        assert_eq!(out, SaturationOutcome::None);
        assert_eq!(rec.zram_size.as_deref(), Some("min(ram, 16384)"));
    }

    #[test]
    fn never_shrinks_a_larger_device() {
        // Already at 2x RAM: the ratio size (1.5x) is smaller, so nothing is proposed.
        let mut status = status_28(4 * GIB, 700 << 20);
        status.swaps[0].size_bytes = 31 * GIB;
        status.swaps[0].used_bytes = 30 * GIB;
        let current = zram("ram * 2");
        let mut rec = zram("min(ram, 16384)");
        let out = grow_for_saturation(
            &mut rec,
            &status,
            Some(&current),
            RecommendProfile::Conservative,
            RecommendSizeScale::Default,
        );
        assert!(matches!(out, SaturationOutcome::AtLimit(_)), "{out:?}");
        assert_eq!(rec.zram_size.as_deref(), Some("min(ram, 16384)"));
    }
}
