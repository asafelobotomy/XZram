use std::fs;
use std::path::Path;

use crate::apply::ZramConfig;
use crate::backend::ZramBackendTrait;
use crate::config::{parse_zram_generator_conf, write_zram_generator_conf, ZramGeneratorConf};
use crate::detect::find_zram_generator_config;
use crate::error::Result;

const CONFIG_PATH: &str = "/etc/systemd/zram-generator.conf";

pub struct ZramGeneratorBackend;

impl ZramGeneratorBackend {
    fn read_config(&self) -> Result<ZramGeneratorConf> {
        let path = find_zram_generator_config().unwrap_or_else(|| CONFIG_PATH.to_string());
        if Path::new(&path).exists() {
            parse_zram_generator_conf(&path)
        } else {
            Ok(ZramGeneratorConf::default())
        }
    }
}

impl crate::backend::SwapBackend for ZramGeneratorBackend {
    fn name(&self) -> &'static str {
        "systemd-zram-generator"
    }

    fn is_available(&self) -> bool {
        which::which("systemctl").is_ok()
    }
}

impl ZramBackendTrait for ZramGeneratorBackend {
    fn show(&self) -> Result<Option<ZramConfig>> {
        let conf = self.read_config()?;
        Ok(conf.devices.into_iter().next().map(|d| ZramConfig {
            device: d.name,
            zram_size: d.zram_size,
            compression_algorithm: d.compression_algorithm,
            swap_priority: d.swap_priority,
            fs_type: d.fs_type,
            mount_point: d.mount_point,
        }))
    }

    fn configure(&self, config: &ZramConfig) -> Result<()> {
        crate::apply::create_backup()?;

        let device = crate::config::ZramDeviceSection {
            name: config.device.clone(),
            zram_size: config.zram_size.clone(),
            compression_algorithm: config.compression_algorithm.clone(),
            swap_priority: config.swap_priority,
            fs_type: config.fs_type.clone(),
            mount_point: config.mount_point.clone(),
        };

        let conf = ZramGeneratorConf {
            devices: vec![device],
        };

        write_zram_generator_conf(CONFIG_PATH, &conf)
    }

    fn disable(&self) -> Result<()> {
        crate::apply::create_backup()?;
        if Path::new(CONFIG_PATH).exists() {
            fs::remove_file(CONFIG_PATH)?;
        }

        for i in 0..8 {
            let device = format!("/dev/zram{i}");
            let _ = std::process::Command::new("swapoff").arg(&device).output();
            let _ = std::process::Command::new("systemctl")
                .args(["stop", &device])
                .output();
        }

        crate::apply::run_systemctl(&["daemon-reload"])?;
        Ok(())
    }

    fn apply(&self) -> Result<()> {
        crate::apply::run_systemctl(&["daemon-reload"])?;

        let conf = self.read_config()?;
        for device in &conf.devices {
            let dev_path = format!("/dev/{}", device.name);
            if Path::new(&dev_path).exists() || conf.devices.iter().any(|d| !d.name.is_empty()) {
                let unit = format!("systemd-zram-setup@{}.service", device.name);
                let _ = crate::apply::run_systemctl(&["restart", &unit]);
            }
            let _ = crate::apply::run_systemctl(&["start", &dev_path]);
        }
        Ok(())
    }
}

mod which {
    use crate::error::XzramError;

    pub fn which(cmd: &str) -> std::result::Result<(), XzramError> {
        std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| {
                if o.status.success() {
                    Ok(())
                } else {
                    Err(XzramError::NotFound(cmd.into()))
                }
            })
            .unwrap_or(Err(XzramError::NotFound(cmd.into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SwapBackend;

    #[test]
    fn backend_name() {
        let b = ZramGeneratorBackend;
        assert_eq!(b.name(), "systemd-zram-generator");
    }
}
