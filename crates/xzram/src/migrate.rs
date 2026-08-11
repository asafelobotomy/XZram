use crate::apply::{self, PendingConfig, ZramConfig};
use crate::config::default_zram_config;
use crate::error::{Result, XzramError};
use crate::snapshot;

pub const ZRAMSWAP_PATH: &str = "/etc/default/zramswap";
const ZRAMSWAP_ARCHIVE: &str = "/etc/default/zramswap.xzram.bak";

pub fn zramswap_path() -> std::path::PathBuf {
    snapshot::etc_root().join("default/zramswap")
}

pub fn zramswap_config_exists() -> bool {
    zramswap_path().exists()
}

/// Parse ALGO/PERCENT/SIZE/PRIORITY from a zram-tools `/etc/default/zramswap` body.
fn parse_zramswap(content: &str) -> (String, Option<u32>, Option<u32>, Option<i32>) {
    let mut algo = default_zram_config()
        .compression_algorithm
        .unwrap_or_else(|| "zstd".into());
    let mut percent: Option<u32> = None;
    let mut size_mib: Option<u32> = None;
    let mut priority: Option<i32> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "ALGO" => algo = value.to_string(),
                "PERCENT" => {
                    if let Ok(p) = value.parse() {
                        percent = Some(p);
                    }
                }
                "SIZE" => {
                    if let Ok(s) = value.parse() {
                        size_mib = Some(s);
                    }
                }
                "PRIORITY" => {
                    if let Ok(p) = value.parse() {
                        priority = Some(p);
                    }
                }
                _ => {}
            }
        }
    }
    (algo, percent, size_mib, priority)
}

fn zram_size_expression(percent: Option<u32>, size_mib: Option<u32>) -> String {
    // Prefer PERCENT when both are set (matches zram-tools precedence on many distros).
    if let Some(p) = percent {
        format!("ram / 100 * {p}")
    } else if let Some(s) = size_mib {
        format!("{s}")
    } else {
        "ram / 100 * 50".into()
    }
}

pub fn migrate_from_zram_tools() -> Result<PendingConfig> {
    if !zramswap_config_exists() {
        return Err(XzramError::NotFound(
            "zram-tools config not found at /etc/default/zramswap".into(),
        ));
    }

    let content = std::fs::read_to_string(zramswap_path())?;
    let (algo, percent, size_mib, priority) = parse_zramswap(&content);
    let defaults = default_zram_config();
    let zram_size = zram_size_expression(percent, size_mib);

    Ok(PendingConfig {
        zram: Some(ZramConfig {
            device: defaults.name,
            zram_size: Some(zram_size),
            zram_resident_limit: None,
            compression_algorithm: Some(algo),
            swap_priority: priority.or(defaults.swap_priority),
            fs_type: None,
            mount_point: None,
        }),
        finalize_zram_tools: true,
        ..Default::default()
    })
}

/// Disable legacy zram-tools service and archive its config after migration apply.
pub fn finalize_zram_tools_migration() -> Result<Vec<String>> {
    if !zramswap_config_exists() {
        return Ok(vec![]);
    }

    let mut messages = Vec::new();

    match apply::run_systemctl(&["disable", "--now", "zramswap.service"]) {
        Ok(()) => messages.push("Disabled zramswap.service".into()),
        Err(e) => {
            return Err(XzramError::Command(format!(
                "failed to disable zramswap.service before archiving config: {e}"
            )));
        }
    }

    let archive = snapshot::etc_root().join("default/zramswap.xzram.bak");
    std::fs::rename(zramswap_path(), &archive)?;
    messages.push(format!(
        "Archived {} to {}",
        ZRAMSWAP_PATH, ZRAMSWAP_ARCHIVE
    ));

    Ok(messages)
}

pub fn zramswap_service_active() -> bool {
    apply::run_systemctl(&["is-active", "--quiet", "zramswap.service"]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_parses_percent_and_algo() {
        let (algo, percent, size, priority) = parse_zramswap("# comment\nALGO=lz4\nPERCENT=25\n");
        assert_eq!(algo, "lz4");
        assert_eq!(percent, Some(25));
        assert_eq!(size, None);
        assert_eq!(priority, None);
        assert_eq!(zram_size_expression(percent, size), "ram / 100 * 25");
    }

    #[test]
    fn migrate_prefers_size_when_percent_absent() {
        let (algo, percent, size, priority) = parse_zramswap("ALGO=zstd\nSIZE=512\nPRIORITY=100\n");
        assert_eq!(algo, "zstd");
        assert_eq!(percent, None);
        assert_eq!(size, Some(512));
        assert_eq!(priority, Some(100));
        assert_eq!(zram_size_expression(percent, size), "512");
    }

    #[test]
    fn migrate_percent_wins_over_size() {
        assert_eq!(zram_size_expression(Some(40), Some(512)), "ram / 100 * 40");
    }
}
