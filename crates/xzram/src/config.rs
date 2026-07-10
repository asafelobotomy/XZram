use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZramGeneratorConf {
    pub devices: Vec<ZramDeviceSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramDeviceSection {
    pub name: String,
    pub zram_size: Option<String>,
    pub compression_algorithm: Option<String>,
    pub swap_priority: Option<i32>,
    pub fs_type: Option<String>,
    pub mount_point: Option<String>,
}

pub fn parse_zram_generator_conf(path: &str) -> Result<ZramGeneratorConf> {
    let content = fs::read_to_string(path)?;
    let mut conf = ZramGeneratorConf::default();
    let mut current: Option<ZramDeviceSection> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(device) = current.take() {
                conf.devices.push(device);
            }
            let name = line.trim_matches(['[', ']']).to_string();
            current = Some(ZramDeviceSection {
                name,
                zram_size: None,
                compression_algorithm: None,
                swap_priority: None,
                fs_type: None,
                mount_point: None,
            });
        } else if let Some((key, value)) = line.split_once('=') {
            if let Some(ref mut device) = current {
                let key = key.trim();
                let value = value.trim().to_string();
                match key {
                    "zram-size" => device.zram_size = Some(value),
                    "compression-algorithm" => device.compression_algorithm = Some(value),
                    "swap-priority" => device.swap_priority = value.parse().ok(),
                    "fs-type" => device.fs_type = Some(value),
                    "mount-point" => device.mount_point = Some(value),
                    _ => {}
                }
            }
        }
    }

    if let Some(device) = current {
        conf.devices.push(device);
    }

    Ok(conf)
}

pub fn write_zram_generator_conf(path: &str, conf: &ZramGeneratorConf) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }

    let mut content = String::from("# Managed by xzram\n");
    for device in &conf.devices {
        content.push_str(&format!("[{}]\n", device.name));
        if let Some(ref size) = device.zram_size {
            content.push_str(&format!("zram-size = {size}\n"));
        }
        if let Some(ref algo) = device.compression_algorithm {
            content.push_str(&format!("compression-algorithm = {algo}\n"));
        }
        if let Some(pri) = device.swap_priority {
            content.push_str(&format!("swap-priority = {pri}\n"));
        }
        if let Some(ref fs) = device.fs_type {
            content.push_str(&format!("fs-type = {fs}\n"));
        }
        if let Some(ref mp) = device.mount_point {
            content.push_str(&format!("mount-point = {mp}\n"));
        }
        content.push('\n');
    }

    fs::write(path, content)?;
    Ok(())
}

pub fn default_zram_config() -> ZramDeviceSection {
    ZramDeviceSection {
        name: "zram0".into(),
        zram_size: Some("min(ram / 2, 4096)".into()),
        compression_algorithm: Some("zstd".into()),
        swap_priority: Some(100),
        fs_type: None,
        mount_point: None,
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
compression-algorithm = zstd
swap-priority = 100
"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zram-generator.conf");
        fs::write(&path, input).unwrap();

        let conf = parse_zram_generator_conf(path.to_str().unwrap()).unwrap();
        assert_eq!(conf.devices.len(), 1);
        assert_eq!(conf.devices[0].name, "zram0");
        assert_eq!(
            conf.devices[0].compression_algorithm.as_deref(),
            Some("zstd")
        );
        assert_eq!(conf.devices[0].swap_priority, Some(100));
    }
}
