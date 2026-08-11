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
    map: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    method: &str,
) -> anyhow::Result<String> {
    map.get("json")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid {method} response"))
}

pub(crate) fn load_pending_readable() -> anyhow::Result<Option<PendingConfig>> {
    match load_pending() {
        Ok(p) => Ok(p),
        Err(e) if is_permission_denied(&e) => {
            let conn = zbus::blocking::Connection::system()?;
            let proxy = zbus::blocking::Proxy::new(
                &conn,
                "io.github.XZram1",
                "/io/github/XZram",
                "io.github.XZram.Manager",
            )?;
            let reply = proxy.call_method("GetPending", &())?;
            let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
                reply.body().deserialize()?;
            let json = json_field_from_map(&map, "GetPending")?;
            Ok(serde_json::from_str(&json)?)
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn list_snapshots() -> anyhow::Result<Vec<SnapshotMeta>> {
    match snapshot::list_snapshots() {
        Ok(list) => Ok(list),
        Err(e) if is_permission_denied(&e) => {
            let conn = zbus::blocking::Connection::system()?;
            let proxy = zbus::blocking::Proxy::new(
                &conn,
                "io.github.XZram1",
                "/io/github/XZram",
                "io.github.XZram.Manager",
            )?;
            let reply = proxy.call_method("ListSnapshots", &())?;
            let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
                reply.body().deserialize()?;
            let json = json_field_from_map(&map, "ListSnapshots")?;
            Ok(serde_json::from_str(&json)?)
        }
        Err(e) => Err(e.into()),
    }
}
