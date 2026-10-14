use std::path::{Path, PathBuf};
use std::process::Command;

use crate::apply::SwapfileConfig;
use crate::backend::available_swapfile_backend;
use crate::status::StatusReport;
use crate::swap_partition;

use super::types::{
    OverflowDecision, OVERFLOW_FREE_SPACE_MARGIN_MB, OVERFLOW_SWAPFILE_PATH, OVERFLOW_SWAP_PRIORITY,
};

/// Cap overflow at [`OVERFLOW_SWAPFILE_MAX_MB`] (recommended / default scale).
pub fn overflow_size_mb(mem_total_kb: u64) -> u64 {
    super::scales::overflow_size_mb_for_scale(
        mem_total_kb,
        super::scales::RecommendSizeScale::Default,
    )
}

pub fn decide_overflow_swapfile(
    status: &StatusReport,
    has_configured_disk_swap: bool,
    configured_paths: &[String],
    available_bytes: Option<u64>,
) -> OverflowDecision {
    decide_overflow_swapfile_scaled(
        status,
        has_configured_disk_swap,
        configured_paths,
        available_bytes,
        super::scales::RecommendSizeScale::Default,
    )
}

pub fn decide_overflow_swapfile_scaled(
    status: &StatusReport,
    has_configured_disk_swap: bool,
    configured_paths: &[String],
    available_bytes: Option<u64>,
    scale: super::scales::RecommendSizeScale,
) -> OverflowDecision {
    if has_active_disk_swap(status) {
        return decide_grow_existing(status, available_bytes, scale);
    }
    if has_configured_disk_swap {
        return OverflowDecision::SkipConfiguredDiskSwap {
            paths: configured_paths.to_vec(),
        };
    }

    let size_mb = super::scales::overflow_size_mb_for_scale(status.memory.mem_total_kb, scale);
    if size_mb == 0 {
        return OverflowDecision::SkipZeroSize;
    }

    let required_mb = size_mb.saturating_add(OVERFLOW_FREE_SPACE_MARGIN_MB);
    let Some(avail) = available_bytes else {
        // Unknown free space: do not stage a large overflow file blindly.
        return OverflowDecision::SkipInsufficientSpace {
            required_mb,
            available_mb: 0,
        };
    };
    let available_mb = avail / (1024 * 1024);
    if available_mb < required_mb {
        return OverflowDecision::SkipInsufficientSpace {
            required_mb,
            available_mb,
        };
    }

    OverflowDecision::Stage(SwapfileConfig {
        path: OVERFLOW_SWAPFILE_PATH.into(),
        size_mb,
        priority: OVERFLOW_SWAP_PRIORITY,
    })
}

pub fn build_overflow_swapfile(status: &StatusReport) -> Option<SwapfileConfig> {
    let (configured, paths) = probe_configured_disk_swap();
    let available = available_bytes_near(OVERFLOW_SWAPFILE_PATH);
    match decide_overflow_swapfile(status, configured, &paths, available) {
        OverflowDecision::Stage(config) => Some(config),
        _ => None,
    }
}

pub(super) fn probe_configured_disk_swap() -> (bool, Vec<String>) {
    let mut paths = Vec::new();

    if let Ok(files) = available_swapfile_backend().list() {
        for file in files {
            if !file.path.contains("zram") {
                paths.push(file.path);
            }
        }
    }

    if let Ok(partitions) = swap_partition::list_swap_partitions() {
        for part in partitions {
            if !part.device.contains("zram") {
                paths.push(part.device);
            }
        }
    }

    paths.sort();
    paths.dedup();
    (!paths.is_empty(), paths)
}

pub(super) fn available_bytes_near(path: &str) -> Option<u64> {
    let mut probe = PathBuf::from(path);
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    if !probe.exists() {
        probe = PathBuf::from("/");
    }
    df_available_bytes(&probe)
}

