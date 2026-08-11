use std::collections::HashMap;

use xzram::apply::PendingConfig;
use xzram::swapfile_btrfs;
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
    validation::validate_staged_pending(pending)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))
}

pub(crate) async fn prepare_swapfile_btrfs(
    path: &str,
    mkdir_parents: bool,
) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
    validation::validate_swapfile_prepare_path(path)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let payload = serde_json::json!({
        "path": path,
        "mkdir_parents": mkdir_parents,
    })
    .to_string();
    let lines = crate::privileged::run_helper("swapfile.prepare", &payload).await?;
    let raw = lines
        .iter()
        .rev()
        .find(|l| l.starts_with('{'))
        .ok_or_else(|| {
            zbus::fdo::Error::Failed("swapfile.prepare returned no status JSON".into())
        })?;
    let status: swapfile_btrfs::NodatacowStatus = serde_json::from_str(raw)
        .map_err(|e| zbus::fdo::Error::Failed(format!("invalid prepare status: {e}")))?;
    Ok(json_map(&status))
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
