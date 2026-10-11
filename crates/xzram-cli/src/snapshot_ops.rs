use std::process::Command;
use std::time::SystemTime;

use xzram::snapshot::{self, SnapshotTrigger};

use crate::dbus_client::{call_classified, manager_proxy};
use crate::privileged::{find_helper_for_pkexec, pkexec_failure};

pub(crate) fn resolve_snapshot_id(id: &str) -> anyhow::Result<String> {
    match id {
        "latest" => crate::store_read::list_snapshots()?
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
    trigger: SnapshotTrigger,
) -> anyhow::Result<snapshot::SnapshotMeta> {
    let payload = serde_json::json!({
        "trigger": trigger.as_str(),
        "label": label,
    });
    let helper = find_helper_for_pkexec()?;
    let spawned_at = SystemTime::now();
    let output = Command::new("pkexec")
        .arg(&helper)
        .arg("snapshot.create")
        .arg(payload.to_string())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(pkexec_failure(
            output.status,
            spawned_at,
            Some(&stderr),
            "snapshot create",
        ));
    }
    let meta: snapshot::SnapshotMeta = serde_json::from_slice(&output.stdout)?;
    Ok(meta)
}

pub(crate) fn run_snapshot_create_dbus(
    label: Option<&str>,
    trigger: SnapshotTrigger,
) -> anyhow::Result<snapshot::SnapshotMeta> {
    let (_conn, proxy) = manager_proxy()?;
    let reply = call_classified(
        &proxy,
        "CreateSnapshot",
        &(trigger.as_str(), label.unwrap_or("")),
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
