//! The settings `xzram config` can show and change: what each key is, how a value is
//! checked, and how it merges into what is already configured.
//!
//! Zram settings go back to XZram as the whole current configuration with one field
//! changed, so a change never resets the fields it was not given (`xzram zram set` does).

use anyhow::{bail, Context};
use serde_json::{json, Value};
use xzram::apply::ZramConfig;
use xzram::config::{default_zram_config, DEFAULT_ZRAM_SIZE};
use xzram::sysctl::{validate_sysctl_values, SysctlValues};
use xzram::validation::validate_zram_config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    ZramSize,
    ZramAlgorithm,
    ZramPriority,
    Swappiness,
    PageCluster,
    VfsCachePressure,
    WatermarkScaleFactor,
    WatermarkBoostFactor,
}

const ALL: [Key; 8] = [
    Key::ZramSize,
    Key::ZramAlgorithm,
    Key::ZramPriority,
    Key::Swappiness,
    Key::PageCluster,
    Key::VfsCachePressure,
    Key::WatermarkScaleFactor,
    Key::WatermarkBoostFactor,
];

impl Key {
    pub fn parse(name: &str) -> anyhow::Result<Self> {
        ALL.into_iter().find(|k| k.name() == name).with_context(|| {
            let names: Vec<_> = ALL.iter().map(|k| k.name()).collect();
            format!("unknown setting '{name}'; one of: {}", names.join(", "))
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::ZramSize => "zram-size",
            Self::ZramAlgorithm => "zram-algorithm",
            Self::ZramPriority => "zram-priority",
            Self::Swappiness => "swappiness",
            Self::PageCluster => "page-cluster",
            Self::VfsCachePressure => "vfs-cache-pressure",
            Self::WatermarkScaleFactor => "watermark-scale-factor",
            Self::WatermarkBoostFactor => "watermark-boost-factor",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ZramSize => "Compressed memory size",
            Self::ZramAlgorithm => "Compression",
            Self::ZramPriority => "Compressed memory priority",
            Self::Swappiness => "Swappiness",
            Self::PageCluster => "Swap read-ahead",
            Self::VfsCachePressure => "File cache pressure",
            Self::WatermarkScaleFactor => "Free memory headroom",
            Self::WatermarkBoostFactor => "Memory boost",
        }
    }

    pub fn is_zram(self) -> bool {
        matches!(
            self,
            Self::ZramSize | Self::ZramAlgorithm | Self::ZramPriority
        )
    }

    /// `(min, max)` of a number setting.
    fn range(self) -> Option<(i64, i64)> {
        match self {
            Self::ZramSize | Self::ZramAlgorithm => None,
            Self::ZramPriority => Some((-1, 32767)),
            Self::Swappiness => Some((0, 200)),
            Self::PageCluster => Some((0, 8)),
            Self::VfsCachePressure => Some((1, 1000)),
            Self::WatermarkScaleFactor | Self::WatermarkBoostFactor => Some((0, 10000)),
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::ZramSize => "How much compressed memory there is, as a share of installed memory.",
            Self::ZramAlgorithm => "How memory is compressed. zstd squeezes most; lz4 is fastest.",
            Self::ZramPriority => "Which swap is used first. Higher is used first.",
            Self::Swappiness => "Higher values move memory to swap sooner. The usual choice is 60, or higher with compressed memory.",
            Self::PageCluster => "How many pages are read back from swap at once, as a power of two. 0 suits compressed memory.",
            Self::VfsCachePressure => "How eagerly the file-name caches are given back. 100 is the kernel's default.",
            Self::WatermarkScaleFactor => "How much memory the kernel keeps free, in tenths of a percent.",
            Self::WatermarkBoostFactor => "How hard the kernel reclaims after fragmentation. 0 turns it off.",
        }
    }

    /// The value now, as text, for a plan: from the zram configuration.
    pub fn show_zram(self, config: Option<&ZramConfig>) -> String {
        let value = config.and_then(|c| match self {
            Self::ZramSize => c.zram_size.clone(),
            Self::ZramAlgorithm => c.compression_algorithm.clone(),
            Self::ZramPriority => c.swap_priority.map(|p| p.to_string()),
            _ => None,
        });
        value.unwrap_or_else(|| "not set".into())
    }

    /// The value now of a tuning key.
    pub fn tuning_value(self, v: &SysctlValues) -> Option<u64> {
        match self {
            Self::Swappiness => v.swappiness,
            Self::PageCluster => v.page_cluster,
            Self::VfsCachePressure => v.vfs_cache_pressure,
            Self::WatermarkScaleFactor => v.watermark_scale_factor,
            Self::WatermarkBoostFactor => v.watermark_boost_factor,
            _ => None,
        }
        .map(u64::from)
    }

    /// The number a value says, checked against this key's range.
    pub fn number(self, value: &str) -> anyhow::Result<i64> {
        let (min, max) = self.range().context("not a number setting")?;
        let n: i64 = value
            .trim()
            .parse()
            .with_context(|| format!("{} is a whole number from {min} to {max}", self.name()))?;
        if !(min..=max).contains(&n) {
            bail!(
                "{} is a whole number from {min} to {max} (got {n})",
                self.name()
            );
        }
        Ok(n)
    }
}

/// The zram configuration after `key` is set to `value`, validated. With nothing configured
/// yet, XZram's defaults are the starting point (the same as `xzram zram set`).
pub(crate) fn merged_zram(
    current: Option<&ZramConfig>,
    key: Key,
    value: &str,
) -> anyhow::Result<ZramConfig> {
    let mut next = current.cloned().unwrap_or_else(|| {
        let d = default_zram_config();
        ZramConfig {
            device: d.name,
            zram_size: Some(DEFAULT_ZRAM_SIZE.into()),
            zram_resident_limit: None,
            compression_algorithm: d.compression_algorithm,
            swap_priority: d.swap_priority,
            fs_type: None,
            mount_point: None,
        }
    });
    match key {
        Key::ZramSize => next.zram_size = Some(value.trim().to_string()),
        Key::ZramAlgorithm => {
            let name = value.trim();
            // One token that is never taken for an option.
            let ok = !name.is_empty()
                && name.len() <= 64
                && !name.starts_with('-')
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            if !ok {
                bail!("'{name}' is not a compression name");
            }
            next.compression_algorithm = Some(name.to_string());
        }
        Key::ZramPriority => next.swap_priority = Some(i32::try_from(key.number(value)?)?),
        _ => bail!("{} is not a compressed memory setting", key.name()),
    }
    validate_zram_config(&next)?;
    Ok(next)
}

/// The sysctl values for one tuning key, validated.
pub(crate) fn tuning_values(key: Key, value: &str) -> anyhow::Result<SysctlValues> {
    let n = u32::try_from(key.number(value)?)?;
    let mut v = SysctlValues::default();
    match key {
        Key::Swappiness => v.swappiness = Some(n),
        Key::PageCluster => v.page_cluster = Some(n),
        Key::VfsCachePressure => v.vfs_cache_pressure = Some(n),
        Key::WatermarkScaleFactor => v.watermark_scale_factor = Some(n),
        Key::WatermarkBoostFactor => v.watermark_boost_factor = Some(n),
        _ => bail!("{} is not a tuning setting", key.name()),
    }
    validate_sysctl_values(&v)?;
    Ok(v)
}

/// What `xzram config show --json` prints.
pub(crate) fn show_document(
    zram: Option<&ZramConfig>,
    tuning: &SysctlValues,
    pending: bool,
    snapshots: usize,
) -> Value {
    let keys: Vec<Value> = ALL
        .iter()
        .map(|k| {
            let mut row = json!({ "key": k.name(), "label": k.label(), "help": k.help(),
                "kind": if k.range().is_some() { "number" } else { "text" } });
            if let Some((min, max)) = k.range() {
                row["min"] = json!(min);
                row["max"] = json!(max);
            }
            row
        })
        .collect();
    json!({
        "schema_version": crate::plan::SCHEMA_VERSION,
        "zram": {
            "configured": zram.is_some(),
            "device": zram.map(|c| c.device.clone()),
            "size": zram.and_then(|c| c.zram_size.clone()),
            "algorithm": zram.and_then(|c| c.compression_algorithm.clone()),
            "priority": zram.and_then(|c| c.swap_priority),
        },
        "tuning": {
            "swappiness": tuning.swappiness,
            "page_cluster": tuning.page_cluster,
            "vfs_cache_pressure": tuning.vfs_cache_pressure,
            "watermark_scale_factor": tuning.watermark_scale_factor,
            "watermark_boost_factor": tuning.watermark_boost_factor,
        },
        "pending": pending,
        "snapshots": snapshots,
        "keys": keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ZramConfig {
        ZramConfig {
            device: "zram0".into(),
            zram_size: Some("ram / 2".into()),
            zram_resident_limit: Some("ram / 4".into()),
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: Some("swap".into()),
            mount_point: None,
        }
    }

    #[test]
    fn unknown_keys_list_the_known_ones() {
        let e = Key::parse("nope").unwrap_err().to_string();
        assert!(e.contains("zram-size") && e.contains("swappiness"));
    }

    #[test]
    fn a_change_keeps_every_other_field() {
        let next = merged_zram(Some(&config()), Key::ZramAlgorithm, "lz4").unwrap();
        assert_eq!(next.compression_algorithm.as_deref(), Some("lz4"));
        assert_eq!(next.zram_size.as_deref(), Some("ram / 2"));
        assert_eq!(next.zram_resident_limit.as_deref(), Some("ram / 4"));
        assert_eq!(next.fs_type.as_deref(), Some("swap"));
        assert_eq!(next.swap_priority, Some(100));
    }

    #[test]
    fn nothing_configured_starts_from_the_defaults() {
        let next = merged_zram(None, Key::ZramAlgorithm, "lz4").unwrap();
        assert_eq!(next.device, "zram0");
        assert_eq!(next.zram_size.as_deref(), Some(DEFAULT_ZRAM_SIZE));
    }

    #[test]
    fn bad_values_are_refused_before_anything_is_sent() {
        assert!(merged_zram(Some(&config()), Key::ZramSize, "ram]\nx=1").is_err());
        assert!(merged_zram(Some(&config()), Key::ZramAlgorithm, "-evil").is_err());
        assert!(merged_zram(Some(&config()), Key::ZramPriority, "99999").is_err());
        assert!(tuning_values(Key::Swappiness, "201").is_err());
        assert!(tuning_values(Key::Swappiness, "abc").is_err());
        assert!(tuning_values(Key::VfsCachePressure, "0").is_err());
    }

    #[test]
    fn tuning_sets_only_its_own_key() {
        let v = tuning_values(Key::Swappiness, " 150 ").unwrap();
        assert_eq!(v.swappiness, Some(150));
        assert_eq!(v.page_cluster, None);
    }

    #[test]
    fn show_document_lists_every_key_with_its_range() {
        let doc = show_document(None, &SysctlValues::default(), false, 0);
        assert_eq!(doc["zram"]["configured"], false);
        let keys = doc["keys"].as_array().unwrap();
        assert_eq!(keys.len(), ALL.len());
        let swap = keys.iter().find(|k| k["key"] == "swappiness").unwrap();
        assert_eq!(
            (swap["min"].as_i64(), swap["max"].as_i64()),
            (Some(0), Some(200))
        );
    }
}
