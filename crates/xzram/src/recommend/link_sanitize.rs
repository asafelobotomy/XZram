//! Sanitize / fill helpers for linked optimize.

use crate::apply::{PendingConfig, SwapfileConfig, ZramConfig};
use crate::sysctl;
use crate::validation::{self, validate_zram_config};

use super::link_types::{LinkedAnchor, LinkedOptimizeContext};
use super::scales::{overflow_size_mb_for_scale, RecommendSizeScale};
use super::types::{OVERFLOW_SWAPFILE_PATH, OVERFLOW_SWAP_PRIORITY};

pub(super) fn ensure_zram(
    pending: &mut PendingConfig,
    defaults: &ZramConfig,
    adjustments: &mut Vec<String>,
) {
    if pending.disable_zram {
        return;
    }
    if pending.zram.is_none() {
        pending.zram = Some(defaults.clone());
        adjustments.push("Filled ZRAM from recommended defaults.".into());
    }
}

pub(super) fn sanitize_zram(
    anchor: LinkedAnchor,
    pending: &mut PendingConfig,
    defaults: &ZramConfig,
    adjustments: &mut Vec<String>,
) {
    let Some(ref mut z) = pending.zram else {
        return;
    };
    if z.device.is_empty() || validate_zram_config(z).is_err() {
        // Preserve valid anchor pieces when possible.
        let mut fixed = defaults.clone();
        match anchor {
            LinkedAnchor::ZramSize => {
                if let Some(ref size) = z.zram_size {
                    let probe = ZramConfig {
                        zram_size: Some(size.clone()),
                        ..defaults.clone()
                    };
                    if validate_zram_config(&probe).is_ok() {
                        fixed.zram_size = Some(size.clone());
                    } else {
                        adjustments.push(format!(
                            "Invalid zram-size replaced with {}.",
                            defaults.zram_size.as_deref().unwrap_or("recommended")
                        ));
                    }
                }
            }
            LinkedAnchor::ZramAlgo => {
                if let Some(ref algo) = z.compression_algorithm {
                    let probe = ZramConfig {
                        compression_algorithm: Some(algo.clone()),
                        ..defaults.clone()
                    };
                    if validate_zram_config(&probe).is_ok() {
                        fixed.compression_algorithm = Some(algo.clone());
                    } else {
                        adjustments.push(format!(
                            "Invalid algorithm replaced with {}.",
                            defaults.compression_algorithm.as_deref().unwrap_or("zstd")
                        ));
                    }
                }
            }
            LinkedAnchor::ZramPriority => {
                if let Some(pri) = z.swap_priority {
                    if (-1..=32767).contains(&pri) {
                        fixed.swap_priority = Some(pri);
                    } else {
                        fixed.swap_priority = Some(100);
                        adjustments.push("Invalid ZRAM priority replaced with 100.".into());
                    }
                }
            }
            _ => {
                adjustments.push("Invalid ZRAM config replaced with recommended defaults.".into());
            }
        }
        *z = fixed;
        return;
    }
    if z.swap_priority.is_some_and(|p| !(-1..=32767).contains(&p)) {
        z.swap_priority = Some(100);
        adjustments.push("Invalid ZRAM priority replaced with 100.".into());
    }
}

