use crate::apply::ZramConfig;
use crate::error::{Result, XzramError};

pub fn validate_zram_config(config: &ZramConfig) -> Result<()> {
    validate_zram_device_name(&config.device)?;
    if let Some(ref s) = config.zram_size {
        validate_zram_expression(s, "zram-size")?;
    }
    if let Some(ref s) = config.zram_resident_limit {
        validate_zram_expression(s, "zram-resident-limit")?;
    }
    if let Some(ref s) = config.compression_algorithm {
        validate_zram_token(s, "compression-algorithm")?;
    }
    if let Some(ref s) = config.fs_type {
        validate_zram_token(s, "fs-type")?;
    }
    if let Some(ref s) = config.mount_point {
        validate_zram_mount_point(s)?;
    }
    Ok(())
}

pub fn validate_zram_device_name(name: &str) -> Result<()> {
    if name.len() <= 4
        || !name.starts_with("zram")
        || !name.as_bytes()[4..].iter().all(u8::is_ascii_digit)
    {
        return Err(XzramError::Validation(format!(
            "zram device name must match zramN (got {name})"
        )));
    }
    Ok(())
}

/// Write-back safety for a whole generator file. The device being changed is validated
/// strictly (`validate_zram_config`) by `configure`; the other sections and unmodeled keys
/// come from the existing file, so here they only have to be safe to render as INI: a
/// `zramN` section name, `[a-z0-9-]` keys, and values without control characters or a
/// leading `[`. (Strict checks here made one value xzram doesn't model, such as
/// `zstd(level=3)`, block every change.)
pub fn validate_zram_generator_devices(devices: &[crate::config::ZramDeviceSection]) -> Result<()> {
    for device in devices {
        validate_zram_device_name(&device.name)?;
        let modeled = [
            ("zram-size", device.zram_size.as_deref()),
            ("zram-resident-limit", device.zram_resident_limit.as_deref()),
            (
                "compression-algorithm",
                device.compression_algorithm.as_deref(),
            ),
            ("fs-type", device.fs_type.as_deref()),
            ("mount-point", device.mount_point.as_deref()),
        ];
        let extra = device
            .extra
            .iter()
            .map(|(k, v)| (k.as_str(), Some(v.as_str())));
        for (key, value) in modeled.into_iter().chain(extra) {
            let key_ok = !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            let value_ok =
                value.is_none_or(|v| !v.starts_with('[') && !v.chars().any(|c| c.is_control()));
            if !key_ok || !value_ok {
                return Err(XzramError::Validation(format!(
                    "[{}] {key}: not safe to write to zram-generator.conf",
                    device.name
                )));
            }
        }
    }
    Ok(())
}

fn validate_zram_expression(value: &str, field: &str) -> Result<()> {
    if value.chars().any(|c| c == '\n' || c == '\r' || c == ']') {
        return Err(XzramError::Validation(format!(
            "{field} must not contain newlines or ']'"
        )));
    }
    // zram-generator expressions: arithmetic (^%/*-+), min()/max(), commas, spaces.
    // Keep ASCII-only so INI values cannot smuggle section/control characters.
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " ()+*/.-_,^%".contains(c))
    {
        return Err(XzramError::Validation(format!(
            "{field} contains invalid characters"
        )));
    }
    Ok(())
}

fn validate_zram_token(value: &str, field: &str) -> Result<()> {
    if value.chars().any(|c| c == '\n' || c == '\r' || c == ']') {
        return Err(XzramError::Validation(format!(
            "{field} must not contain newlines or ']'"
        )));
    }
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(XzramError::Validation(format!(
            "{field} contains invalid characters"
        )));
    }
    Ok(())
}

fn validate_zram_mount_point(value: &str) -> Result<()> {
    if value.chars().any(|c| c == '\n' || c == '\r' || c == ']') {
        return Err(XzramError::Validation(
            "mount-point must not contain newlines or ']'".into(),
        ));
    }
    if !value.starts_with('/') {
        return Err(XzramError::Validation(
            "mount-point must be an absolute path".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_size(size: &str) -> ZramConfig {
        ZramConfig {
            device: "zram0".into(),
            zram_size: Some(size.into()),
            zram_resident_limit: None,
            compression_algorithm: None,
            swap_priority: None,
            fs_type: None,
            mount_point: None,
        }
    }

    #[test]
    fn rejects_evil_zram_name() {
        assert!(validate_zram_device_name("zram0\n[evil]").is_err());
        assert!(validate_zram_device_name("zram0").is_ok());
        assert!(validate_zram_config(&config_with_size("ram\n]")).is_err());
    }

    #[test]
    fn accepts_recommended_zram_size_formulas() {
        for size in [
            "ram",
            "ram / 2",
            "min(ram / 2, 4096)",
            "min(ram, 8192)",
            "min(min(ram, 4096) + max(ram - 4096, 0) / 2, 32 * 1024)",
            "ram % 100",
            "2^10",
        ] {
            assert!(
                validate_zram_config(&config_with_size(size)).is_ok(),
                "expected ok for {size}"
            );
        }
    }

    #[test]
    fn rejects_non_ascii_and_brackets_in_expression() {
        assert!(validate_zram_config(&config_with_size("min(ram, 4π)")).is_err());
        assert!(validate_zram_config(&config_with_size("ram]")).is_err());
    }
}
