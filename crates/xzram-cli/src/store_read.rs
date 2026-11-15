//! Read pending/snapshots from the local store, falling back to D-Bus when
//! `/var/lib/xzram` is not readable (root-only modes). D-Bus methods are
//! gated by `io.github.xzram.store.read`.

use xzram::apply::{load_pending, PendingConfig};
use xzram::error::XzramError;
use xzram::snapshot::{self, SnapshotMeta};

fn is_permission_denied(err: &XzramError) -> bool {
    match err {
        XzramError::Permission(_) => true,
        XzramError::Io(e) => e.kind() == std::io::ErrorKind::PermissionDenied,
        _ => false,
    }
}

fn json_field_from_map(
    map: &std::collections::HashMap<String, zbus::OwnedVariant>,
    method: &str,
) -> anyhow::Result<String> {
    map.get("json")
        .and_then(|v| v.downcast_ref::<zbus::Str>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid {method} response"))
}

/// Call a parameterless Manager method that replies `a{sv}` with a `json` field.
fn fetch_json(method: &str) -> anyhow::Result<String> {
    zbus::block_on(async {
        let conn = zbus::Connection::system().await?;
        let proxy = zbus::Proxy::new(
            &conn,
            "io.github.XZram1",
            "/io/github/XZram",
            "io.github.XZram.Manager",
        )
        .await?;
        let reply = proxy.call_method(method, &()).await?;
        let map: std::collections::HashMap<String, zbus::OwnedVariant> =
            reply.body().deserialize()?;
        json_field_from_map(&map, method)
    })
}

pub(crate) fn load_pending_readable() -> anyhow::Result<Option<PendingConfig>> {
    match load_pending() {
        Ok(p) => Ok(p),
        Err(e) if is_permission_denied(&e) => {
            let json = fetch_json("GetPending")?;
            Ok(serde_json::from_str(&json)?)
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn list_snapshots() -> anyhow::Result<Vec<SnapshotMeta>> {
    match snapshot::list_snapshots() {
        Ok(list) => Ok(list),
        Err(e) if is_permission_denied(&e) => {
            let json = fetch_json("ListSnapshots")?;
            Ok(serde_json::from_str(&json)?)
        }
        Err(e) => Err(e.into()),
    }
}