pub(super) fn apply_zram_dependents(
    anchor: LinkedAnchor,
    pending: &mut PendingConfig,
    defaults: &ZramConfig,
    ctx: &LinkedOptimizeContext,
    adjustments: &mut Vec<String>,
) {
    match anchor {
        LinkedAnchor::ZramSize => {
            if let Some(ref mut z) = pending.zram {
                if z.compression_algorithm.is_none() {
                    z.compression_algorithm = defaults.compression_algorithm.clone();
                }
                if z.swap_priority.is_none() {
                    z.swap_priority = Some(100);
                }
                z.zram_resident_limit = defaults.zram_resident_limit.clone();
            }
            fill_sysctl_defaults(pending, adjustments);
            maybe_stage_overflow(pending, ctx, adjustments);
        }
        LinkedAnchor::ZramAlgo => {
            if let Some(ref mut z) = pending.zram {
                if (ctx.has_disk_swap || pending.swapfile.is_some())
                    && z.swap_priority.unwrap_or(0) < 100
                {
                    z.swap_priority = Some(100);
                    adjustments.push("ZRAM priority set to 100 above disk swap.".into());
                }
            }
            fill_sysctl_defaults(pending, adjustments);
            if let Some(ref mut sc) = pending.sysctl {
                if sc.page_cluster != Some(0) {
                    sc.page_cluster = Some(0);
                    adjustments.push("page-cluster set to 0 for zram.".into());
                }
            }
        }
        LinkedAnchor::ZramPriority => {
            align_swap_priority_with_zram(pending, ctx, adjustments);
        }
        _ => {}
    }
}

fn clamp_u32(v: u32, max: u32) -> u32 {
    v.min(max)
}

pub(super) fn sanitize_sysctl(pending: &mut PendingConfig, adjustments: &mut Vec<String>) {
    let Some(ref mut sc) = pending.sysctl else {
        return;
    };
    let mut changed = false;
    if let Some(v) = sc.swappiness {
        let c = clamp_u32(v, 200);
        if c != v {
            sc.swappiness = Some(c);
            changed = true;
        }
    }
    if let Some(v) = sc.watermark_boost_factor {
        let c = clamp_u32(v, 10000);
        if c != v {
            sc.watermark_boost_factor = Some(c);
            changed = true;
        }
    }
    if let Some(v) = sc.watermark_scale_factor {
        let c = clamp_u32(v, 10000);
        if c != v {
            sc.watermark_scale_factor = Some(c);
            changed = true;
        }
    }
    if let Some(v) = sc.page_cluster {
        let c = clamp_u32(v, 8);
        if c != v {
            sc.page_cluster = Some(c);
            changed = true;
        }
    }
    if changed {
        adjustments.push("Sysctl values clamped to validated ranges.".into());
    }
}

pub(super) fn fill_sysctl_defaults(pending: &mut PendingConfig, adjustments: &mut Vec<String>) {
    let defaults = sysctl::zram_tuning_defaults();
    match pending.sysctl.as_mut() {
        None => {
            pending.sysctl = Some(defaults);
            adjustments.push("Applied zram sysctl tuning defaults.".into());
        }
        Some(sc) => {
            let mut filled = false;
            if sc.swappiness.is_none() {
                sc.swappiness = defaults.swappiness;
                filled = true;
            }
            if sc.watermark_boost_factor.is_none() {
                sc.watermark_boost_factor = defaults.watermark_boost_factor;
                filled = true;
            }
            if sc.watermark_scale_factor.is_none() {
                sc.watermark_scale_factor = defaults.watermark_scale_factor;
                filled = true;
            }
            if sc.page_cluster.is_none() {
                sc.page_cluster = defaults.page_cluster;
                filled = true;
            }
            if filled {
                adjustments.push("Filled missing sysctl knobs with zram tuning defaults.".into());
            }
        }
    }
}

pub(super) fn maybe_stage_overflow(
    pending: &mut PendingConfig,
    ctx: &LinkedOptimizeContext,
    adjustments: &mut Vec<String>,
) {
    if ctx.has_disk_swap || pending.swapfile.is_some() || pending.remove_swapfile.is_some() {
        return;
    }
    let size_mb = overflow_size_mb_for_scale(ctx.mem_total_kb, RecommendSizeScale::Default);
    if size_mb == 0 {
        return;
    }
    pending.swapfile = Some(SwapfileConfig {
        path: OVERFLOW_SWAPFILE_PATH.into(),
        size_mb,
        priority: OVERFLOW_SWAP_PRIORITY,
    });
    adjustments.push(format!(
        "Staged overflow swapfile {OVERFLOW_SWAPFILE_PATH} ({size_mb} MiB, pri {OVERFLOW_SWAP_PRIORITY})."
    ));
}

