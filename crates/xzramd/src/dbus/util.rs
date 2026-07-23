use std::collections::HashMap;

use xzram::apply::PendingConfig;
use xzram::validation;

pub(crate) fn json_map<T: serde::Serialize>(
    value: &T,
) -> HashMap<String, zbus::zvariant::OwnedValue> {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    let mut map = HashMap::new();
    let owned: zbus::zvariant::OwnedValue = zbus::zvariant::Value::from(json)
        .try_into()
        .expect("json string is a valid D-Bus value");
    map.insert("json".into(), owned);
    map
}

pub(crate) fn validate_staged_pending(pending: &PendingConfig) -> zbus::fdo::Result<()> {
    if let Some(ref swapfile) = pending.swapfile {
        validation::validate_swapfile_config(swapfile)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    }
    if let Some(ref path) = pending.remove_swapfile {
        validation::validate_swapfile_path(path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    }
    if let Some(ref resize) = pending.swapfile_resize {
        validation::validate_swapfile_path(&resize.path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        if resize.size_mb == 0 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "swapfile_resize size_mb must be greater than 0".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_map_contains_json_key() {
        let map = json_map(&serde_json::json!({"ok": true}));
        assert!(map.contains_key("json"));
    }
}
