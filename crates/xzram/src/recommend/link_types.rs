//! Shared types for linked optimize.

use serde::{Deserialize, Serialize};

use crate::apply::PendingConfig;

use super::types::RecommendProfile;

/// Which field the user just edited (anchor is preserved when valid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkedAnchor {
    ZramSize,
    ZramAlgo,
    ZramPriority,
    Sysctl,
    SwapfileCreate,
    SwapfileResize,
    SwapfileRemove,
}

impl LinkedAnchor {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "zram_size" => Some(Self::ZramSize),
            "zram_algo" => Some(Self::ZramAlgo),
            "zram_priority" => Some(Self::ZramPriority),
            "sysctl" => Some(Self::Sysctl),
            "swapfile_create" => Some(Self::SwapfileCreate),
            "swapfile_resize" => Some(Self::SwapfileResize),
            "swapfile_remove" => Some(Self::SwapfileRemove),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ZramSize => "zram_size",
            Self::ZramAlgo => "zram_algo",
            Self::ZramPriority => "zram_priority",
            Self::Sysctl => "sysctl",
            Self::SwapfileCreate => "swapfile_create",
            Self::SwapfileResize => "swapfile_resize",
            Self::SwapfileRemove => "swapfile_remove",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinkedOptimizeContext {
    pub profile: RecommendProfile,
    pub mem_total_kb: u64,
    pub has_active_zram: bool,
    pub has_disk_swap: bool,
    pub hibernate_blocks_zram: bool,
    pub etc_writable: bool,
    pub immutable_os: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedOptimizeResult {
    pub pending: PendingConfig,
    pub adjustments: Vec<String>,
}
