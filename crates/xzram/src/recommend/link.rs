//! Live linked optimization: rewrite dependents when one staged field changes.

use crate::apply::ZramConfig;

use super::link_sanitize::{
    align_swap_priority_with_zram, apply_zram_dependents, ensure_zram, fill_sysctl_defaults,
    sanitize_remove_path, sanitize_swapfile_create, sanitize_swapfile_resize, sanitize_sysctl,
    sanitize_zram,
};
use super::link_types::{LinkedAnchor, LinkedOptimizeContext};
use super::profile::pick_profile;
use super::scales::{zram_size_formula, RecommendSizeScale};
use super::types::RecommendProfile;

pub use super::link_types::*;

/// Build context from live detect/status (CLI / GUI path).
pub fn linked_optimize_context() -> crate::error::Result<LinkedOptimizeContext> {
    let detection = crate::detect::detect()?;
    let status = crate::status::status()?;
    Ok(LinkedOptimizeContext {
        profile: pick_profile(&detection, &status),
        mem_total_kb: status.memory.mem_total_kb,
        has_active_zram: !status.zram_devices.is_empty(),
        has_disk_swap: status.swaps.iter().any(|s| !s.name.contains("zram")),
        hibernate_blocks_zram: crate::checks::hibernation_zram_conflict(&status.swaps),
        etc_writable: detection.etc_writable,
        immutable_os: detection.immutable_os,
    })
}

pub fn optimize_linked(
    anchor: LinkedAnchor,
    seed: crate::apply::PendingConfig,
    ctx: &LinkedOptimizeContext,
) -> LinkedOptimizeResult {
    let mut pending = seed;
    let mut adjustments = Vec::new();
    let mem_gb = ctx.mem_total_kb as f64 / (1024.0 * 1024.0);
    let defaults = recommended_zram_shell(ctx.profile, mem_gb);

    if ctx.immutable_os || !ctx.etc_writable {
        adjustments.push(
            "Staging is blocked on this host (immutable OS or read-only /etc); \
             forms were still normalized locally."
                .into(),
        );
    }

    if ctx.hibernate_blocks_zram && pending.zram.is_some() {
        pending.zram = None;
        adjustments.push(
            "Hibernation resume uses zram; zram staging cleared (use disk swap for resume).".into(),
        );
    }

    match anchor {
        LinkedAnchor::ZramSize | LinkedAnchor::ZramAlgo | LinkedAnchor::ZramPriority => {
            ensure_zram(&mut pending, &defaults, &mut adjustments);
            sanitize_zram(anchor, &mut pending, &defaults, &mut adjustments);
            apply_zram_dependents(anchor, &mut pending, &defaults, ctx, &mut adjustments);
        }
        LinkedAnchor::Sysctl => {
            sanitize_sysctl(&mut pending, &mut adjustments);
            if let Some(ref sc) = pending.sysctl {
                if sc.swappiness.is_some_and(|s| s >= 100) {
                    ensure_zram(&mut pending, &defaults, &mut adjustments);
                    if let Some(ref mut z) = pending.zram {
                        if z.swap_priority.unwrap_or(0) < 100 {
                            z.swap_priority = Some(100);
                            adjustments.push(
                                "ZRAM priority set to 100 for zram-oriented swappiness.".into(),
                            );
                        }
                    }
                }
            }
            fill_sysctl_defaults(&mut pending, &mut adjustments);
        }
        LinkedAnchor::SwapfileCreate => {
            sanitize_swapfile_create(&mut pending, ctx, &mut adjustments);
            align_swap_priority_with_zram(&mut pending, ctx, &mut adjustments);
        }
        LinkedAnchor::SwapfileResize => {
            sanitize_swapfile_resize(&mut pending, ctx, &mut adjustments);
        }
        LinkedAnchor::SwapfileRemove => {
            if pending.remove_swapfile.is_some() {
                pending.swapfile = None;
                pending.swapfile_resize = None;
                adjustments.push("Cleared create/resize siblings for swapfile remove.".into());
            }
            sanitize_remove_path(&mut pending, &mut adjustments);
        }
    }

    if pending.zram.is_some() {
        sanitize_zram(
            LinkedAnchor::ZramSize,
            &mut pending,
            &defaults,
            &mut adjustments,
        );
    }
    if pending.sysctl.is_some() {
        sanitize_sysctl(&mut pending, &mut adjustments);
    }
    if pending.swapfile.is_some() {
        sanitize_swapfile_create(&mut pending, ctx, &mut adjustments);
    }
    if pending.swapfile_resize.is_some() {
        sanitize_swapfile_resize(&mut pending, ctx, &mut adjustments);
    }

    LinkedOptimizeResult {
        pending,
        adjustments,
    }
}

