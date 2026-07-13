use std::path::Path;

use crate::status::SwapEntry;

/// Whether zswap is enabled (`Y` or `1`). `None` if sysfs is unavailable.
pub fn zswap_enabled() -> Option<bool> {
    let path = Path::new("/sys/module/zswap/parameters/enabled");
    let content = std::fs::read_to_string(path).ok()?;
    let enabled = content.trim();
    Some(enabled == "Y" || enabled == "1")
}

pub fn zram_zswap_conflict(zram_devices: &[crate::status::ZramDevice]) -> bool {
    if zram_devices.is_empty() {
        return false;
    }
    matches!(zswap_enabled(), Some(true))
}

pub fn hibernation_zram_conflict(swaps: &[SwapEntry]) -> bool {
    let resume_path = Path::new("/sys/power/resume");
    let Ok(resume) = std::fs::read_to_string(resume_path) else {
        return false;
    };
    let resume = resume.trim();
    if resume.is_empty() || resume == "0:0" {
        return false;
    }

    if resume_device_is_zram(resume) {
        return true;
    }

    let _ = swaps;
    false
}

fn resume_device_is_zram(resume: &str) -> bool {
    let Some((major, minor)) = parse_major_minor(resume) else {
        return false;
    };

    if let Some(name) = block_device_name(major, minor) {
        return name.starts_with("zram");
    }

    false
}

fn parse_major_minor(value: &str) -> Option<(u32, u32)> {
    let (major, minor) = value.split_once(':')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn block_device_name(major: u32, minor: u32) -> Option<String> {
    let dev_path = format!("/sys/dev/block/{major}:{minor}");
    let link = std::fs::read_link(&dev_path).ok()?;
    link.file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

pub fn priority_inverted(swaps: &[SwapEntry]) -> bool {
    if swaps.len() < 2 {
        return false;
    }
    let zram_swaps: Vec<_> = swaps.iter().filter(|s| s.name.contains("zram")).collect();
    let disk_swaps: Vec<_> = swaps.iter().filter(|s| !s.name.contains("zram")).collect();
    if zram_swaps.is_empty() || disk_swaps.is_empty() {
        return false;
    }
    let zram_prio = zram_swaps.iter().map(|s| s.priority).max().unwrap_or(0);
    let disk_prio = disk_swaps.iter().map(|s| s.priority).max().unwrap_or(0);
    disk_prio >= zram_prio
}

pub fn algorithm_mismatch(configured: &str, active: &str) -> bool {
    let configured = configured.trim();
    let active = active.trim();
    if configured.is_empty() || active.is_empty() {
        return false;
    }
    !active.eq_ignore_ascii_case(configured)
        && !active.starts_with(configured)
        && !configured.starts_with(active)
}

pub fn cpu_core_count() -> u32 {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|content| {
            content
                .lines()
                .filter(|line| line.starts_with("processor"))
                .count() as u32
        })
        .unwrap_or(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_mismatch_detects_lzo_vs_zstd() {
        assert!(algorithm_mismatch("zstd", "lzo-rle"));
        assert!(!algorithm_mismatch("zstd", "zstd"));
    }

    #[test]
    fn priority_inverted_when_disk_higher() {
        let swaps = vec![
            SwapEntry {
                name: "/dev/zram0".into(),
                swap_type: "partition".into(),
                size_bytes: 0,
                used_bytes: 0,
                priority: 100,
            },
            SwapEntry {
                name: "/swapfile".into(),
                swap_type: "file".into(),
                size_bytes: 0,
                used_bytes: 0,
                priority: 100,
            },
        ];
        assert!(priority_inverted(&swaps));
    }

    #[test]
    fn parse_major_minor_works() {
        assert_eq!(parse_major_minor("259:5"), Some((259, 5)));
        assert_eq!(parse_major_minor("invalid"), None);
    }
}
