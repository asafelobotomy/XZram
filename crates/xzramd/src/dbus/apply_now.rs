//! Immediate apply helpers for the Manager D-Bus interface.
//!
//! Every method parses and validates the caller's JSON in the daemon, then forwards a
//! payload re-serialized from the validated struct — never the caller's raw string — so
//! the helper only ever sees fields the daemon understood and checked.

use xzram::apply::{SwapfileConfig, SwapfileResizeConfig, ZramConfig};
use xzram::sysctl::{self, SysctlValues};
use xzram::validation;
use zbus::message::Header;

use super::auth::authorize;
use super::Manager;

fn invalid(e: impl ToString) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(e.to_string())
}

fn to_payload<T: serde::Serialize>(value: &T) -> zbus::fdo::Result<String> {
    serde_json::to_string(value).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
}

pub(super) async fn apply_now_zram(
    mgr: &Manager,
    hdr: Header<'_>,
    config_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.zram.configure").await?;
    let config: ZramConfig = serde_json::from_str(config_json).map_err(invalid)?;
    validation::validate_zram_config(&config).map_err(invalid)?;
    let payload = to_payload(&config)?;
    crate::privileged::run_helper(mgr.lock_gate().await, "zram.configure", &payload).await?;
    Ok(())
}

pub(super) async fn apply_now_zram_disable(
    mgr: &Manager,
    hdr: Header<'_>,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.zram.disable").await?;
    crate::privileged::run_helper(mgr.lock_gate().await, "zram.disable", "{}").await?;
    Ok(())
}

/// Build the helper payload for `swapfile.create` from the caller's JSON.
///
/// Accepts the optional `prepare` / `mkdir_parents` booleans the CLI sends alongside the
/// `SwapfileConfig` fields; everything else is dropped.
fn swapfile_create_payload(config_json: &str) -> zbus::fdo::Result<String> {
    let value: serde_json::Value = serde_json::from_str(config_json).map_err(invalid)?;
    let config: SwapfileConfig = serde_json::from_value(value.clone()).map_err(invalid)?;
    let config = validation::validate_swapfile_config(&config).map_err(invalid)?;
    let flag = |key: &str| -> zbus::fdo::Result<bool> {
        match value.get(key) {
            None | Some(serde_json::Value::Null) => Ok(false),
            Some(serde_json::Value::Bool(b)) => Ok(*b),
            Some(_) => Err(invalid(format!("'{key}' must be a boolean"))),
        }
    };
    let prepare = flag("prepare")?;
    let mkdir_parents = flag("mkdir_parents")?;
    let mut out = serde_json::to_value(&config).map_err(invalid)?;
    if prepare {
        validation::validate_swapfile_prepare_path(&config.path).map_err(invalid)?;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("prepare".into(), true.into());
            obj.insert("mkdir_parents".into(), mkdir_parents.into());
        }
    }
    to_payload(&out)
}

pub(super) async fn apply_now_swapfile_create(
    mgr: &Manager,
    hdr: Header<'_>,
    config_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.swapfile.create").await?;
    let payload = swapfile_create_payload(config_json)?;
    crate::privileged::run_helper(mgr.lock_gate().await, "swapfile.create", &payload).await?;
    Ok(())
}

pub(super) async fn apply_now_swapfile_resize(
    mgr: &Manager,
    hdr: Header<'_>,
    path: &str,
    size_mb: u64,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.swapfile.resize").await?;
    validation::validate_swapfile_resize_path(path, size_mb).map_err(invalid)?;
    let payload = to_payload(&SwapfileResizeConfig {
        path: path.into(),
        size_mb,
    })?;
    crate::privileged::run_helper(mgr.lock_gate().await, "swapfile.resize", &payload).await?;
    Ok(())
}

pub(super) async fn apply_now_swapfile_remove(
    mgr: &Manager,
    hdr: Header<'_>,
    path: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.swapfile.remove").await?;
    validation::validate_swapfile_remove_path(path).map_err(invalid)?;
    let payload = serde_json::json!({ "path": path }).to_string();
    crate::privileged::run_helper(mgr.lock_gate().await, "swapfile.remove", &payload).await?;
    Ok(())
}

pub(super) async fn apply_now_sysctl(
    mgr: &Manager,
    hdr: Header<'_>,
    values_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.sysctl.set").await?;
    let values: SysctlValues = serde_json::from_str(values_json).map_err(invalid)?;
    sysctl::validate_sysctl_values(&values).map_err(invalid)?;
    let payload = to_payload(&values)?;
    crate::privileged::run_helper(mgr.lock_gate().await, "sysctl.set", &payload).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapfile_create_payload_rejects_non_bool_flags() {
        let json = r#"{"path":"/swapfile","size_mb":1024,"priority":-2,"prepare":"yes"}"#;
        assert!(swapfile_create_payload(json).is_err());
    }

    #[test]
    fn swapfile_create_payload_drops_unknown_fields() {
        let json = r#"{"path":"/swapfile","size_mb":1024,"priority":-2,"extra":"x"}"#;
        if let Ok(out) = swapfile_create_payload(json) {
            assert!(!out.contains("extra"));
            assert!(!out.contains("prepare"));
        }
    }
}
