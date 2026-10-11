use std::fs;
use std::path::PathBuf;

use crate::apply::ZramConfig;
use crate::backend::ZramBackendTrait;
use crate::config::{parse_zram_generator_conf, write_zram_generator_conf, ZramGeneratorConf};
use crate::detect::find_zram_generator_config;
use crate::error::{Result, XzramError};
use crate::snapshot::paths::{etc_path, ZRAM_CONF};

const FALLBACK_CONFIG_PATH: &str = "/etc/systemd/zram-generator.conf";

pub struct ZramGeneratorBackend;

impl ZramGeneratorBackend {
    fn managed_config_path() -> PathBuf {
        etc_path(ZRAM_CONF)
    }

    fn read_path(&self) -> PathBuf {
        if std::env::var_os("XZRAM_ETC_ROOT").is_some() {
            return Self::managed_config_path();
        }
        find_zram_generator_config()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(FALLBACK_CONFIG_PATH))
    }

    fn write_path(&self) -> PathBuf {
        Self::managed_config_path()
    }

    fn read_config(&self) -> Result<ZramGeneratorConf> {
        let path = self.read_path();
        if path.exists() {
            parse_zram_generator_conf(path.to_str().unwrap_or(FALLBACK_CONFIG_PATH))
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
        crate::validation::validate_zram_config(config)?;
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

        let path = self.write_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_zram_generator_conf(path.to_str().unwrap_or(FALLBACK_CONFIG_PATH), &conf)
    }

    fn disable(&self) -> Result<()> {
        let hermetic = std::env::var_os("XZRAM_ETC_ROOT").is_some();
        let mut errors: Vec<String> = Vec::new();

        // Tear down every live device; one busy device must not stop the others (or the
        // override below) from being handled.
        if !hermetic {
            for i in 0..8 {
                let name = format!("zram{i}");
                if let Err(e) = crate::apply::stop_zram_setup_unit(&name) {
                    errors.push(format!("{name}: {e}"));
                }
            }
        }

        // Empty /etc override disables zram-generator even when vendor config exists,
        // so devices that failed to stop now stay off after the next reboot.
        let path = self.write_path();
        if let Err(e) = crate::fsutil::atomic_write(&path, b"", 0o644) {
            errors.push(format!("write {}: {e}", path.display()));
        }

        if !hermetic {
            if let Err(e) = crate::apply::run_systemctl(&["daemon-reload"]) {
                errors.push(e.to_string());
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(XzramError::Command(format!(
                "zram disable incomplete: {}",
                errors.join("; ")
            )))
        }
    }

    fn apply(&self) -> Result<()> {
        if std::env::var_os("XZRAM_ETC_ROOT").is_some() {
            return Ok(());
        }
        crate::apply::run_systemctl(&["daemon-reload"])?;

        let conf = self.read_config()?;
        for device in &conf.devices {
            crate::apply::restart_zram_setup_unit(&device.name)?;
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

    #[test]
    fn managed_path_honors_etc_root() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        let path = ZramGeneratorBackend::managed_config_path();
        assert!(path.starts_with(etc.path()));
        assert!(path.ends_with(std::path::Path::new("systemd/zram-generator.conf")));
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn disable_writes_empty_override_under_etc_root() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        let conf = etc.path().join("systemd/zram-generator.conf");
        fs::create_dir_all(conf.parent().unwrap()).unwrap();
        fs::write(&conf, "[zram0]\nzram-size = ram / 2\n").unwrap();
        let result = ZramGeneratorBackend.disable();
        let content = fs::read_to_string(&conf).unwrap();
        std::env::remove_var("XZRAM_ETC_ROOT");
        result.unwrap();
        assert!(content.is_empty());
    }
}
