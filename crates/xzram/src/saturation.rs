//! zram saturation: a zram swap device that is (nearly) full while lower-priority disk swap
//! already holds overflow, and the ratio-based size that would have absorbed it.
//!
//! Shared by `doctor` (finding + severity) and `defaults recommend` (larger size proposal).
//! Everything here is pure over a [`StatusReport`], so it is testable without a live host.

use serde::{Deserialize, Serialize};

use crate::status::{format_bytes, StatusReport};

/// zram use (of disksize) at which the device counts as full.
pub const NEAR_FULL_RATIO: f64 = 0.9;
/// Fraction of RAM a completely full device may cost at the measured ratio.
pub const TARGET_RAM_SHARE: f64 = 0.5;
/// Measured ratios are clamped to this range before sizing (very high ratios come from
/// same-filled pages and do not last).
const RATIO_MIN: f64 = 1.5;
const RATIO_MAX: f64 = 4.0;
/// Upper bound for a ratio-based `zram-size`, in MiB.
pub const RATIO_SIZE_CAP_MB: u64 = 65_536;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZramSaturation {
    /// `/proc/swaps` name, e.g. `/dev/zram0`.
    pub device: String,
    pub used_bytes: u64,
    pub size_bytes: u64,
    /// Data on lower-priority disk swap (the overflow).
    pub overflow_bytes: u64,
    /// Measured `orig_data_size / compr_data_size`, when the device reports one.
    pub ratio: Option<f64>,
}

impl ZramSaturation {
    pub fn fill(&self) -> f64 {
        self.used_bytes as f64 / self.size_bytes.max(1) as f64
    }

    pub fn describe(&self) -> String {
        format!(
            "{} is {:.0}% full ({} / {}) and disk swap already holds {} of overflow",
            self.device,
            self.fill() * 100.0,
            format_bytes(self.used_bytes),
            format_bytes(self.size_bytes),
            format_bytes(self.overflow_bytes),
        )
    }
}

/// The first zram swap device that is at least [`NEAR_FULL_RATIO`] full while disk swap
/// with a lower priority holds data.
pub fn detect(status: &StatusReport) -> Option<ZramSaturation> {
    for zram in status.swaps.iter().filter(|s| s.name.contains("zram")) {
        if zram.size_bytes == 0
            || (zram.used_bytes as f64) < zram.size_bytes as f64 * NEAR_FULL_RATIO
        {
            continue;
        }
        let overflow_bytes: u64 = status
            .swaps
            .iter()
            .filter(|s| !s.name.contains("zram") && s.priority < zram.priority)
            .map(|s| s.used_bytes)
            .sum();
        if overflow_bytes == 0 {
            continue;
        }
        let short = zram.name.trim_start_matches("/dev/");
        let ratio = status
            .zram_devices
            .iter()
            .find(|d| d.name == short)
            .filter(|d| d.data_bytes > 0 && d.compressed_bytes > 0)
            .map(|d| d.data_bytes as f64 / d.compressed_bytes as f64);
        return Some(ZramSaturation {
            device: zram.name.clone(),
            used_bytes: zram.used_bytes,
            size_bytes: zram.size_bytes,
            overflow_bytes,
            ratio,
        });
    }
    None
}

/// `ram` multiplier so a full device costs at most [`TARGET_RAM_SHARE`] of RAM at `ratio`,
/// rounded down to a quarter.
pub fn ratio_multiplier(ratio: f64) -> f64 {
    let ratio = ratio.clamp(RATIO_MIN, RATIO_MAX);
    (ratio * TARGET_RAM_SHARE * 4.0).floor() / 4.0
}

/// zram-generator formula for a multiplier: `ram`, `ram * 1.5`, capped at
/// [`RATIO_SIZE_CAP_MB`].
pub fn ratio_formula(multiplier: f64) -> String {
    let core = if (multiplier - 1.0).abs() < f64::EPSILON {
        "ram".to_string()
    } else {
        format!("ram * {multiplier}")
    };
    format!("min({core}, {RATIO_SIZE_CAP_MB})")
}

/// A larger size proposal for a saturated device.
#[derive(Debug, Clone, PartialEq)]
pub struct SizeProposal {
    pub formula: String,
    pub approx_mib: u64,
    pub reason: String,
}

