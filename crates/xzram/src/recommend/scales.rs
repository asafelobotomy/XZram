//! Low / default / high size tiers for recommend UI and CLI.

use serde::{Deserialize, Serialize};

use super::staging::eval_zram_size_mb;
use super::types::{RecommendProfile, OVERFLOW_SWAPFILE_HIGH_MAX_MB, OVERFLOW_SWAPFILE_MAX_MB};

/// User-selectable size tier for zram or overflow swapfile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendSizeScale {
    Low,
    #[default]
    Default,
    High,
}

impl RecommendSizeScale {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Default => "default",
            Self::High => "high",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "default" => Some(Self::Default),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecommendScales {
    pub zram: RecommendSizeScale,
    pub swapfile: RecommendSizeScale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeScaleOption {
    pub formula: Option<String>,
    pub size_mib: Option<u64>,
    pub approx_mib: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramSizeScaleOptions {
    pub low: SizeScaleOption,
    pub default: SizeScaleOption,
    pub high: SizeScaleOption,
    pub selected: RecommendSizeScale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapfileSizeScaleOptions {
    /// False when overflow will not be staged (disk swap present, etc.).
    pub available: bool,
    pub low: SizeScaleOption,
    pub default: SizeScaleOption,
    pub high: SizeScaleOption,
    pub selected: RecommendSizeScale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendSizeScales {
    pub zram: ZramSizeScaleOptions,
    pub swapfile: SwapfileSizeScaleOptions,
}

/// MemTotal (GiB) from which the mid tier applies. A "12 GiB" machine reports ~11.5 GiB
/// MemTotal once firmware and the kernel have taken their share, so the cut is at 11.
pub const MID_RAM_GB: f64 = 11.0;
/// MemTotal (GiB) from which the large tier applies.
pub const LARGE_RAM_GB: f64 = 32.0;

/// RAM tier for the conservative profile's size formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RamTier {
    Small,
    Mid,
    Large,
}

fn ram_tier(mem_gb: f64) -> RamTier {
    if mem_gb >= LARGE_RAM_GB {
        RamTier::Large
    } else if mem_gb >= MID_RAM_GB {
        RamTier::Mid
    } else {
        RamTier::Small
    }
}

/// Profile “Recommended” formula.
///
/// Conservative, 12–31 GiB: `min(ram, 16384)` (`min(ram / 2, 4096)` up to 0.5.0). With
/// zstd's typical 2.5–3.5x ratio a full device of that size costs about a third of RAM, and
/// the 4 GiB cap filled under ordinary multi-build developer workloads (issues #13, #28),
/// sending the overflow to disk while most RAM was still free.
pub fn zram_size_formula_default(profile: RecommendProfile, mem_gb: f64) -> String {
    match profile {
        RecommendProfile::Performance => "ram".into(),
        RecommendProfile::Constrained => "min(ram, 4096)".into(),
        RecommendProfile::Conservative => match ram_tier(mem_gb) {
            RamTier::Small => "min(ram / 2, 4096)".into(),
            RamTier::Mid => "min(ram, 16384)".into(),
            RamTier::Large => "min(ram / 2, 16384)".into(),
        },
    }
}

pub fn zram_size_formula(
    profile: RecommendProfile,
    mem_gb: f64,
    scale: RecommendSizeScale,
) -> String {
    use RecommendSizeScale::{Default, High, Low};
    match (profile, scale) {
        (RecommendProfile::Constrained, Low) => "min(ram / 2, 2048)".into(),
        (RecommendProfile::Constrained, High) => "min(ram, 8192)".into(),
        (RecommendProfile::Conservative, Low) => match ram_tier(mem_gb) {
            RamTier::Small => "min(ram / 4, 2048)".into(),
            RamTier::Mid => "min(ram / 2, 4096)".into(),
            RamTier::Large => "min(ram / 4, 4096)".into(),
        },
        (RecommendProfile::Conservative, High) => match ram_tier(mem_gb) {
            RamTier::Small => "min(ram, 8192)".into(),
            RamTier::Mid => "min(ram * 1.5, 24576)".into(),
            RamTier::Large => "min(ram, 32768)".into(),
        },
        (RecommendProfile::Performance, Low) => "min(ram / 2, 4096)".into(),
        (RecommendProfile::Performance, High) => "ram".into(),
        (_, Default) => zram_size_formula_default(profile, mem_gb),
    }
}

pub fn overflow_size_mb_for_scale(mem_total_kb: u64, scale: RecommendSizeScale) -> u64 {
    let ram_mb = mem_total_kb / 1024;
    match scale {
        RecommendSizeScale::Low => (ram_mb / 2).min(4096),
        RecommendSizeScale::Default => ram_mb.min(OVERFLOW_SWAPFILE_MAX_MB),
        RecommendSizeScale::High => ram_mb.min(OVERFLOW_SWAPFILE_HIGH_MAX_MB),
    }
}

pub fn build_size_scales(
    profile: RecommendProfile,
    mem_total_kb: u64,
    scales: RecommendScales,
    swapfile_available: bool,
) -> RecommendSizeScales {
    let mem_gb = mem_total_kb as f64 / (1024.0 * 1024.0);
    let ram_mb = mem_total_kb / 1024;

    let zram_opt = |scale: RecommendSizeScale| {
        let formula = zram_size_formula(profile, mem_gb, scale);
        let approx = eval_zram_size_mb(&formula, ram_mb);
        SizeScaleOption {
            formula: Some(formula),
            size_mib: None,
            approx_mib: approx,
        }
    };

    let swap_opt = |scale: RecommendSizeScale| {
        let size = overflow_size_mb_for_scale(mem_total_kb, scale);
        SizeScaleOption {
            formula: None,
            size_mib: Some(size),
            approx_mib: Some(size),
        }
    };

    RecommendSizeScales {
        zram: ZramSizeScaleOptions {
            low: zram_opt(RecommendSizeScale::Low),
            default: zram_opt(RecommendSizeScale::Default),
            high: zram_opt(RecommendSizeScale::High),
            selected: scales.zram,
        },
        swapfile: SwapfileSizeScaleOptions {
            available: swapfile_available,
            low: swap_opt(RecommendSizeScale::Low),
            default: swap_opt(RecommendSizeScale::Default),
            high: swap_opt(RecommendSizeScale::High),
            selected: scales.swapfile,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conservative(mem_gb: f64) -> [String; 3] {
        [
            RecommendSizeScale::Low,
            RecommendSizeScale::Default,
            RecommendSizeScale::High,
        ]
        .map(|s| zram_size_formula(RecommendProfile::Conservative, mem_gb, s))
    }

    #[test]
    fn conservative_tiers_per_ram_size() {
        assert_eq!(
            conservative(8.0),
            ["min(ram / 4, 2048)", "min(ram / 2, 4096)", "min(ram, 8192)"]
        );
        // 12 GiB machines report ~11.5 GiB MemTotal; 15.5 GiB is the #28 machine.
        for mem_gb in [11.5, 15.5, 31.2] {
            assert_eq!(
                conservative(mem_gb),
                [
                    "min(ram / 2, 4096)",
                    "min(ram, 16384)",
                    "min(ram * 1.5, 24576)"
                ],
                "{mem_gb} GiB"
            );
        }
        assert_eq!(
            conservative(64.0),
            [
                "min(ram / 4, 4096)",
                "min(ram / 2, 16384)",
                "min(ram, 32768)"
            ]
        );
    }

    #[test]
    fn conservative_sizes_grow_with_ram_and_scale() {
        let mib = |gb: f64, scale| {
            let ram_mb = (gb * 1024.0) as u64;
            let f = zram_size_formula(RecommendProfile::Conservative, gb, scale);
            eval_zram_size_mb(&f, ram_mb).unwrap()
        };
        let mut last_default = 0;
        for gb in [4.0, 8.0, 11.5, 15.5, 24.0, 31.2, 32.0, 64.0, 128.0] {
            let (low, def, high) = (
                mib(gb, RecommendSizeScale::Low),
                mib(gb, RecommendSizeScale::Default),
                mib(gb, RecommendSizeScale::High),
            );
            assert!(low <= def && def <= high, "{gb} GiB: {low} {def} {high}");
            assert!(def >= last_default, "default shrank at {gb} GiB");
            last_default = def;
        }
        assert_eq!(mib(15.5, RecommendSizeScale::Default), 15_872);
        assert_eq!(mib(64.0, RecommendSizeScale::Default), 16_384);
    }

    #[test]
    fn other_profiles_unchanged() {
        let f = |p, gb, s| zram_size_formula(p, gb, s);
        assert_eq!(
            f(
                RecommendProfile::Constrained,
                2.0,
                RecommendSizeScale::Default
            ),
            "min(ram, 4096)"
        );
        assert_eq!(
            f(
                RecommendProfile::Performance,
                16.0,
                RecommendSizeScale::Default
            ),
            "ram"
        );
        assert_eq!(
            f(RecommendProfile::Performance, 16.0, RecommendSizeScale::Low),
            "min(ram / 2, 4096)"
        );
    }

    #[test]
    fn overflow_high_caps_at_16g() {
        assert_eq!(
            overflow_size_mb_for_scale(32 * 1024 * 1024, RecommendSizeScale::High),
            16384
        );
        assert_eq!(
            overflow_size_mb_for_scale(8 * 1024 * 1024, RecommendSizeScale::Default),
            8192
        );
        assert_eq!(
            overflow_size_mb_for_scale(8 * 1024 * 1024, RecommendSizeScale::Low),
            4096
        );
    }
}
