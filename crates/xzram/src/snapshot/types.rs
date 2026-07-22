use serde::{Deserialize, Serialize};

use crate::error::{Result, XzramError};
use crate::status::ZramDevice;

pub const SNAPSHOTS_DIR: &str = "snapshots";
pub const DEFAULT_KEEP: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTrigger {
    AppOpen,
    PreApply,
    Manual,
}

impl SnapshotTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppOpen => "app_open",
            Self::PreApply => "pre_apply",
            Self::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "app_open" => Ok(Self::AppOpen),
            "pre_apply" => Ok(Self::PreApply),
            "manual" => Ok(Self::Manual),
            _ => Err(XzramError::Validation(format!(
                "unknown snapshot trigger: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotArtifact {
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSwapfile {
    pub path: String,
    pub size_mb: u64,
    pub priority: i32,
    pub present_on_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub created_at: String,
    pub label: String,
    pub trigger: SnapshotTrigger,
    pub state_hash: String,
    pub xzram_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_summary: Option<String>,
    pub artifacts: SnapshotArtifacts,
    pub swapfiles: Vec<SnapshotSwapfile>,
    pub zram_devices: Vec<ZramDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotArtifacts {
    pub zram_generator_conf: SnapshotArtifact,
    pub fstab: SnapshotArtifact,
    pub sysctl: SnapshotArtifact,
    pub zramswap: SnapshotArtifact,
}
