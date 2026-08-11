use crate::backend::available_zram_backend;
use crate::detect;
use crate::error::Result;
use crate::status;
use crate::sysctl;

use super::engine_build::recommend_from_context_scaled;
use super::scales::RecommendScales;
use super::types::RecommendedDefaults;

pub fn recommend() -> Result<RecommendedDefaults> {
    recommend_with_scales(RecommendScales::default())
}

pub fn recommend_with_scales(scales: RecommendScales) -> Result<RecommendedDefaults> {
    let detection = detect::detect()?;
    let status = status::status()?;
    let current_sysctl = sysctl::show().ok();
    let current_zram = available_zram_backend()
        .ok()
        .and_then(|b| b.show().ok())
        .flatten();

    Ok(recommend_from_context_scaled(
        &detection,
        &status,
        current_sysctl,
        current_zram,
        None,
        scales,
    ))
}

pub fn stage_recommended() -> Result<RecommendedDefaults> {
    stage_recommended_with_scales(RecommendScales::default())
}

pub fn stage_recommended_with_scales(scales: RecommendScales) -> Result<RecommendedDefaults> {
    let report = recommend_with_scales(scales)?;
    if !report.pending.zram.is_none()
        || report.pending.sysctl.is_some()
        || report.pending.disable_zram
        || report.pending.swapfile.is_some()
        || report.pending.swapfile_resize.is_some()
        || report.pending.remove_swapfile.is_some()
    {
        crate::apply::stage(&report.pending)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::super::engine_build::{recommend_from_context_scaled, OverflowInputs};
    use super::super::scales::{RecommendScales, RecommendSizeScale};
    use crate::apply::ZramConfig;
    use crate::detect::{DetectionReport, DistroFamily, DistroInfo, PackageManager, ZramBackend};
    use crate::status::{MemoryInfo, StatusReport};
    use crate::sysctl::SysctlValues;

    fn base_detection(etc_writable: bool, immutable_os: bool) -> DetectionReport {
        DetectionReport {
            distro: DistroInfo {
                id: "fedora".into(),
                id_like: vec![],
                family: DistroFamily::Fedora,
                version_id: Some("40".into()),
                pretty_name: Some("Fedora".into()),
            },
            package_manager: PackageManager::Dnf,
            init_system: "systemd".into(),
            zram_backend: ZramBackend::SystemdZramGenerator,
            zram_generator_installed: true,
            zram_generator_config: None,
            root_filesystem: Some("ext4".into()),
            etc_writable,
            immutable_os,
        }
    }

    fn base_status(mem_total_kb: u64) -> StatusReport {
        StatusReport {
            swaps: vec![],
            zram_devices: vec![],
            memory: MemoryInfo {
                mem_total_kb,
                mem_available_kb: mem_total_kb / 2,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        }
    }

    #[test]
    fn immutable_os_leaves_pending_empty() {
        let report = recommend_from_context_scaled(
            &base_detection(true, true),
            &base_status(16 * 1024 * 1024),
            None,
            None,
            Some(OverflowInputs {
                configured_disk_swap: false,
                configured_paths: vec![],
                available_bytes: Some(100 * 1024 * 1024 * 1024),
            }),
            RecommendScales::default(),
        );
        assert!(report.pending.zram.is_none());
        assert!(report.pending.sysctl.is_none());
        assert!(report.pending.swapfile.is_none());
        assert!(report
            .items
            .iter()
            .any(|i| i.summary.to_lowercase().contains("immutable")));
    }

    #[test]
    fn read_only_etc_leaves_pending_empty() {
        let report = recommend_from_context_scaled(
            &base_detection(false, false),
            &base_status(16 * 1024 * 1024),
            None,
            None,
            Some(OverflowInputs::default()),
            RecommendScales::default(),
        );
        assert!(report.pending.zram.is_none());
        assert!(report.pending.sysctl.is_none());
        assert!(report.pending.swapfile.is_none());
        assert!(report
            .items
            .iter()
            .any(|i| i.summary.to_lowercase().contains("read-only")));
    }

    #[test]
    fn overflow_may_stage_with_injected_space() {
        let matching_sysctl = SysctlValues {
            swappiness: Some(180),
            watermark_boost_factor: Some(0),
            watermark_scale_factor: Some(125),
            page_cluster: Some(0),
        };
        let current_zram = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("min(ram / 2, 4096)".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        };
        let report = recommend_from_context_scaled(
            &base_detection(true, false),
            &base_status(16 * 1024 * 1024),
            Some(matching_sysctl),
            Some(current_zram),
            Some(OverflowInputs {
                configured_disk_swap: false,
                configured_paths: vec![],
                available_bytes: Some(100 * 1024 * 1024 * 1024),
            }),
            RecommendScales::default(),
        );
        assert!(report.pending.swapfile.is_some());
        assert!(report.items.iter().any(|i| i.category == "swapfile"));
    }

    #[test]
    fn vendor_size_not_shrunk_via_engine() {
        let current_zram = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("min(ram, 8192)".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("lzo-rle".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        };
        let matching_sysctl = SysctlValues {
            swappiness: Some(180),
            watermark_boost_factor: Some(0),
            watermark_scale_factor: Some(125),
            page_cluster: Some(0),
        };
        let report = recommend_from_context_scaled(
            &base_detection(true, false),
            &base_status(16 * 1024 * 1024),
            Some(matching_sysctl),
            Some(current_zram),
            Some(OverflowInputs {
                configured_disk_swap: true,
                configured_paths: vec!["/swapfile".into()],
                available_bytes: Some(100 * 1024 * 1024 * 1024),
            }),
            RecommendScales::default(),
        );
        let staged = report.pending.zram.expect("algo change should stage");
        assert_eq!(staged.zram_size.as_deref(), Some("min(ram, 8192)"));
        assert_eq!(staged.compression_algorithm.as_deref(), Some("zstd"));
    }

    #[test]
    fn high_zram_scale_uses_larger_formula() {
        let report = recommend_from_context_scaled(
            &base_detection(true, false),
            &base_status(8 * 1024 * 1024),
            None,
            None,
            Some(OverflowInputs {
                configured_disk_swap: true,
                configured_paths: vec!["/swapfile".into()],
                available_bytes: Some(100 * 1024 * 1024 * 1024),
            }),
            RecommendScales {
                zram: RecommendSizeScale::High,
                swapfile: RecommendSizeScale::Default,
            },
        );
        let staged = report.pending.zram.expect("zram should stage");
        assert_eq!(staged.zram_size.as_deref(), Some("min(ram, 8192)"));
        assert_eq!(report.size_scales.zram.selected, RecommendSizeScale::High);
    }
}
