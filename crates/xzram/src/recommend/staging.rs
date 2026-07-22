use crate::apply::ZramConfig;
use crate::sysctl::SysctlValues;

fn normalize_size_formula(formula: &str) -> String {
    formula
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Evaluate common zram-generator size formulas to MiB for a given RAM size.
pub fn eval_zram_size_mb(formula: &str, ram_mb: u64) -> Option<u64> {
    let f = normalize_size_formula(formula);
    match f.as_str() {
        "ram" => Some(ram_mb),
        "min(ram,8192)" => Some(ram_mb.min(8192)),
        "min(ram,4096)" => Some(ram_mb.min(4096)),
        "min(ram/2,8192)" => Some((ram_mb / 2).min(8192)),
        "min(ram/2,4096)" => Some((ram_mb / 2).min(4096)),
        _ => None,
    }
}

fn zram_size_needs_update(current: Option<&str>, recommended: &str, ram_mb: u64) -> bool {
    let Some(current) = current else {
        return true;
    };
    if normalize_size_formula(current) == normalize_size_formula(recommended) {
        return false;
    }
    !matches!(
        (
            eval_zram_size_mb(current, ram_mb),
            eval_zram_size_mb(recommended, ram_mb),
        ),
        (Some(c), Some(r)) if c >= r
    )
}

/// Prefer keeping a larger vendor size while still updating algo/priority/resident-limit.
pub(super) fn zram_for_staging(
    current: Option<&ZramConfig>,
    recommended: &ZramConfig,
    ram_mb: u64,
) -> ZramConfig {
    let mut staged = recommended.clone();
    if let Some(current) = current {
        if let (Some(cur_size), Some(rec_size)) = (
            current.zram_size.as_deref(),
            recommended.zram_size.as_deref(),
        ) {
            if !zram_size_needs_update(Some(cur_size), rec_size, ram_mb) {
                staged.zram_size = current.zram_size.clone();
            }
        }
    }
    staged
}

pub(super) fn zram_needs_update(
    current: Option<&ZramConfig>,
    recommended: &ZramConfig,
    ram_mb: u64,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    if current.device != recommended.device {
        return true;
    }
    if zram_size_needs_update(
        current.zram_size.as_deref(),
        recommended.zram_size.as_deref().unwrap_or(""),
        ram_mb,
    ) {
        return true;
    }
    current.zram_resident_limit != recommended.zram_resident_limit
        || current.compression_algorithm != recommended.compression_algorithm
        || current.swap_priority != recommended.swap_priority
}

pub(super) fn sysctl_needs_update(
    current: Option<&SysctlValues>,
    recommended: &SysctlValues,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    field_differs(current.swappiness, recommended.swappiness)
        || field_differs(
            current.watermark_boost_factor,
            recommended.watermark_boost_factor,
        )
        || field_differs(
            current.watermark_scale_factor,
            recommended.watermark_scale_factor,
        )
        || field_differs(current.page_cluster, recommended.page_cluster)
}

fn field_differs(current: Option<u32>, recommended: Option<u32>) -> bool {
    match (current, recommended) {
        (None, None) => false,
        (Some(a), Some(b)) => a != b,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zram_needs_update_detects_algorithm_change() {
        let current = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("min(ram / 2, 4096)".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("lzo-rle".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        };
        let recommended = ZramConfig {
            compression_algorithm: Some("zstd".into()),
            ..current.clone()
        };
        assert!(zram_needs_update(Some(&current), &recommended, 16 * 1024));
    }

    #[test]
    fn zram_needs_update_detects_resident_limit() {
        let current = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("ram".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        };
        let recommended = ZramConfig {
            zram_resident_limit: Some("ram / 2".into()),
            ..current.clone()
        };
        assert!(zram_needs_update(Some(&current), &recommended, 16 * 1024));
    }

    #[test]
    fn vendor_fedora_size_not_shrunk() {
        let current = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("min(ram, 8192)".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        };
        let recommended = ZramConfig {
            zram_size: Some("min(ram / 2, 4096)".into()),
            ..current.clone()
        };
        assert!(!zram_size_needs_update(
            current.zram_size.as_deref(),
            recommended.zram_size.as_deref().unwrap(),
            16 * 1024
        ));
        assert!(!zram_needs_update(Some(&current), &recommended, 16 * 1024));
    }

    #[test]
    fn vendor_size_kept_when_staging_algo_change() {
        let current = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("min(ram, 8192)".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("lzo-rle".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        };
        let recommended = ZramConfig {
            zram_size: Some("min(ram / 2, 4096)".into()),
            compression_algorithm: Some("zstd".into()),
            ..current.clone()
        };
        assert!(zram_needs_update(Some(&current), &recommended, 16 * 1024));
        let staged = zram_for_staging(Some(&current), &recommended, 16 * 1024);
        assert_eq!(staged.zram_size.as_deref(), Some("min(ram, 8192)"));
        assert_eq!(staged.compression_algorithm.as_deref(), Some("zstd"));
    }
}
