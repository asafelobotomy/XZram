use serde::{Deserialize, Serialize};

use crate::apply::{PendingConfig, SwapfileConfig};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::PendingConfig;
    use crate::detect;

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
