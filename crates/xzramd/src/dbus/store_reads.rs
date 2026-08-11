//! Polkit-gated reads of pending config and snapshot metadata.

use xzram::apply::load_pending;
use xzram::snapshot;
use zbus::message::Header;

use super::auth::authorize;
use super::util::json_map;
use super::Manager;

type JsonReply = std::collections::HashMap<String, zbus::zvariant::OwnedValue>;

pub(super) async fn get_pending(mgr: &Manager, hdr: Header<'_>) -> zbus::fdo::Result<JsonReply> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.store.read").await?;
    let pending = load_pending().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(json_map(&pending))
}

pub(super) async fn list_snapshots(mgr: &Manager, hdr: Header<'_>) -> zbus::fdo::Result<JsonReply> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.store.read").await?;
    let list = snapshot::list_snapshots().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(json_map(&list))
}

pub(super) async fn get_snapshot(
    mgr: &Manager,
    hdr: Header<'_>,
    id: &str,
) -> zbus::fdo::Result<JsonReply> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.store.read").await?;
    let meta = snapshot::get_snapshot(id).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(json_map(&meta))
}