pub(super) fn sanitize_swapfile_create(
    pending: &mut PendingConfig,
    ctx: &LinkedOptimizeContext,
    adjustments: &mut Vec<String>,
) {
    let Some(ref mut sf) = pending.swapfile else {
        return;
    };
    if validation::validate_swapfile_path(&sf.path).is_err() {
        sf.path = OVERFLOW_SWAPFILE_PATH.into();
        adjustments.push(format!(
            "Invalid swapfile path replaced with {OVERFLOW_SWAPFILE_PATH}."
        ));
    }
    let max = overflow_size_mb_for_scale(ctx.mem_total_kb, RecommendSizeScale::High);
    if sf.size_mb == 0 || sf.size_mb > max {
        let capped = overflow_size_mb_for_scale(ctx.mem_total_kb, RecommendSizeScale::Default);
        sf.size_mb = capped.max(1);
        adjustments.push(format!("Swapfile size adjusted to {} MiB.", sf.size_mb));
    }
    if !(-1..=32767).contains(&sf.priority) {
        sf.priority = OVERFLOW_SWAP_PRIORITY;
        adjustments.push(format!(
            "Swapfile priority adjusted to {OVERFLOW_SWAP_PRIORITY}."
        ));
    }
}

pub(super) fn sanitize_swapfile_resize(
    pending: &mut PendingConfig,
    ctx: &LinkedOptimizeContext,
    adjustments: &mut Vec<String>,
) {
    let Some(ref mut rz) = pending.swapfile_resize else {
        return;
    };
    if validation::validate_swapfile_path(&rz.path).is_err() {
        pending.swapfile_resize = None;
        adjustments.push("Invalid resize path cleared.".into());
        return;
    }
    let max = overflow_size_mb_for_scale(ctx.mem_total_kb, RecommendSizeScale::High);
    if rz.size_mb == 0 || rz.size_mb > max {
        rz.size_mb =
            overflow_size_mb_for_scale(ctx.mem_total_kb, RecommendSizeScale::Default).max(1);
        adjustments.push(format!("Resize size adjusted to {} MiB.", rz.size_mb));
    }
}

pub(super) fn sanitize_remove_path(pending: &mut PendingConfig, adjustments: &mut Vec<String>) {
    if let Some(ref path) = pending.remove_swapfile {
        if validation::validate_swapfile_path(path).is_err() {
            pending.remove_swapfile = None;
            adjustments.push("Invalid remove path cleared.".into());
        }
    }
}

pub(super) fn align_swap_priority_with_zram(
    pending: &mut PendingConfig,
    ctx: &LinkedOptimizeContext,
    adjustments: &mut Vec<String>,
) {
    let zram_pri = pending
        .zram
        .as_ref()
        .and_then(|z| z.swap_priority)
        .unwrap_or(if ctx.has_active_zram { 100 } else { 0 });
    if let Some(ref mut sf) = pending.swapfile {
        if sf.priority >= zram_pri && zram_pri > 0 {
            sf.priority = OVERFLOW_SWAP_PRIORITY.min(zram_pri.saturating_sub(1).max(-1));
            adjustments.push(format!(
                "Overflow swap priority set to {} below ZRAM.",
                sf.priority
            ));
        }
    }
    if let Some(ref mut z) = pending.zram {
        if let Some(ref sf) = pending.swapfile {
            if z.swap_priority.unwrap_or(0) <= sf.priority {
                z.swap_priority = Some(100);
                adjustments.push("ZRAM priority raised to 100 above overflow swap.".into());
            }
        } else if ctx.has_disk_swap && z.swap_priority.unwrap_or(0) < 100 {
            z.swap_priority = Some(100);
            adjustments.push("ZRAM priority raised to 100 above disk swap.".into());
        }
    }
}
