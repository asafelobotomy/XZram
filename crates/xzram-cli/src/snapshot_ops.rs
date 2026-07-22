use std::process::Command;

use xzram::snapshot::{self, SnapshotTrigger};

use crate::privileged::find_helper;

pub(crate) fn resolve_snapshot_id(id: &str) -> anyhow::Result<String> {
    match id {
        "latest" => snapshot::list_snapshots()?
            .into_iter()
            .next()
            .map(|s| s.id)
            .ok_or_else(|| anyhow::anyhow!("no snapshots found")),
        "last-apply" => Ok(snapshot::latest_pre_apply_id()?),
        other => Ok(other.to_string()),
    }
}

pub(crate) fn run_snapshot_create_pkexec(
    label: Option<&str>,
) -> anyhow::Result<snapshot::SnapshotMeta> {
    let payload = serde_json::json!({
        "trigger": SnapshotTrigger::Manual.as_str(),
        "label": label,
    });
    let helper = find_helper()?;
    let output = Command::new("pkexec")
        .arg(&helper)
        .arg("snapshot.create")
        .arg(payload.to_string())
        .output()?;
    if !output.status.success() {
        if let Some(err) = xzram::apply::read_last_error() {
            anyhow::bail!("{err}");
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            anyhow::bail!("{stderr}");
        }
        anyhow::bail!(
            "snapshot create failed (pkexec exit {:?})",
            output.status.code()
        );
    }
    let meta: snapshot::SnapshotMeta = serde_json::from_slice(&output.stdout)?;
    Ok(meta)
}

pub(crate) fn run_snapshot_create_dbus(
    label: Option<&str>,
) -> anyhow::Result<snapshot::SnapshotMeta> {
    let conn = zbus::blocking::Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "io.github.XZram1",
        "/io/github/XZram",
        "io.github.XZram.Manager",
    )?;
    let reply = proxy.call_method(
        "CreateSnapshot",
        &(SnapshotTrigger::Manual.as_str(), label.unwrap_or("")),
    )?;
    let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
        reply.body().deserialize()?;
    let json = map
        .get("json")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid CreateSnapshot response"))?;
    Ok(serde_json::from_str(&json)?)
}
