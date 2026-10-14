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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Runtime zswap state when captured; `None` for older snapshots or no zswap support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zswap_enabled: Option<bool>,
    /// Whether systemd-oomd was enabled at boot when captured; `None` for older snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oomd_enabled: Option<bool>,
    /// Live MGLRU / THP sysfs values when captured; `None` for older snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mm_runtime: Option<crate::mm_tunables::MmTunables>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotArtifacts {
    pub zram_generator_conf: SnapshotArtifact,
    pub fstab: SnapshotArtifact,
    pub sysctl: SnapshotArtifact,
    pub zramswap: SnapshotArtifact,
    #[serde(default)]
    pub zswap: SnapshotArtifact,
    #[serde(default)]
    pub oomd_root: SnapshotArtifact,
    #[serde(default)]
    pub oomd_user: SnapshotArtifact,
    #[serde(default)]
    pub oomd_session: SnapshotArtifact,
    #[serde(default)]
    pub mm: SnapshotArtifact,
}
