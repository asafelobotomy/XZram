use crate::apply::ZramConfig;
use crate::checks;
use crate::config::default_zram_config;
use crate::detect::{self, DetectionReport};
use crate::status::StatusReport;

use super::types::RecommendProfile;

pub(super) fn pick_profile(detection: &DetectionReport, status: &StatusReport) -> RecommendProfile {
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

pub(super) fn profile_reference(profile: RecommendProfile) -> Option<&'static str> {
    Some(match profile {
        RecommendProfile::Conservative => "profile-conservative",
        RecommendProfile::Performance => "profile-performance",
        RecommendProfile::Constrained => "profile-constrained",
    })
}

pub(super) fn build_recommended_zram(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::ZramBackend;
    use crate::status;

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
}