fn df_available_bytes(path: &Path) -> Option<u64> {
    let output = Command::new("df")
        .args(["-B1", "--output=avail"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return trimmed.parse().ok();
    }
    None
}

/// Slack below the target before an existing overflow swapfile counts as undersized
/// (`/proc/swaps` reports the size minus the header page).
const GROW_SLACK_MB: u64 = 64;

/// Active disk swap exists. If it is solely an undersized swapfile at the managed overflow
/// path (e.g. a distro-installed 512 MiB `/swap/swapfile`), grow it to the target size;
/// any other disk swap is left alone.
fn decide_grow_existing(
    status: &StatusReport,
    available_bytes: Option<u64>,
    scale: super::scales::RecommendSizeScale,
) -> OverflowDecision {
    let disk: Vec<_> = status
        .swaps
        .iter()
        .filter(|s| !s.name.contains("zram"))
        .collect();
    let [only] = disk.as_slice() else {
        return OverflowDecision::SkipActiveDiskSwap;
    };
    if only.name != OVERFLOW_SWAPFILE_PATH || only.swap_type != "file" {
        return OverflowDecision::SkipActiveDiskSwap;
    }
    let size_mb = super::scales::overflow_size_mb_for_scale(status.memory.mem_total_kb, scale);
    let current_mb = only.size_bytes / (1024 * 1024);
    if current_mb.saturating_add(GROW_SLACK_MB) >= size_mb {
        return OverflowDecision::SkipActiveDiskSwap;
    }
    // In-place growth only needs the delta; apply re-checks space before touching anything.
    let required_mb = (size_mb - current_mb).saturating_add(OVERFLOW_FREE_SPACE_MARGIN_MB);
    let available_mb = available_bytes.unwrap_or(0) / (1024 * 1024);
    if available_mb < required_mb {
        return OverflowDecision::SkipInsufficientSpace {
            required_mb,
            available_mb,
        };
    }
    OverflowDecision::Grow {
        path: OVERFLOW_SWAPFILE_PATH.into(),
        from_mb: current_mb,
        to_mb: size_mb,
    }
}

pub(super) fn has_active_disk_swap(status: &StatusReport) -> bool {
    status.swaps.iter().any(|s| !s.name.contains("zram"))
}

#[cfg(test)]
mod tests {
    use super::super::types::OVERFLOW_SWAPFILE_MAX_MB;
    use super::*;
    use crate::status;

    #[test]
    fn overflow_size_capped_at_8_gib() {
        assert_eq!(overflow_size_mb(32 * 1024 * 1024), 8192);
        assert_eq!(overflow_size_mb(4 * 1024 * 1024), 4096);
    }

    #[test]
    fn overflow_swapfile_when_no_disk_swap() {
        let status = StatusReport {
            swaps: vec![status::SwapEntry {
                name: "/dev/zram0".into(),
                swap_type: "partition".into(),
                size_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 0,
                priority: 100,
            }],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 8 * 1024 * 1024,
                mem_available_kb: 4 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let decision = decide_overflow_swapfile(&status, false, &[], Some(64 * 1024 * 1024 * 1024));
        match decision {
            OverflowDecision::Stage(swapfile) => {
                assert_eq!(swapfile.path, OVERFLOW_SWAPFILE_PATH);
                assert_eq!(swapfile.size_mb, 8192);
                assert_eq!(swapfile.priority, OVERFLOW_SWAP_PRIORITY);
            }
            other => panic!("expected Stage, got {other:?}"),
        }
    }

    #[test]
    fn overflow_capped_for_large_ram() {
        let status = StatusReport {
            swaps: vec![],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 64 * 1024 * 1024,
                mem_available_kb: 32 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let decision =
            decide_overflow_swapfile(&status, false, &[], Some(128 * 1024 * 1024 * 1024));
        match decision {
            OverflowDecision::Stage(swapfile) => {
                assert_eq!(swapfile.size_mb, OVERFLOW_SWAPFILE_MAX_MB);
            }
            other => panic!("expected Stage, got {other:?}"),
        }
    }

    #[test]
    fn no_overflow_when_disk_swap_exists() {
        let status = StatusReport {
            swaps: vec![status::SwapEntry {
                name: "/swapfile".into(),
                swap_type: "file".into(),
                size_bytes: 1024,
                used_bytes: 0,
                priority: 10,
            }],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 8 * 1024 * 1024,
                mem_available_kb: 4 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        assert!(matches!(
            decide_overflow_swapfile(&status, false, &[], Some(64 * 1024 * 1024 * 1024)),
            OverflowDecision::SkipActiveDiskSwap
        ));
    }

    #[test]
    fn grows_undersized_managed_swapfile() {
        let status = StatusReport {
            swaps: vec![status::SwapEntry {
                name: OVERFLOW_SWAPFILE_PATH.into(),
                swap_type: "file".into(),
                size_bytes: 512 * 1024 * 1024 - 4096,
                used_bytes: 0,
                priority: -1,
            }],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 16 * 1024 * 1024,
                mem_available_kb: 8 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let space = Some(64 * 1024 * 1024 * 1024);
        match decide_overflow_swapfile_scaled(
            &status,
            true,
            &[],
            space,
            super::super::scales::RecommendSizeScale::High,
        ) {
            OverflowDecision::Grow {
                path,
                from_mb,
                to_mb,
            } => {
                assert_eq!(path, OVERFLOW_SWAPFILE_PATH);
                assert_eq!(from_mb, 511);
                assert_eq!(to_mb, 16384);
            }
            other => panic!("expected Grow, got {other:?}"),
        }
        // Already at target: nothing to do.
        let mut sized = status.clone();
        sized.swaps[0].size_bytes = 8192 * 1024 * 1024 - 4096;
        assert!(matches!(
            decide_overflow_swapfile(&sized, true, &[], space),
            OverflowDecision::SkipActiveDiskSwap
        ));
    }

    #[test]
    fn no_overflow_when_fstab_disk_swap_configured() {
        let status = StatusReport {
            swaps: vec![],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 8 * 1024 * 1024,
                mem_available_kb: 4 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let paths = vec!["/swap/oldswap".into()];
        match decide_overflow_swapfile(&status, true, &paths, Some(64 * 1024 * 1024 * 1024)) {
            OverflowDecision::SkipConfiguredDiskSwap { paths: p } => {
                assert_eq!(p, paths);
            }
            other => panic!("expected SkipConfiguredDiskSwap, got {other:?}"),
        }
    }

    #[test]
    fn no_overflow_when_insufficient_space() {
        let status = StatusReport {
            swaps: vec![],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 8 * 1024 * 1024,
                mem_available_kb: 4 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let decision = decide_overflow_swapfile(&status, false, &[], Some(100 * 1024 * 1024));
        match decision {
            OverflowDecision::SkipInsufficientSpace {
                required_mb,
                available_mb,
            } => {
                assert_eq!(required_mb, 8192 + OVERFLOW_FREE_SPACE_MARGIN_MB);
                assert_eq!(available_mb, 100);
            }
            other => panic!("expected SkipInsufficientSpace, got {other:?}"),
        }
    }

    #[test]
    fn no_overflow_when_free_space_unknown() {
        let status = StatusReport {
            swaps: vec![],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 8 * 1024 * 1024,
                mem_available_kb: 4 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        assert!(matches!(
            decide_overflow_swapfile(&status, false, &[], None),
            OverflowDecision::SkipInsufficientSpace {
                available_mb: 0,
                ..
            }
        ));
    }
}
