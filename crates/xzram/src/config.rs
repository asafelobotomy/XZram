use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZramGeneratorConf {
    pub devices: Vec<ZramDeviceSection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZramDeviceSection {
    pub name: String,
    pub zram_size: Option<String>,
    pub zram_resident_limit: Option<String>,
    pub compression_algorithm: Option<String>,
    pub swap_priority: Option<i32>,
    pub fs_type: Option<String>,
    pub mount_point: Option<String>,
    /// Keys xzram doesn't model (`writeback-device`, `options`, `host-memory-limit`, ...),
    /// and modeled keys whose value didn't parse, in file order. Written back verbatim so a
    /// rewrite never drops settings the user or the distro put there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<(String, String)>,
}

impl ZramDeviceSection {
    /// Replace the modeled fields with `new`'s, keeping this section's `extra` keys
    /// (minus any `new` now models with a value).
    pub fn merged_with(&self, new: ZramDeviceSection) -> ZramDeviceSection {
        let mut out = new;
        out.extra = self
            .extra
            .iter()
            .filter(|(k, _)| !out.models(k))
            .cloned()
            .collect();
        out
    }

    /// Whether `key` is a field this section writes itself (so an `extra` copy is stale).
    fn models(&self, key: &str) -> bool {
        match key {
            "zram-size" => self.zram_size.is_some(),
            "zram-resident-limit" => self.zram_resident_limit.is_some(),
            "compression-algorithm" => self.compression_algorithm.is_some(),
            "swap-priority" => self.swap_priority.is_some(),
            "fs-type" => self.fs_type.is_some(),
            "mount-point" => self.mount_point.is_some(),
            _ => false,
        }
    }
}

pub fn parse_zram_generator_conf(path: impl AsRef<Path>) -> Result<ZramGeneratorConf> {
    Ok(parse_zram_generator_str(&fs::read_to_string(path)?))
}

/// Parse zram-generator INI text. Comments and blank lines are skipped; every other
/// `key = value` line inside a section is kept, either in a modeled field or in `extra`.
pub fn parse_zram_generator_str(content: &str) -> ZramGeneratorConf {
    let mut conf = ZramGeneratorConf::default();
    let mut current: Option<ZramDeviceSection> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(device) = current.take() {
                conf.devices.push(device);
            }
            current = Some(ZramDeviceSection {
                name: line.trim_matches(['[', ']']).to_string(),
                ..Default::default()
            });
        } else if let Some((key, value)) = line.split_once('=') {
            let Some(ref mut device) = current else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().to_string();
            let slot = match key {
                "zram-size" => &mut device.zram_size,
                "zram-resident-limit" => &mut device.zram_resident_limit,
                "compression-algorithm" => &mut device.compression_algorithm,
                "fs-type" => &mut device.fs_type,
                "mount-point" => &mut device.mount_point,
                "swap-priority" => {
                    match value.parse() {
                        Ok(p) => device.swap_priority = Some(p),
                        // Keep a value we can't read (e.g. a typo) rather than dropping it.
                        Err(_) => device.extra.push((key.to_string(), value)),
                    }
                    continue;
                }
                _ => {
                    device.extra.push((key.to_string(), value));
                    continue;
                }
            };
            *slot = Some(value);
        }
    }

    if let Some(device) = current {
        conf.devices.push(device);
    }
    conf
}

/// Render the config. Unmodeled keys follow the modeled ones in each section.
pub fn render_zram_generator_conf(conf: &ZramGeneratorConf) -> String {
    let mut content = String::from("# Managed by xzram\n");
    for device in &conf.devices {
        content.push_str(&format!("[{}]\n", device.name));
        let mut put = |key: &str, value: Option<String>| {
            if let Some(v) = value {
                content.push_str(&format!("{key} = {v}\n"));
            }
        };
        put("zram-size", device.zram_size.clone());
        put("zram-resident-limit", device.zram_resident_limit.clone());
        put(
            "compression-algorithm",
            device.compression_algorithm.clone(),
        );
        put("swap-priority", device.swap_priority.map(|p| p.to_string()));
        put("fs-type", device.fs_type.clone());
        put("mount-point", device.mount_point.clone());
        for (key, value) in &device.extra {
            put(key, Some(value.clone()));
        }
        content.push('\n');
    }
    content
}