fn recommended_zram_shell(profile: RecommendProfile, mem_gb: f64) -> ZramConfig {
    let algo = match profile {
        RecommendProfile::Constrained => "lz4",
        _ => "zstd",
    };
    ZramConfig {
        device: "zram0".into(),
        zram_size: Some(zram_size_formula(
            profile,
            mem_gb,
            RecommendSizeScale::Default,
        )),
        zram_resident_limit: match profile {
            RecommendProfile::Performance => Some("ram / 2".into()),
            _ => None,
        },
        compression_algorithm: Some(algo.into()),
        swap_priority: Some(100),
        fs_type: None,
        mount_point: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{PendingConfig, SwapfileConfig, ZramConfig};
    use crate::sysctl::SysctlValues;

    fn ctx_8g() -> LinkedOptimizeContext {
        LinkedOptimizeContext {
            profile: RecommendProfile::Conservative,
            mem_total_kb: 8 * 1024 * 1024,
            has_active_zram: false,
            has_disk_swap: false,
            hibernate_blocks_zram: false,
            etc_writable: true,
            immutable_os: false,
        }
    }

    #[test]
    fn clamps_fatal_sysctl() {
        let seed = PendingConfig {
            sysctl: Some(SysctlValues {
                swappiness: Some(999),
                watermark_boost_factor: Some(0),
                watermark_scale_factor: Some(125),
                page_cluster: Some(99),
            }),
            ..Default::default()
        };
        let out = optimize_linked(LinkedAnchor::Sysctl, seed, &ctx_8g());
        let sc = out.pending.sysctl.expect("sysctl");
        assert_eq!(sc.swappiness, Some(200));
        assert_eq!(sc.page_cluster, Some(8));
        assert!(!out.adjustments.is_empty());
    }

    #[test]
    fn invalid_zram_size_replaced() {
        let seed = PendingConfig {
            zram: Some(ZramConfig {
                device: "zram0".into(),
                zram_size: Some("ram\n]".into()),
                zram_resident_limit: None,
                compression_algorithm: Some("zstd".into()),
                swap_priority: Some(100),
                fs_type: None,
                mount_point: None,
            }),
            ..Default::default()
        };
        let out = optimize_linked(LinkedAnchor::ZramSize, seed, &ctx_8g());
        let z = out.pending.zram.expect("zram");
        assert_eq!(z.zram_size.as_deref(), Some("min(ram / 2, 4096)"));
        assert!(out.pending.sysctl.is_some());
    }

    #[test]
    fn priority_inversion_fixed_on_overflow() {
        let seed = PendingConfig {
            zram: Some(ZramConfig {
                device: "zram0".into(),
                zram_size: Some("ram / 2".into()),
                zram_resident_limit: None,
                compression_algorithm: Some("zstd".into()),
                swap_priority: Some(5),
                fs_type: None,
                mount_point: None,
            }),
            swapfile: Some(SwapfileConfig {
                path: "/swap/swapfile".into(),
                size_mb: 2048,
                priority: 50,
            }),
            ..Default::default()
        };
        let out = optimize_linked(LinkedAnchor::ZramPriority, seed, &ctx_8g());
        let z = out.pending.zram.expect("zram");
        let sf = out.pending.swapfile.expect("swap");
        assert!(z.swap_priority.unwrap() > sf.priority);
    }
}
