use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::apply::{PendingConfig, SwapfileConfig, ZramConfig};
use crate::backend::{available_swapfile_backend, available_zram_backend};
use crate::checks;
use crate::config::default_zram_config;
use crate::detect::{self, DetectionReport, ZramBackend};
use crate::error::Result;
use crate::status::{self, StatusReport};
use crate::swap_partition;
use crate::sysctl::{self, SysctlValues};

pub const OVERFLOW_SWAPFILE_PATH: &str = "/swap/swapfile";
pub const OVERFLOW_SWAP_PRIORITY: i32 = 10;
pub const OVERFLOW_SWAPFILE_MAX_MB: u64 = 8192;
pub const OVERFLOW_FREE_SPACE_MARGIN_MB: u64 = 512;

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
    pub immutable_os: bool,
    pub etc_writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedDefaults {
    pub pending: PendingConfig,
    pub items: Vec<RecommendationItem>,
    pub context: SystemContext,
}

#[derive(Debug, Clone)]
pub enum OverflowDecision {
    Stage(SwapfileConfig),
    SkipActiveDiskSwap,
    SkipConfiguredDiskSwap { paths: Vec<String> },
    SkipInsufficientSpace { required_mb: u64, available_mb: u64 },
    SkipZeroSize,
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
    let staging_blocked = !detection.etc_writable || detection.immutable_os;
    let hibernate_blocks_zram = checks::hibernation_zram_conflict(&status.swaps);

    if !detection.etc_writable {
        items.push(note_item(
            "Read-only /etc: cannot stage configuration",
            "XZram cannot stage zram, sysctl, or swapfile changes on a read-only /etc. Use distro layering or a writable /etc overlay.",
            Some("doctor-mapping"),
        ));
    }

    if detection.immutable_os {
        items.push(note_item(
            "Immutable OS: cannot stage configuration",
            "This system looks immutable (NixOS, ostree, Silverblue, or similar). Configure swap via distro layering or the Doctor tab — Apply recommended defaults will not stage changes.",
            Some("doctor-mapping"),
        ));
    }

    if detection.zram_backend == ZramBackend::ZramTools {
        items.push(note_item(
            "Legacy zram-tools configuration detected",
            "Advisory only — Apply defaults does not migrate backends. Run 'xzram zram migrate' first if you are switching to systemd-zram-generator.",
            Some("doctor-mapping"),
        ));
    }

    if detection.zram_backend == ZramBackend::None && status.zram_devices.is_empty() {
        let pkg = detect::zram_generator_package_name(detection.package_manager);
        items.push(note_item(
            "No zram backend detected",
            format!(
                "Install {pkg} and apply defaults to create a zram swap device. Apply defaults does not install packages."
            ),
            Some("doctor-mapping"),
        ));
    }

