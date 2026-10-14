//! Strict JSON parsing for privileged payloads.
//!
//! serde ignores unknown fields by default, so a newer CLI talking to an older helper or
//! daemon had its new settings silently dropped while apply still reported success. These
//! parsers deserialize, serialize the result back, and reject any non-null input key that
//! did not survive the round trip (recursing into nested objects), naming each one.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::{Result, XzramError};

pub fn from_json_strict<T: DeserializeOwned + Serialize>(payload: &str) -> Result<T> {
    let value: Value = serde_json::from_str(payload).map_err(parse_err)?;
    from_value_strict(&value)
}

pub fn from_value_strict<T: DeserializeOwned + Serialize>(value: &Value) -> Result<T> {
    let parsed: T = serde_json::from_value(value.clone()).map_err(parse_err)?;
    let known = serde_json::to_value(&parsed).map_err(parse_err)?;
    let mut unknown = Vec::new();
    unknown_paths(value, &known, "", &mut unknown);
    if unknown.is_empty() {
        return Ok(parsed);
    }
    Err(XzramError::Validation(format!(
        "unsupported field(s) {} for xzram {}; the privileged helper/daemon is older than \
         the client. Upgrade the xzram package so both match.",
        unknown.join(", "),
        env!("CARGO_PKG_VERSION")
    )))
}

fn parse_err(e: serde_json::Error) -> XzramError {
    XzramError::Parse(e.to_string())
}

fn unknown_paths(input: &Value, known: &Value, prefix: &str, out: &mut Vec<String>) {
    let Value::Object(fields) = input else {
        return;
    };
    for (key, value) in fields.iter().filter(|(_, v)| !v.is_null()) {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match known.get(key) {
            None => out.push(path),
            Some(k) if value.is_object() && k.is_object() => unknown_paths(value, k, &path, out),
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::PendingConfig;
    use crate::sysctl::SysctlValues;

    #[test]
    fn accepts_every_field_this_build_serializes() {
        let full = PendingConfig {
            enable_oomd: true,
            disable_zswap: true,
            sysctl: Some(crate::sysctl::recommended_defaults()),
            mm: Some(crate::mm_tunables::MmTunables {
                mglru_min_ttl_ms: Some(1000),
                thp_defrag: Some("defer+madvise".into()),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&full).unwrap();
        let back: PendingConfig = from_json_strict(&json).unwrap();
        assert!(back.enable_oomd);
        // Explicit nulls are fine.
        let mut with_nulls = base();
        with_nulls["zram"] = Value::Null;
        with_nulls["mm"] = Value::Null;
        assert!(from_value_strict::<PendingConfig>(&with_nulls).is_ok());
    }

    /// A minimal valid PendingConfig as JSON (required fields included).
    fn base() -> Value {
        serde_json::to_value(PendingConfig::default()).unwrap()
    }

    #[test]
    fn rejects_unknown_top_level_and_nested_fields() {
        let mut top = base();
        top["enable_turbo"] = Value::Bool(true);
        let err = from_value_strict::<PendingConfig>(&top)
            .unwrap_err()
            .to_string();
        assert!(err.contains("enable_turbo"), "{err}");

        let mut nested = base();
        nested["sysctl"] = serde_json::json!({"swappiness": 180, "vm_future_knob": 1});
        let err = from_value_strict::<PendingConfig>(&nested)
            .unwrap_err()
            .to_string();
        assert!(err.contains("sysctl.vm_future_knob"), "{err}");

        assert!(from_json_strict::<SysctlValues>(r#"{"swappiness":180,"nope":1}"#).is_err());
    }
}
