use serde::{Deserialize, Serialize};

use std::io::BufRead;

use crate::apply::run_command;
use crate::error::{Result, XzramError};
use crate::status;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapPartition {
    pub device: String,
    pub uuid: Option<String>,
    pub fstab_options: String,
    pub priority: i32,
    pub active: bool,
    pub source: SwapSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapSource {
    Active,
    Fstab,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapListEntry {
    pub name: String,
    pub swap_type: String,
    pub size_bytes: u64,
    pub used_bytes: u64,
    pub priority: i32,
    pub source: SwapSource,
    pub active: bool,
}

const FSTAB_PATH: &str = "/etc/fstab";

pub fn list_swap_partitions() -> Result<Vec<SwapPartition>> {
    let file = std::fs::File::open(FSTAB_PATH)?;
    let reader = std::io::BufReader::new(file);
    let active = active_swap_devices()?;
    let mut partitions = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 || parts[2] != "swap" {
            continue;
        }
        let spec = parts[0];
        if !(spec.starts_with("/dev/") || spec.starts_with("UUID=")) {
            continue;
        }

        let device = resolve_device(spec)?;
        let active_flag = active
            .iter()
            .any(|a| a == &device || a.ends_with(spec.trim_start_matches("/dev/")));
        let priority = crate::backend::swapfile::parse_fstab_priority(
            parts.get(3).copied().unwrap_or("defaults"),
        );

        partitions.push(SwapPartition {
            device: device.clone(),
            uuid: extract_uuid(spec),
            fstab_options: parts.get(3).copied().unwrap_or("defaults").to_string(),
            priority,
            active: active_flag,
            source: if active_flag {
                SwapSource::Active
            } else {
                SwapSource::Fstab
            },
        });
    }

    Ok(partitions)
}

pub fn list_swaps_merged() -> Result<Vec<SwapListEntry>> {
    let report = status::status()?;
    let partitions = list_swap_partitions()?;
    let mut entries: Vec<SwapListEntry> = report
        .swaps
        .iter()
        .map(|s| SwapListEntry {
            name: s.name.clone(),
            swap_type: s.swap_type.clone(),
            size_bytes: s.size_bytes,
            used_bytes: s.used_bytes,
            priority: s.priority,
            source: SwapSource::Active,
            active: true,
        })
        .collect();

    for part in partitions {
        if entries.iter().any(|e| e.name == part.device) {
            continue;
        }
        entries.push(SwapListEntry {
            name: part.device.clone(),
            swap_type: "partition".into(),
            size_bytes: 0,
            used_bytes: 0,
            priority: part.priority,
            source: SwapSource::Fstab,
            active: false,
        });
    }

    Ok(entries)
}

fn active_swap_devices() -> Result<Vec<String>> {
    Ok(status::status()?
        .swaps
        .into_iter()
        .map(|s| s.name)
        .collect())
}

fn extract_uuid(spec: &str) -> Option<String> {
    spec.strip_prefix("UUID=").map(str::to_string)
}

fn resolve_device(spec: &str) -> Result<String> {
    if let Some(uuid) = extract_uuid(spec) {
        if let Ok(path) = run_command("blkid", &["-U", &uuid]) {
            let path = path.trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
        let by_uuid = format!("/dev/disk/by-uuid/{uuid}");
        if std::path::Path::new(&by_uuid).exists() {
            return Ok(by_uuid);
        }
        return Err(XzramError::NotFound(format!(
            "swap partition UUID={uuid} not found"
        )));
    }
    Ok(spec.to_string())
}

pub fn check_missing_swap_partitions(issues: &mut Vec<crate::doctor::DoctorIssue>) -> Result<()> {
    for part in list_swap_partitions()? {
        if !part.active {
            let resolved = std::path::Path::new(&part.device);
            if !resolved.exists() {
                issues.push(crate::doctor::DoctorIssue {
                    severity: crate::doctor::IssueSeverity::Warning,
                    code: "swap_partition_missing".into(),
                    message: format!(
                        "fstab swap entry '{}' points to a missing device",
                        part.device
                    ),
                    suggestion: Some(
                        "Remove stale fstab entry or attach the swap partition".into(),
                    ),
                    action: None,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_uuid_parses() {
        assert_eq!(extract_uuid("UUID=abc-123"), Some("abc-123".to_string()));
        assert_eq!(extract_uuid("/dev/sda2"), None);
    }
}
