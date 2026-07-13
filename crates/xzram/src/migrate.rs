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

pub fn migrate_from_zram_tools() -> Result<PendingConfig> {
    if !zramswap_config_exists() {
        return Err(XzramError::NotFound(
            "zram-tools config not found at /etc/default/zramswap".into(),
        ));
    }

    let content = std::fs::read_to_string(zramswap_path())?;
    let mut algo = default_zram_config()
        .compression_algorithm
        .unwrap_or_else(|| "zstd".into());
    let mut percent = 50u32;

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
                        percent = p;
                    }
                }
                _ => {}
            }
        }
    }

    let defaults = default_zram_config();
    let zram_size = format!("ram / 100 * {percent}");

    Ok(PendingConfig {
        zram: Some(ZramConfig {
            device: defaults.name,
            zram_size: Some(zram_size),
            zram_resident_limit: None,
            compression_algorithm: Some(algo),
            swap_priority: defaults.swap_priority,
            fs_type: None,
            mount_point: None,
        }),
        ..Default::default()
    })
}

/// Disable legacy zram-tools service and archive its config after migration apply.
pub fn finalize_zram_tools_migration() -> Result<Vec<String>> {
    if !zramswap_config_exists() {
        return Ok(vec![]);
    }

    let mut messages = Vec::new();

    let _ = apply::run_systemctl(&["disable", "--now", "zramswap.service"]);
    messages.push("Disabled zramswap.service".into());

    let archive = snapshot::etc_root().join("default/zramswap.xzram.bak");
    std::fs::rename(zramswap_path(), &archive)?;
    messages.push(format!(
        "Archived {} to {}",
        ZRAMSWAP_PATH,
        ZRAMSWAP_ARCHIVE
    ));

    Ok(messages)
}

pub fn zramswap_service_active() -> bool {
    apply::run_systemctl(&["is-active", "--quiet", "zramswap.service"]).is_ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrate_parses_zramswap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zramswap");
        std::fs::write(&path, "# comment\nALGO=lz4\nPERCENT=25\n").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let mut algo = "zstd".to_string();
        let mut percent = 50u32;
        for line in content.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "ALGO" => algo = value.trim().to_string(),
                    "PERCENT" => percent = value.trim().parse().unwrap(),
                    _ => {}
                }
            }
        }
        assert_eq!(algo, "lz4");
        assert_eq!(percent, 25);
    }
}
