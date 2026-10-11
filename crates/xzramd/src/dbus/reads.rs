//! Unauthenticated read-only methods.
//!
//! These shell out (`swapon`, `zramctl`, `btrfs`, …) and read sysfs/procfs, so they run on
//! the blocking pool and are bounded by a small semaphore. Any caller on the system bus
//! may invoke them; without the bound a flood of reads could starve authorized
//! mutations of tokio worker threads.

use std::sync::Arc;

use tokio::sync::Semaphore;
use xzram::backend::{available_swapfile_backend, ensure_zram_backend};
use xzram::{detect, doctor, recommend, status, swap_partition, swapfile_btrfs, sysctl};

use super::util::json_map;
use super::{JsonReply, Manager};

/// Maximum concurrent unauthenticated read operations.
pub(super) const MAX_CONCURRENT_READS: usize = 4;

pub(super) fn read_slots() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(MAX_CONCURRENT_READS))
}

/// Run `f` on the blocking pool while holding a read slot.
///
/// The permit moves into the blocking closure, so the slot stays occupied until the
/// blocking work really finishes even if the D-Bus call future is dropped.
pub(super) async fn bounded<T, F>(mgr: &Manager, f: F) -> zbus::fdo::Result<T>
where
    F: FnOnce() -> xzram::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let permit = mgr
        .read_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    tokio::task::spawn_blocking(move || {
        let result = f();
        drop(permit);
        result
    })
    .await
    .map_err(|e| zbus::fdo::Error::Failed(format!("read task failed: {e}")))?
    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
}

async fn bounded_json<T, F>(mgr: &Manager, f: F) -> zbus::fdo::Result<JsonReply>
where
    F: FnOnce() -> xzram::Result<T> + Send + 'static,
    T: serde::Serialize + Send + 'static,
{
    let value = bounded(mgr, f).await?;
    Ok(json_map(&value))
}

pub(super) async fn get_status(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, status::status).await
}

pub(super) async fn get_detection(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, detect::detect).await
}

pub(super) async fn run_doctor(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, doctor::doctor).await
}

pub(super) async fn get_zram_config(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, || ensure_zram_backend()?.show()).await
}

pub(super) async fn list_swapfiles(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, || available_swapfile_backend().list()).await
}

pub(super) async fn list_swaps(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, swap_partition::list_swaps_merged).await
}

pub(super) async fn get_sysctl(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, sysctl::show).await
}

pub(super) async fn get_recommended_defaults(mgr: &Manager) -> zbus::fdo::Result<JsonReply> {
    bounded_json(mgr, recommend::recommend).await
}

pub(super) async fn check_swapfile_btrfs(
    mgr: &Manager,
    path: &str,
) -> zbus::fdo::Result<JsonReply> {
    xzram::validation::validate_swapfile_prepare_path(path)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let path = std::path::PathBuf::from(path);
    bounded_json(mgr, move || swapfile_btrfs::check_nodatacow(&path)).await
}