/// Validate and atomically replace `path` (a crash leaves the old file or the new one;
/// a truncated or empty file here would turn zram off at the next boot).
pub fn write_zram_generator_conf(path: &Path, conf: &ZramGeneratorConf) -> Result<()> {
    crate::validation::validate_zram_generator_devices(&conf.devices)?;
    let content = render_zram_generator_conf(conf);
    crate::fsutil::atomic_write(path, content.as_bytes(), 0o644)
}

pub fn default_zram_config() -> ZramDeviceSection {
    ZramDeviceSection {
        name: "zram0".into(),
        zram_size: Some("min(ram / 2, 4096)".into()),
        compression_algorithm: Some("zstd".into()),
        swap_priority: Some(100),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_roundtrip() {
        let input = r#"
[zram0]
zram-size = min(ram / 2, 4096)
zram-resident-limit = ram / 2
compression-algorithm = zstd
swap-priority = 100
"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zram-generator.conf");
        fs::write(&path, input).unwrap();

        let conf = parse_zram_generator_conf(&path).unwrap();
        assert_eq!(conf.devices.len(), 1);
        assert_eq!(conf.devices[0].name, "zram0");
        assert_eq!(
            conf.devices[0].compression_algorithm.as_deref(),
            Some("zstd")
        );
        assert_eq!(
            conf.devices[0].zram_resident_limit.as_deref(),
            Some("ram / 2")
        );
        assert_eq!(conf.devices[0].swap_priority, Some(100));
        assert!(conf.devices[0].extra.is_empty());
    }

    #[test]
    fn unknown_keys_and_other_sections_survive_a_rewrite() {
        let input = "[zram0]\nzram-size = ram\nwriteback-device = /dev/sdb2\n\
                     options = discard\nswap-priority = high\n\n\
                     [zram1]\nzram-size = 512\nfs-type = ext4\nmount-point = /var/tmp\n";
        let mut conf = parse_zram_generator_str(input);
        assert_eq!(conf.devices[0].swap_priority, None);
        // Change zram0 the way `configure` does.
        let new = ZramDeviceSection {
            name: "zram0".into(),
            zram_size: Some("min(ram, 8192)".into()),
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            ..Default::default()
        };
        conf.devices[0] = conf.devices[0].merged_with(new);
        let out = render_zram_generator_conf(&conf);
        assert!(out.contains("zram-size = min(ram, 8192)\n"), "{out}");
        assert!(out.contains("writeback-device = /dev/sdb2\n"), "{out}");
        assert!(out.contains("options = discard\n"), "{out}");
        // The unreadable priority is replaced, not written twice.
        assert_eq!(out.matches("swap-priority").count(), 1, "{out}");
        assert!(out.contains("swap-priority = 100\n"), "{out}");
        assert!(out.contains("[zram1]\nzram-size = 512\nfs-type = ext4\nmount-point = /var/tmp\n"));
        // And it parses back to the same thing.
        let again = parse_zram_generator_str(&out);
        assert_eq!(again.devices.len(), 2);
        assert_eq!(again.devices[0].extra.len(), 2);
    }

    #[test]
    fn write_is_atomic_and_keeps_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zram-generator.conf");
        let conf = ZramGeneratorConf {
            devices: vec![default_zram_config()],
        };
        write_zram_generator_conf(&path, &conf).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temp file left"
        );
    }

    #[test]
    fn default_zram_config_values() {
        let d = default_zram_config();
        assert_eq!(d.name, "zram0");
        assert_eq!(d.swap_priority, Some(100));
    }
}
