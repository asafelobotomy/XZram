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

pub fn validate_zram_generator_devices(devices: &[crate::config::ZramDeviceSection]) -> Result<()> {
    for device in devices {
        let config = ZramConfig {
            device: device.name.clone(),
            zram_size: device.zram_size.clone(),
            zram_resident_limit: device.zram_resident_limit.clone(),
            compression_algorithm: device.compression_algorithm.clone(),
            swap_priority: device.swap_priority,
            fs_type: device.fs_type.clone(),
            mount_point: device.mount_point.clone(),
        };
        validate_zram_config(&config)?;
    }
    Ok(())
}

fn validate_zram_expression(value: &str, field: &str) -> Result<()> {
    if value.chars().any(|c| c == '\n' || c == '\r' || c == ']') {
        return Err(XzramError::Validation(format!(
            "{field} must not contain newlines or ']'"
        )));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " ()+*/.-_".contains(c))
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

    #[test]
    fn rejects_evil_zram_name() {
        assert!(validate_zram_device_name("zram0\n[evil]").is_err());
        assert!(validate_zram_device_name("zram0").is_ok());
        let bad = ZramConfig {
            device: "zram0".into(),
            zram_size: Some("ram\n]".into()),
            zram_resident_limit: None,
            compression_algorithm: None,
            swap_priority: None,
            fs_type: None,
            mount_point: None,
        };
        assert!(validate_zram_config(&bad).is_err());
    }
}