    if !staging_blocked {
        let recommended_zram = build_recommended_zram(profile, &detection, &status);
        let ram_mb = status.memory.mem_total_kb / 1024;
        let skip_zram_staging = hibernate_blocks_zram;

        if skip_zram_staging {
            items.push(note_item(
                "Hibernation conflict: zram staging skipped",
                "Hibernation resume device points to zram. Disk-backed swap is required as the resume device.",
                Some("known-conflicts"),
            ));
        } else if zram_needs_update(current_zram.as_ref(), &recommended_zram, ram_mb) {
            let staged_zram = zram_for_staging(current_zram.as_ref(), &recommended_zram, ram_mb);
            let algo = staged_zram
                .compression_algorithm
                .as_deref()
                .unwrap_or("zstd");
            let size = staged_zram
                .zram_size
                .as_deref()
                .unwrap_or("min(ram / 2, 4096)");
            let pri = staged_zram.swap_priority.unwrap_or(100);
            let resident = staged_zram
                .zram_resident_limit
                .as_deref()
                .map(|r| format!(", resident-limit {r}"))
                .unwrap_or_default();
            items.push(RecommendationItem {
                category: "zram".into(),
                summary: format!("Configure ZRAM ({profile:?}, {algo}, priority {pri})"),
                detail: format!(
                    "Device {} with size formula '{size}'{resident} based on {} RAM",
                    staged_zram.device,
                    format_bytes(status.memory.mem_total_kb * 1024)
                ),
                will_stage: true,
                reference: Some(profile_reference(profile).unwrap().into()),
            });
            pending.zram = Some(staged_zram);
        } else if status.zram_devices.is_empty() {
            items.push(note_item(
                "ZRAM configuration already matches recommendations",
                "No zram generator changes needed.",
                profile_reference(profile),
            ));
        } else {
            items.push(note_item(
                "Active ZRAM already matches recommended settings",
                "Current generator config aligns with hardware-based defaults (vendor size is kept when already large enough).",
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

        let (configured_disk_swap, configured_paths) = probe_configured_disk_swap();
        let available_bytes = available_bytes_near(OVERFLOW_SWAPFILE_PATH);
        match decide_overflow_swapfile(
            &status,
            configured_disk_swap,
            &configured_paths,
            available_bytes,
        ) {
            OverflowDecision::Stage(swapfile) => {
                items.push(RecommendationItem {
                    category: "swapfile".into(),
                    summary: format!(
                        "Create overflow swap file ({} MiB, priority {})",
                        swapfile.size_mb, swapfile.priority
                    ),
                    detail: format!(
                        "Disk-backed safety net at {} (capped at {} MiB). Btrfs nodatacow is prepared automatically on apply.",
                        swapfile.path, OVERFLOW_SWAPFILE_MAX_MB
                    ),
                    will_stage: true,
                    reference: Some("overflow-swapfile".into()),
                });
                pending.swapfile = Some(swapfile);
                items.push(note_item(
                    "Dual-tier swap tradeoff",
                    "Advisory only — Apply defaults stages overflow as a safety net. If swap regularly exceeds ~30% of RAM, zswap may fit better — see docs/RECOMMENDATIONS.md#dual-tier-tradeoff.",
                    Some("dual-tier-tradeoff"),
                ));
            }
            OverflowDecision::SkipConfiguredDiskSwap { paths } => {
                items.push(note_item(
                    "Configured disk swap already present",
                    format!(
                        "Overflow swapfile not staged because fstab (or managed inventory) already lists non-zram swap: {}. Enable it with swapon or remove stale entries before adding {}.",
                        paths.join(", "),
                        OVERFLOW_SWAPFILE_PATH
                    ),
                    Some("overflow-swapfile"),
                ));
            }
            OverflowDecision::SkipInsufficientSpace {
                required_mb,
                available_mb,
            } => {
                items.push(note_item(
                    "Insufficient free disk space for overflow swapfile",
                    format!(
                        "Need about {required_mb} MiB free near {} (including {} MiB margin); about {available_mb} MiB available. Free space or choose a smaller swapfile manually.",
                        OVERFLOW_SWAPFILE_PATH, OVERFLOW_FREE_SPACE_MARGIN_MB
                    ),
                    Some("overflow-swapfile"),
                ));
            }
            OverflowDecision::SkipActiveDiskSwap | OverflowDecision::SkipZeroSize => {}
        }
    }

    items.extend(advisory_items(
        &detection,
        &status,
        current_zram.as_ref(),
        pending.swapfile.is_some(),
    ));

    if !staging_blocked && items.iter().all(|i| !i.will_stage) {
        items.insert(
            0,
            note_item(
                "System already matches recommended defaults",
                "No configuration changes will be staged.",
                None,
            ),
        );
    }

    let active_disk = has_active_disk_swap(&status);
    let (configured_disk_swap, _) = probe_configured_disk_swap();

    let context = SystemContext {
        mem_total_bytes: status.memory.mem_total_kb * 1024,
        mem_available_bytes: status.memory.mem_available_kb * 1024,
        has_active_zram: !status.zram_devices.is_empty(),
        has_disk_swap: active_disk || configured_disk_swap,
        distro: detection
            .distro
            .pretty_name
            .clone()
            .unwrap_or(detection.distro.id.clone()),
        root_filesystem: detection.root_filesystem.clone(),
        zram_backend: format!("{:?}", detection.zram_backend).to_lowercase(),
        profile: format!("{profile:?}").to_lowercase(),
        immutable_os: detection.immutable_os,
        etc_writable: detection.etc_writable,
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
            "Advisory only — Apply defaults does not disable zswap. Both zram and zswap are active; disable zswap before using zram (see Doctor tab)."
        } else {
            "Advisory only — Apply defaults does not disable zswap. Disable manually: echo 0 | sudo tee /sys/module/zswap/parameters/enabled, or add zswap.enabled=0 to the kernel cmdline."
        };
        items.push(note_item(
            "Disable zswap when using zram",
            detail,
            Some("known-conflicts"),
        ));
    }

    items.push(note_item(
        "When zswap may fit better",
        "Advisory only — Apply defaults does not configure zswap. If sustained swap use exceeds ~30% of RAM or is unpredictable on fast NVMe, consider a zswap-based setup instead of zram-only tuning.",
        Some("zswap-alternative"),
    ));

    if detection.root_filesystem.as_deref() == Some("zfs") {
        items.push(note_item(
            "ZFS root: swapfiles have special requirements",
            "Advisory only — prefer a dedicated swap partition or zvol on ZFS systems. Apply defaults will not create a ZFS-safe swap layout automatically.",
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
                        "Advisory only — generator config specifies '{configured}' but active device uses '{}'. Check the ZRAM tab or Doctor; Apply defaults may restage generator settings but cannot force a live algorithm change alone.",
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
            "Apply defaults stages zram priority 100 and disk swapfile priority 10 when those changes are in scope, to restore correct tiering.",
            Some("priority-tiers"),
        ));
    }

    if status.zram_devices.len() > 1 {
        items.push(note_item(
            "Multiple zram devices detected",
            "Advisory only — XZram manages swap on zram0 only. Additional zram devices (e.g. /tmp ramdisk) are not changed.",
            Some("multi-device"),
        ));
    }

    items.push(note_item(
        "Writeback device not used",
        "Advisory only — Apply defaults does not configure writeback-device. XZram uses a low-priority overflow swapfile instead (requires no separate daemon). See docs/RECOMMENDATIONS.md#writeback-device.",
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

/// Cap overflow at [`OVERFLOW_SWAPFILE_MAX_MB`].
pub fn overflow_size_mb(mem_total_kb: u64) -> u64 {
    (mem_total_kb / 1024).min(OVERFLOW_SWAPFILE_MAX_MB)
}

pub fn decide_overflow_swapfile(
    status: &StatusReport,
    has_configured_disk_swap: bool,
    configured_paths: &[String],
    available_bytes: Option<u64>,
) -> OverflowDecision {
    if has_active_disk_swap(status) {
        return OverflowDecision::SkipActiveDiskSwap;
    }
    if has_configured_disk_swap {
        return OverflowDecision::SkipConfiguredDiskSwap {
            paths: configured_paths.to_vec(),
        };
    }

    let size_mb = overflow_size_mb(status.memory.mem_total_kb);
    if size_mb == 0 {
        return OverflowDecision::SkipZeroSize;
    }

    let required_mb = size_mb.saturating_add(OVERFLOW_FREE_SPACE_MARGIN_MB);
    if let Some(avail) = available_bytes {
        let available_mb = avail / (1024 * 1024);
        if available_mb < required_mb {
            return OverflowDecision::SkipInsufficientSpace {
                required_mb,
                available_mb,
            };
        }
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

fn probe_configured_disk_swap() -> (bool, Vec<String>) {
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

fn available_bytes_near(path: &str) -> Option<u64> {
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
    match (
        eval_zram_size_mb(current, ram_mb),
        eval_zram_size_mb(recommended, ram_mb),
    ) {
        (Some(c), Some(r)) if c >= r => false,
        _ => true,
    }
}

/// Prefer keeping a larger vendor size while still updating algo/priority/resident-limit.
fn zram_for_staging(
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

fn zram_needs_update(current: Option<&ZramConfig>, recommended: &ZramConfig, ram_mb: u64) -> bool {
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

fn has_active_disk_swap(status: &StatusReport) -> bool {
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

    #[test]
    fn apt_package_name_is_systemd_zram_generator() {
        assert_eq!(
            detect::zram_generator_package_name(detect::PackageManager::Apt),
            "systemd-zram-generator"
        );
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
                immutable_os: false,
                etc_writable: true,
            },
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("sysctl-tuning"));
        assert!(json.contains("profile"));
        assert!(json.contains("immutable_os"));
        assert!(json.contains("etc_writable"));
    }
}
