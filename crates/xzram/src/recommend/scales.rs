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

/// Profile “Recommended” formula (unchanged from prior defaults).
pub fn zram_size_formula_default(profile: RecommendProfile, mem_gb: f64) -> String {
    match profile {
        RecommendProfile::Performance => "ram".into(),
        RecommendProfile::Constrained => "min(ram, 4096)".into(),
        RecommendProfile::Conservative => {
            if mem_gb >= 32.0 {
                "min(ram / 2, 8192)".into()
            } else {
                "min(ram / 2, 4096)".into()
            }
        }
    }
}

pub fn zram_size_formula(
    profile: RecommendProfile,
    mem_gb: f64,
    scale: RecommendSizeScale,
) -> String {
    match (profile, scale) {
        (RecommendProfile::Constrained, RecommendSizeScale::Low) => "min(ram / 2, 2048)".into(),
        (RecommendProfile::Constrained, RecommendSizeScale::Default) => {
            zram_size_formula_default(profile, mem_gb)
        }
        (RecommendProfile::Constrained, RecommendSizeScale::High) => "min(ram, 8192)".into(),

        (RecommendProfile::Conservative, RecommendSizeScale::Low) if mem_gb >= 32.0 => {
            "min(ram / 4, 4096)".into()
        }
        (RecommendProfile::Conservative, RecommendSizeScale::Low) => "min(ram / 4, 2048)".into(),
        (RecommendProfile::Conservative, RecommendSizeScale::Default) => {
            zram_size_formula_default(profile, mem_gb)
        }
        (RecommendProfile::Conservative, RecommendSizeScale::High) if mem_gb >= 32.0 => {
            "min(ram, 16384)".into()
        }
        (RecommendProfile::Conservative, RecommendSizeScale::High) => "min(ram, 8192)".into(),

        (RecommendProfile::Performance, RecommendSizeScale::Low) => "min(ram / 2, 4096)".into(),
        (RecommendProfile::Performance, RecommendSizeScale::Default | RecommendSizeScale::High) => {
            "ram".into()
        }
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

    #[test]
    fn conservative_tiers() {
        assert_eq!(
            zram_size_formula(RecommendProfile::Conservative, 8.0, RecommendSizeScale::Low),
            "min(ram / 4, 2048)"
        );
        assert_eq!(
            zram_size_formula(
                RecommendProfile::Conservative,
                8.0,
                RecommendSizeScale::Default
            ),
            "min(ram / 2, 4096)"
        );
        assert_eq!(
            zram_size_formula(
                RecommendProfile::Conservative,
                8.0,
                RecommendSizeScale::High
            ),
            "min(ram, 8192)"
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
