use serde::{Deserialize, Serialize};

use crate::apply::{PendingConfig, SwapfileConfig, ZramConfig};
use crate::backend::available_zram_backend;
use crate::checks;
use crate::config::default_zram_config;
use crate::detect::{self, DetectionReport, ZramBackend};
use crate::error::Result;
use crate::status::{self, StatusReport};
use crate::sysctl::{self, SysctlValues};

pub const OVERFLOW_SWAPFILE_PATH: &str = "/swap/swapfile";
pub const OVERFLOW_SWAP_PRIORITY: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendProfile {
    Conservative,
    Performance,
    Constrained,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationItem {
    pub category: String,
    pub summary: String,
    pub detail: String,
    pub will_stage: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    pub mem_total_bytes: u64,
    pub mem_available_bytes: u64,
    pub has_active_zram: bool,
    pub has_disk_swap: bool,
    pub distro: String,
    pub root_filesystem: Option<String>,
    pub zram_backend: String,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedDefaults {
    pub pending: PendingConfig,
    pub items: Vec<RecommendationItem>,
    pub context: SystemContext,
}

pub fn recommend() -> Result<RecommendedDefaults> {
    let detection = detect::detect()?;
    let status = status::status()?;
    let current_sysctl = sysctl::show().ok();
    let current_zram = available_zram_backend()
        .ok()
        .and_then(|b| b.show().ok())
        .flatten();

    let profile = pick_profile(&detection, &status);
    let mut items = Vec::new();
    let mut pending = PendingConfig::default();
    let skip_zram_staging =
        checks::hibernation_zram_conflict(&status.swaps) || !detection.etc_writable;

    if !detection.etc_writable {
        items.push(note_item(
            "Read-only /etc: staging skipped",
            "XZram cannot write configuration on this system. Use distro layering or a writable /etc overlay.",
            Some("doctor-mapping"),
        ));
    }

    let recommended_zram = build_recommended_zram(profile, &detection, &status);
    if detection.zram_backend == ZramBackend::ZramTools {
        items.push(note_item(
            "Legacy zram-tools configuration detected",
            "Apply will stage systemd-zram-generator settings. Run 'xzram zram migrate' first if you are switching backends.",
            Some("doctor-mapping"),
        ));
    }

    if detection.zram_backend == ZramBackend::None && status.zram_devices.is_empty() {
        items.push(note_item(
            "No zram backend detected",
            "Install zram-generator and apply defaults to create a zram swap device.",
            Some("doctor-mapping"),
        ));
    }

    if skip_zram_staging && detection.etc_writable {
        items.push(note_item(
            "Hibernation conflict: zram staging skipped",
            "Hibernation resume device points to zram. Disk-backed swap is required as the resume device.",
            Some("known-conflicts"),
        ));
    } else if zram_needs_update(current_zram.as_ref(), &recommended_zram) {
        let algo = recommended_zram
            .compression_algorithm
            .as_deref()
            .unwrap_or("zstd");
        let size = recommended_zram
            .zram_size
            .as_deref()
            .unwrap_or("min(ram / 2, 4096)");
        let pri = recommended_zram.swap_priority.unwrap_or(100);
        let resident = recommended_zram
            .zram_resident_limit
            .as_deref()
            .map(|r| format!(", resident-limit {r}"))
            .unwrap_or_default();
        items.push(RecommendationItem {
            category: "zram".into(),
            summary: format!("Configure ZRAM ({profile:?}, {algo}, priority {pri})"),
            detail: format!(
                "Device {} with size formula '{size}'{resident} based on {} RAM",
                recommended_zram.device,
                format_bytes(status.memory.mem_total_kb * 1024)
            ),
            will_stage: true,
            reference: Some(profile_reference(profile).unwrap().into()),
        });
        pending.zram = Some(recommended_zram);
    } else if status.zram_devices.is_empty() {
        items.push(note_item(
            "ZRAM configuration already matches recommendations",
            "No zram generator changes needed.",
            profile_reference(profile),
        ));
    } else {
        items.push(note_item(
            "Active ZRAM already matches recommended settings",
            "Current generator config aligns with hardware-based defaults.",
            profile_reference(profile),
        ));
    }

    if profile == RecommendProfile::Performance {
        items.push(note_item(
            "Performance profile resident-limit",
            "zram-resident-limit = ram / 2 caps RAM used for compressed pages when zram-size = ram. See docs/RECOMMENDATIONS.md#resident-limit.",
            Some("resident-limit"),
        ));
    }

    let recommended_sysctl = sysctl::zram_tuning_defaults();
    if sysctl_needs_update(current_sysctl.as_ref(), &recommended_sysctl) {
        items.push(RecommendationItem {
            category: "sysctl".into(),
            summary: "Apply zram sysctl tuning defaults".into(),
            detail: "vm.swappiness=180, vm.watermark_boost_factor=0, vm.watermark_scale_factor=125, vm.page-cluster=0".into(),
            will_stage: true,
            reference: Some("sysctl-tuning".into()),
        });
        pending.sysctl = Some(recommended_sysctl);
    } else {
        items.push(note_item(
            "Sysctl values already match zram tuning defaults",
            "No vm.* changes needed.",
            Some("sysctl-tuning"),
        ));
    }

    if let Some(swapfile) = build_overflow_swapfile(&status) {
        items.push(RecommendationItem {
            category: "swapfile".into(),
            summary: format!(
                "Create overflow swap file ({} MiB, priority {})",
                swapfile.size_mb, swapfile.priority
            ),
            detail: format!(
                "Disk-backed safety net at {} when zram is primary. Btrfs nodatacow is prepared automatically on apply.",
                swapfile.path
            ),
            will_stage: true,
            reference: Some("overflow-swapfile".into()),
        });
        pending.swapfile = Some(swapfile);
        items.push(note_item(
            "Dual-tier swap tradeoff",
            "Overflow swap is a safety net for occasional pressure. If swap regularly exceeds ~30% of RAM, zswap may fit better — see docs/RECOMMENDATIONS.md#dual-tier-tradeoff.",
            Some("dual-tier-tradeoff"),
        ));
    }

    items.extend(advisory_items(
        &detection,
        &status,
        current_zram.as_ref(),
        pending.swapfile.is_some(),
    ));

    if items.iter().all(|i| !i.will_stage) {
        items.insert(
            0,
            note_item(
                "System already matches recommended defaults",
                "No configuration changes will be staged.",
                None,
            ),
        );
    }

    let context = SystemContext {
        mem_total_bytes: status.memory.mem_total_kb * 1024,
        mem_available_bytes: status.memory.mem_available_kb * 1024,
        has_active_zram: !status.zram_devices.is_empty(),
        has_disk_swap: has_disk_swap(&status),
        distro: detection
            .distro
            .pretty_name
            .clone()
            .unwrap_or(detection.distro.id.clone()),
        root_filesystem: detection.root_filesystem.clone(),
        zram_backend: format!("{:?}", detection.zram_backend).to_lowercase(),
        profile: format!("{profile:?}").to_lowercase(),
    };

    Ok(RecommendedDefaults {
        pending,
        items,
        context,
    })
}

pub fn stage_recommended() -> Result<RecommendedDefaults> {
    let report = recommend()?;
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

fn note_item(
    summary: &str,
    detail: impl Into<String>,
    reference: Option<&str>,
) -> RecommendationItem {
    RecommendationItem {
        category: "note".into(),
        summary: summary.into(),
        detail: detail.into(),
        will_stage: false,
        reference: reference.map(str::to_string),
    }
}

fn advisory_items(
    detection: &DetectionReport,
    status: &StatusReport,
    current_zram: Option<&ZramConfig>,
    staging_overflow_swapfile: bool,
) -> Vec<RecommendationItem> {
    let mut items = Vec::new();

    if checks::zswap_enabled() == Some(true) {
        let detail = if checks::zram_zswap_conflict(&status.zram_devices) {
            "Both zram and zswap are active. Disable zswap before using zram (see Doctor tab)."
        } else {
            "Disable zswap: echo 0 | sudo tee /sys/module/zswap/parameters/enabled, or add zswap.enabled=0 to kernel cmdline."
        };
        items.push(note_item(
            "Disable zswap when using zram",
            detail,
            Some("known-conflicts"),
        ));
    }

    items.push(note_item(
        "When zswap may fit better",
        "If sustained swap use exceeds ~30% of RAM or is unpredictable on fast NVMe, consider a zswap-based setup instead of zram-only tuning.",
        Some("zswap-alternative"),
    ));

    if detection.root_filesystem.as_deref() == Some("zfs") {
        items.push(note_item(
            "ZFS root: swapfiles have special requirements",
            "Prefer a dedicated swap partition or zvol on ZFS systems.",
            Some("doctor-mapping"),
        ));
    }

    if detection.root_filesystem.as_deref() == Some("btrfs") && staging_overflow_swapfile {
        items.push(note_item(
            "Btrfs: nodatacow prepared on apply",
            format!(
                "Apply runs 'xzram swapfile prepare' before creating {}.",
                OVERFLOW_SWAPFILE_PATH
            ),
            Some("doctor-mapping"),
        ));
    }

    if let Some(configured) = current_zram.and_then(|z| z.compression_algorithm.as_deref()) {
        if let Some(active) = status.zram_devices.first() {
            if checks::algorithm_mismatch(configured, &active.algorithm) {
                items.push(note_item(
                    "ZRAM algorithm mismatch",
                    format!(
                        "Generator config specifies '{configured}' but active device uses '{}'. Check the ZRAM tab or Doctor.",
                        active.algorithm
                    ),
                    Some("known-conflicts"),
                ));
            }
        }
    }

    if checks::priority_inverted(&status.swaps) {
        items.push(note_item(
            "Swap priority inversion detected",
            "Apply defaults stages zram priority 100 and disk swapfile priority 10 to restore correct tiering.",
            Some("priority-tiers"),
        ));
    }

    if status.zram_devices.len() > 1 {
        items.push(note_item(
            "Multiple zram devices detected",
            "XZram manages swap on zram0 only. Additional zram devices (e.g. /tmp ramdisk) are not changed.",
            Some("multi-device"),
        ));
    }

    items.push(note_item(
        "Writeback device not used",
        "XZram uses a low-priority overflow swapfile instead of zram writeback-device (requires a separate daemon). See docs/RECOMMENDATIONS.md#writeback-device.",
        Some("writeback-device"),
    ));

    items
}

fn pick_profile(detection: &DetectionReport, status: &StatusReport) -> RecommendProfile {
    if detection.distro.id == "cachyos" {
        return RecommendProfile::Performance;
    }
    let mem_gb = status.memory.mem_total_kb as f64 / (1024.0 * 1024.0);
    if mem_gb < 4.0 {
        RecommendProfile::Constrained
    } else {
        RecommendProfile::Conservative
    }
}

fn profile_reference(profile: RecommendProfile) -> Option<&'static str> {
    Some(match profile {
        RecommendProfile::Conservative => "profile-conservative",
        RecommendProfile::Performance => "profile-performance",
        RecommendProfile::Constrained => "profile-constrained",
    })
}

fn build_recommended_zram(
    profile: RecommendProfile,
    detection: &DetectionReport,
    status: &StatusReport,
) -> ZramConfig {
    let defaults = default_zram_config();
    let mem_gb = status.memory.mem_total_kb as f64 / (1024.0 * 1024.0);
    let algorithm = pick_compression_algorithm(mem_gb, detection);
    let zram_size = pick_zram_size_formula(profile, mem_gb);
    let zram_resident_limit = pick_zram_resident_limit(profile);

    ZramConfig {
        device: defaults.name,
        zram_size: Some(zram_size),
        zram_resident_limit,
        compression_algorithm: Some(algorithm),
        swap_priority: defaults.swap_priority,
        fs_type: None,
        mount_point: None,
    }
}

fn pick_compression_algorithm(mem_gb: f64, detection: &DetectionReport) -> String {
    pick_compression_algorithm_for_cores(mem_gb, detection, checks::cpu_core_count())
}

fn pick_compression_algorithm_for_cores(
    mem_gb: f64,
    detection: &DetectionReport,
    cpu_cores: u32,
) -> String {
    if detection.distro.family == detect::DistroFamily::Arch || detection.distro.id == "cachyos" {
        return "zstd".into();
    }
    if mem_gb < 4.0 && cpu_cores < 4 {
        "lz4".into()
    } else {
        "zstd".into()
    }
}

fn pick_zram_size_formula(profile: RecommendProfile, mem_gb: f64) -> String {
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

fn pick_zram_resident_limit(profile: RecommendProfile) -> Option<String> {
    match profile {
        RecommendProfile::Performance => Some("ram / 2".into()),
        RecommendProfile::Conservative | RecommendProfile::Constrained => None,
    }
}

pub fn build_overflow_swapfile(status: &StatusReport) -> Option<SwapfileConfig> {
    if has_disk_swap(status) {
        return None;
    }

    let size_mb = status.memory.mem_total_kb / 1024;
    if size_mb == 0 {
        return None;
    }

    Some(SwapfileConfig {
        path: OVERFLOW_SWAPFILE_PATH.into(),
        size_mb,
        priority: OVERFLOW_SWAP_PRIORITY,
    })
}

fn zram_needs_update(current: Option<&ZramConfig>, recommended: &ZramConfig) -> bool {
    let Some(current) = current else {
        return true;
    };
    current.device != recommended.device
        || current.zram_size != recommended.zram_size
        || current.zram_resident_limit != recommended.zram_resident_limit
        || current.compression_algorithm != recommended.compression_algorithm
        || current.swap_priority != recommended.swap_priority
}

fn sysctl_needs_update(current: Option<&SysctlValues>, recommended: &SysctlValues) -> bool {
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

fn has_disk_swap(status: &StatusReport) -> bool {
    status.swaps.iter().any(|s| !s.name.contains("zram"))
}

fn format_bytes(bytes: u64) -> String {
    status::format_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ubuntu_detection() -> DetectionReport {
        DetectionReport {
            distro: detect::DistroInfo {
                id: "ubuntu".into(),
                id_like: vec!["debian".into()],
                family: detect::DistroFamily::Ubuntu,
                version_id: None,
                pretty_name: None,
            },
            package_manager: detect::PackageManager::Apt,
            init_system: "systemd".into(),
            zram_backend: ZramBackend::None,
            zram_generator_installed: false,
            zram_generator_config: None,
            root_filesystem: None,
            etc_writable: true,
            immutable_os: false,
        }
    }

    fn cachyos_detection() -> DetectionReport {
        DetectionReport {
            distro: detect::DistroInfo {
                id: "cachyos".into(),
                id_like: vec!["arch".into()],
                family: detect::DistroFamily::Arch,
                version_id: None,
                pretty_name: Some("CachyOS".into()),
            },
            package_manager: detect::PackageManager::Pacman,
            init_system: "systemd".into(),
            zram_backend: ZramBackend::None,
            zram_generator_installed: false,
            zram_generator_config: None,
            root_filesystem: None,
            etc_writable: true,
            immutable_os: false,
        }
    }

    #[test]
    fn pick_algorithm_low_ram_weak_cpu() {
        let detection = ubuntu_detection();
        assert_eq!(
            pick_compression_algorithm_for_cores(2.0, &detection, 2),
            "lz4"
        );
        assert_eq!(
            pick_compression_algorithm_for_cores(2.0, &detection, 8),
            "zstd"
        );
        assert_eq!(pick_compression_algorithm(8.0, &detection), "zstd");
    }

    #[test]
    fn cachyos_uses_performance_size_and_resident_limit() {
        let detection = cachyos_detection();
        let status = StatusReport {
            swaps: vec![],
            zram_devices: vec![],
            memory: status::MemoryInfo {
                mem_total_kb: 16 * 1024 * 1024,
                mem_available_kb: 8 * 1024 * 1024,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        };
        let profile = pick_profile(&detection, &status);
        assert_eq!(profile, RecommendProfile::Performance);
        let zram = build_recommended_zram(profile, &detection, &status);
        assert_eq!(zram.zram_size.as_deref(), Some("ram"));
        assert_eq!(zram.zram_resident_limit.as_deref(), Some("ram / 2"));
    }

    #[test]
    fn conservative_large_ram_caps_at_8192() {
        assert_eq!(
            pick_zram_size_formula(RecommendProfile::Conservative, 64.0),
            "min(ram / 2, 8192)"
        );
    }

    #[test]
    fn constrained_profile_size() {
        assert_eq!(
            pick_zram_size_formula(RecommendProfile::Constrained, 2.0),
            "min(ram, 4096)"
        );
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
        let swapfile = build_overflow_swapfile(&status).expect("should stage overflow");
        assert_eq!(swapfile.path, OVERFLOW_SWAPFILE_PATH);
        assert_eq!(swapfile.size_mb, 8192);
        assert_eq!(swapfile.priority, OVERFLOW_SWAP_PRIORITY);
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
        assert!(build_overflow_swapfile(&status).is_none());
    }

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
        assert!(zram_needs_update(Some(&current), &recommended));
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
        assert!(zram_needs_update(Some(&current), &recommended));
    }

    #[test]
    fn recommend_serializes_reference_field() {
        let report = RecommendedDefaults {
            pending: PendingConfig::default(),
            items: vec![RecommendationItem {
                category: "sysctl".into(),
                summary: "test".into(),
                detail: "detail".into(),
                will_stage: true,
                reference: Some("sysctl-tuning".into()),
            }],
            context: SystemContext {
                mem_total_bytes: 8 * 1024 * 1024 * 1024,
                mem_available_bytes: 4 * 1024 * 1024 * 1024,
                has_active_zram: true,
                has_disk_swap: false,
                distro: "Test".into(),
                root_filesystem: Some("btrfs".into()),
                zram_backend: "systemd_zram_generator".into(),
                profile: "conservative".into(),
            },
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("sysctl-tuning"));
        assert!(json.contains("profile"));
    }
}
