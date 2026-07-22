use std::collections::HashMap;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_map_contains_json_key() {
        let map = json_map(&serde_json::json!({"ok": true}));
        assert!(map.contains_key("json"));
    }
}
