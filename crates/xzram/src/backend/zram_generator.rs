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
            zram_resident_limit: d.zram_resident_limit,
            compression_algorithm: d.compression_algorithm,
            swap_priority: d.swap_priority,
            fs_type: d.fs_type,
            mount_point: d.mount_point,
        }))
    }

    fn configure(&self, config: &ZramConfig) -> Result<()> {
        let device = crate::config::ZramDeviceSection {
            name: config.device.clone(),
            zram_size: config.zram_size.clone(),
            zram_resident_limit: config.zram_resident_limit.clone(),
            compression_algorithm: config.compression_algorithm.clone(),
            swap_priority: config.swap_priority,
            fs_type: config.fs_type.clone(),
            mount_point: config.mount_point.clone(),
        };

        let mut conf = self.read_config().unwrap_or_default();
        if let Some(idx) = conf.devices.iter().position(|d| d.name == device.name) {
            conf.devices[idx] = device;
        } else {
            conf.devices.push(device);
        }

        write_zram_generator_conf(CONFIG_PATH, &conf)
    }

    fn disable(&self) -> Result<()> {
        // Empty /etc override disables zram-generator even when vendor config exists.
        if let Some(parent) = Path::new(CONFIG_PATH).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(CONFIG_PATH, "")?;

        for i in 0..8 {
            let device = format!("/dev/zram{i}");
            let _ = std::process::Command::new("swapoff").arg(&device).output();
            let _ = std::process::Command::new("systemctl")
                .args(["stop", &format!("systemd-zram-setup@zram{i}.service")])
                .output();
        }

        crate::apply::run_systemctl(&["daemon-reload"])?;
        Ok(())
    }

    fn apply(&self) -> Result<()> {
        crate::apply::run_systemctl(&["daemon-reload"])?;

        let conf = self.read_config()?;
        for device in &conf.devices {
            let unit = format!("systemd-zram-setup@{}.service", device.name);
            crate::apply::run_systemctl(&["try-restart", &unit])?;
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