/// Size that absorbs the overflow: ratio-based when the device reports its compression
/// ratio, else `fallback_formula` (the next size tier). `eval` evaluates a formula to MiB.
pub fn propose_size(
    sat: &ZramSaturation,
    ram_mb: u64,
    fallback_formula: &str,
    eval: impl Fn(&str, u64) -> Option<u64>,
) -> Option<SizeProposal> {
    let (formula, why) = match sat.ratio {
        Some(ratio) => {
            let multiplier = ratio_multiplier(ratio);
            (
                ratio_formula(multiplier),
                format!(
                    "at the measured {ratio:.1}x compression ratio a full device of that size \
                     costs at most about {:.0}% of RAM",
                    TARGET_RAM_SHARE * 100.0
                ),
            )
        }
        None => (
            fallback_formula.to_string(),
            "no compression ratio was reported, so this is the next size tier".to_string(),
        ),
    };
    let approx_mib = eval(&formula, ram_mb)?;
    Some(SizeProposal {
        reason: format!(
            "{}; '{formula}' (~{}) holds it in RAM instead: {why}",
            sat.describe(),
            format_bytes(approx_mib * 1024 * 1024)
        ),
        formula,
        approx_mib,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::recommend::eval_zram_size_mb;
    use crate::status::{MemoryInfo, SwapEntry, ZramDevice};

    const GIB: u64 = 1 << 30;

    pub(crate) fn status_28(zram_used: u64, disk_used: u64) -> StatusReport {
        let swap = |name: &str, size, used, priority| SwapEntry {
            name: name.into(),
            swap_type: if name.contains("zram") {
                "partition"
            } else {
                "file"
            }
            .into(),
            size_bytes: size,
            used_bytes: used,
            priority,
        };
        StatusReport {
            swaps: vec![
                swap("/dev/zram0", 4 * GIB, zram_used, 100),
                swap("/swap/swapfile", 8 * GIB, disk_used, 10),
            ],
            zram_devices: vec![ZramDevice {
                name: "zram0".into(),
                algorithm: "zstd".into(),
                disk_size_bytes: 4 * GIB,
                data_bytes: 3_790_000_000,
                compressed_bytes: 1_200_000_000,
                total_bytes: 1_250_000_000,
                mem_used_max_bytes: 1_300_000_000,
                mount_point: "[SWAP]".into(),
            }],
            memory: MemoryInfo {
                mem_total_kb: 15_872 * 1024,
                mem_available_kb: 5 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        }
    }

    #[test]
    fn detects_issue_28_state() {
        let sat = detect(&status_28(4 * GIB, 700 << 20)).expect("saturated");
        assert_eq!(sat.device, "/dev/zram0");
        assert_eq!(sat.overflow_bytes, 700 << 20);
        let ratio = sat.ratio.unwrap();
        assert!((ratio - 3.158).abs() < 0.01, "{ratio}");
        assert!(sat.describe().contains("100% full"));
    }

    #[test]
    fn not_saturated_with_headroom_or_without_overflow() {
        assert!(detect(&status_28(2 * GIB, 700 << 20)).is_none());
        assert!(detect(&status_28(4 * GIB, 0)).is_none());
        // Disk swap above zram priority is not "overflow".
        let mut inverted = status_28(4 * GIB, 700 << 20);
        inverted.swaps[1].priority = 200;
        assert!(detect(&inverted).is_none());
    }

    #[test]
    fn ratio_multiplier_targets_half_of_ram() {
        assert_eq!(ratio_multiplier(3.158), 1.5);
        assert_eq!(ratio_multiplier(2.8), 1.25);
        assert_eq!(ratio_multiplier(2.0), 1.0);
        assert_eq!(ratio_multiplier(1.1), 0.75); // clamped to 1.5x
        assert_eq!(ratio_multiplier(9.0), 2.0); // clamped to 4x
        assert_eq!(ratio_formula(1.0), "min(ram, 65536)");
        assert_eq!(ratio_formula(1.5), "min(ram * 1.5, 65536)");
    }

    #[test]
    fn proposal_uses_ratio_then_fallback() {
        let sat = detect(&status_28(4 * GIB, 700 << 20)).unwrap();
        let p = propose_size(&sat, 15_872, "min(ram * 1.5, 24576)", eval_zram_size_mb).unwrap();
        assert_eq!(p.formula, "min(ram * 1.5, 65536)");
        assert_eq!(p.approx_mib, 23_808);
        assert!(p.reason.contains("3.2x"), "{}", p.reason);

        let no_ratio = ZramSaturation { ratio: None, ..sat };
        let p = propose_size(
            &no_ratio,
            15_872,
            "min(ram * 1.5, 24576)",
            eval_zram_size_mb,
        )
        .unwrap();
        assert_eq!(p.formula, "min(ram * 1.5, 24576)");
        assert!(p.reason.contains("next size tier"));
    }
}
