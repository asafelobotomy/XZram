//! `--dry-run`: gather what a plan needs (live memory, stored snapshots, disk space) and
//! print it. Nothing here changes the system, and none of it needs root.

use xzram::apply::ApplyOptions;
use xzram::backend::available_swapfile_backend;
use xzram::snapshot::SnapshotMeta;
use xzram::status::{self, StatusReport};
use xzram::validation::{
    validate_swapfile_config, validate_swapfile_remove_path, validate_swapfile_resize_path,
};

use crate::plan;

fn report() -> anyhow::Result<StatusReport> {
    Ok(status::status()?)
}

/// Free bytes on the disk that holds `path` (looked up from its nearest existing parent).
fn free_bytes(path: &str) -> Option<u64> {
    let mut dir = std::path::Path::new(path).parent();
    while let Some(d) = dir {
        if d.exists() {
            return fs2::available_space(d).ok();
        }
        dir = d.parent();
    }
    None
}

fn used_bytes(report: &StatusReport, path: &str) -> u64 {
    report
        .swaps
        .iter()
        .filter(|s| s.name == path)
        .map(|s| s.used_bytes)
        .sum()
}

fn find(id: &str) -> anyhow::Result<Option<SnapshotMeta>> {
    let resolved = crate::snapshot_ops::resolve_snapshot_id(id);
    let Ok(id) = resolved else { return Ok(None) };
    Ok(crate::store_read::list_snapshots()?
        .into_iter()
        .find(|s| s.id == id))
}

pub(crate) fn zram_disable(opts: ApplyOptions, json: bool) -> anyhow::Result<()> {
    plan::zram_disable(opts, &report()?).print(json)
}

pub(crate) fn swapfile_create(
    path: &str,
    size_mb: u64,
    priority: i32,
    json: bool,
) -> anyhow::Result<()> {
    let config = xzram::apply::SwapfileConfig {
        path: path.into(),
        size_mb,
        priority,
    };
    let valid = validate_swapfile_config(&config)
        .map(|_| ())
        .map_err(|e| e.to_string());
    plan::swapfile_create(path, size_mb, priority, free_bytes(path), valid).print(json)
}

pub(crate) fn swapfile_resize(path: &str, size_mb: u64, json: bool) -> anyhow::Result<()> {
    let report = report()?;
    let valid = validate_swapfile_resize_path(path, size_mb).map_err(|e| e.to_string());
    let used = used_bytes(&report, path);
    plan::swapfile_resize(path, size_mb, used, free_bytes(path), &report, valid).print(json)
}

pub(crate) fn swapfile_remove(path: &str, json: bool) -> anyhow::Result<()> {
    let report = report()?;
    let size_mb = available_swapfile_backend()
        .list()?
        .into_iter()
        .find(|f| f.path == path)
        .map(|f| f.size_mb);
    let valid = validate_swapfile_remove_path(path)
        .map(|_| ())
        .map_err(|e| e.to_string());
    let used = used_bytes(&report, path);
    plan::swapfile_remove(path, size_mb, used, &report, valid).print(json)
}

pub(crate) fn snapshot_restore(id: &str, json: bool) -> anyhow::Result<()> {
    plan::snapshot_restore(find(id)?.as_ref(), id, &report()?).print(json)
}

pub(crate) fn snapshot_delete(id: &str, json: bool) -> anyhow::Result<()> {
    plan::snapshot_delete(find(id)?.as_ref(), id).print(json)
}

pub(crate) fn snapshot_prune(keep: u32, json: bool) -> anyhow::Result<()> {
    let list = crate::store_read::list_snapshots()?;
    plan::snapshot_prune(&list, keep as usize).print(json)
}

pub(crate) fn rollback(json: bool) -> anyhow::Result<()> {
    let latest = xzram::snapshot::latest_pre_apply_id().ok().and_then(|id| {
        crate::store_read::list_snapshots()
            .ok()?
            .into_iter()
            .find(|s| s.id == id)
    });
    plan::rollback(latest.as_ref(), &report()?).print(json)
}
