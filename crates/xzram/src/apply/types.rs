use serde::{Deserialize, Serialize};

use crate::sysctl::SysctlValues;

pub const PENDING_PATH: &str = "/var/lib/xzram/pending.json";
pub const SYSCTL_PATH: &str = "/etc/sysctl.d/99-xzram.conf";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramConfig {
    pub device: String,
    pub zram_size: Option<String>,
    pub zram_resident_limit: Option<String>,
    pub compression_algorithm: Option<String>,
    pub swap_priority: Option<i32>,
    pub fs_type: Option<String>,
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapfileConfig {
    pub path: String,
    pub size_mb: u64,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapfileResizeConfig {
    pub path: String,
    pub size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub zram: Option<ZramConfig>,
    pub swapfile: Option<SwapfileConfig>,
    pub disable_zram: bool,
    pub remove_swapfile: Option<String>,
}

/// Staged configuration merged into pending.json before `apply`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingConfig {
    pub zram: Option<ZramConfig>,
    pub disable_zram: bool,
    pub swapfile: Option<SwapfileConfig>,
    pub swapfile_resize: Option<SwapfileResizeConfig>,
    pub remove_swapfile: Option<String>,
    pub sysctl: Option<SysctlValues>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub success: bool,
    pub messages: Vec<String>,
}
