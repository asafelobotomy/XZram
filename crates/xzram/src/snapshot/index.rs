use std::fs;

use tracing::info;

use super::create::{capture_system_state, chrono_like_id, rfc3339_now};
use super::paths::{index_path, snapshots_root};
use super::types::{SnapshotMeta, SnapshotTrigger};
use crate::apply::pending::data_dir;
use crate::apply::store_lock::with_store_lock;
use crate::error::{Result, XzramError};

pub fn ensure_snapshots_initialized() -> Result<()> {
    fs::create_dir_all(snapshots_root())?;
    with_store_lock(ensure_snapshots_initialized_unlocked)
}

pub(crate) fn ensure_snapshots_initialized_unlocked() -> Result<()> {
    if !index_path().try_exists()? {
        write_index_unlocked(&[])?;
    }
    migrate_legacy_backup_unlocked()?;
    Ok(())
}

pub fn list_snapshots() -> Result<Vec<SnapshotMeta>> {
    // Read-only: do not create dirs or migrate on list (avoids EACCES → polkit prompts).
    load_index()
}

pub fn get_snapshot(id: &str) -> Result<SnapshotMeta> {
    list_snapshots()?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| XzramError::NotFound(format!("snapshot not found: {id}")))
}

pub fn latest_pre_apply_id() -> Result<String> {
    list_snapshots()?
        .into_iter()
        .find(|s| s.trigger == SnapshotTrigger::PreApply)
        .map(|s| s.id)
        .ok_or_else(|| XzramError::NotFound("no pre_apply snapshot found".into()))
}

pub fn delete_snapshot(id: &str) -> Result<()> {
    with_store_lock(|| delete_snapshot_unlocked(id))
}

fn delete_snapshot_unlocked(id: &str) -> Result<()> {
    let mut index = load_index()?;
    let pos = index
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| XzramError::NotFound(format!("snapshot not found: {id}")))?;
    index.remove(pos);
    write_index_unlocked(&index)?;

    let dir = snapshots_root().join(id);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    info!(id, "deleted snapshot");
    Ok(())
}

pub fn prune_snapshots(keep: usize) -> Result<u32> {
    if keep == 0 {
        return Err(XzramError::Validation(
            "prune keep must be at least 1 (refusing to delete all snapshots)".into(),
        ));
    }
    with_store_lock(|| {
        let index = load_index()?;
        if index.len() <= keep {
            return Ok(0);
        }
        let to_remove: Vec<String> = index.iter().skip(keep).map(|s| s.id.clone()).collect();
        let count = to_remove.len() as u32;
        for id in to_remove {
            delete_snapshot_unlocked(&id)?;
        }
        Ok(count)
    })
}

pub fn migrate_legacy_backup() -> Result<Option<SnapshotMeta>> {
    with_store_lock(migrate_legacy_backup_unlocked)
}

fn migrate_legacy_backup_unlocked() -> Result<Option<SnapshotMeta>> {
    let legacy = data_dir().join("backup");
    if !legacy.exists() {
        return Ok(None);
    }
    if !legacy.join("fstab").exists()
        && !legacy.join("zram-generator.conf").exists()
        && !legacy.join("99-xzram.conf").exists()
    {
        return Ok(None);
    }

    let id = format!("{}-legacy_import", chrono_like_id());
    let dir = snapshots_root().join(&id);
    create_private_dir(&dir)?;

    for name in ["zram-generator.conf", "fstab", "99-xzram.conf"] {
        let src = legacy.join(name);
        if src.exists() {
            fs::copy(&src, dir.join(name))?;
        }
    }

    let captured = capture_system_state()?;
    let meta = SnapshotMeta {
        id: id.clone(),
        created_at: rfc3339_now(),
        label: "Legacy backup (imported)".into(),
        trigger: SnapshotTrigger::Manual,
        state_hash: captured.state_hash,
        xzram_version: env!("CARGO_PKG_VERSION").to_string(),
        pending_summary: None,
        artifacts: captured.artifacts,
        swapfiles: captured.swapfiles,
        zram_devices: captured.zram_devices,
        zswap_enabled: None,
        oomd_enabled: None,
    };
    write_manifest(&dir, &meta)?;

    let mut index = load_index()?;
    index.push(meta.clone());
    write_index_unlocked(&index)?;

    fs::remove_dir_all(&legacy)?;
    info!(id = %meta.id, "migrated legacy backup to snapshot");
    Ok(Some(meta))
}

pub(crate) fn load_index() -> Result<Vec<SnapshotMeta>> {
    // try_exists (not exists) so EACCES surfaces as PermissionDenied for the D-Bus fallback.
    if !index_path().try_exists()? {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(index_path())?;
    if content.trim().is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_str(&content).map_err(|e| XzramError::Parse(format!("snapshot index: {e}")))
}

/// Mode of the snapshots root: traversable (so index.json can be read) but not listable.
const SNAPSHOTS_ROOT_MODE: u32 = 0o711;
/// Per-snapshot payload directories hold copies of /etc files and stay root-only.
const SNAPSHOT_DIR_MODE: u32 = 0o700;

fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

/// Create a per-snapshot payload directory with private permissions.
pub(crate) fn create_private_dir(dir: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    set_mode(dir, SNAPSHOT_DIR_MODE);
    Ok(())
}

pub(crate) fn write_manifest(dir: &std::path::Path, meta: &SnapshotMeta) -> Result<()> {
    let content =
        serde_json::to_string_pretty(meta).map_err(|e| XzramError::Parse(e.to_string()))?;
    crate::fsutil::atomic_write(&dir.join("manifest.json"), content.as_bytes(), 0o600)
}

pub(crate) fn write_index_unlocked(index: &[SnapshotMeta]) -> Result<()> {
    let root = snapshots_root();
    fs::create_dir_all(&root)?;
    set_mode(&root, SNAPSHOTS_ROOT_MODE);
    set_mode(&data_dir(), 0o755);
    // Root is no longer 0700, so make sure payload dirs (incl. ones created by older
    // versions with the default umask) are private.
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                set_mode(&entry.path(), SNAPSHOT_DIR_MODE);
            }
        }
    }
    let content =
        serde_json::to_string_pretty(index).map_err(|e| XzramError::Parse(e.to_string()))?;
    crate::fsutil::atomic_write(&index_path(), content.as_bytes(), 0o644)?;
    set_mode(&index_path(), 0o644);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::create::create_snapshot;
    use crate::snapshot::test_lock;
    use crate::snapshot::types::SnapshotTrigger;

    struct TestEnv {
        _data: tempfile::TempDir,
        _etc: tempfile::TempDir,
    }

    fn test_env() -> TestEnv {
        let data = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());
        TestEnv {
            _data: data,
            _etc: etc,
        }
    }

    fn cleanup_test_env() {
        std::env::remove_var("XZRAM_DATA_DIR");
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn list_snapshots_empty_without_creating_store() {
        let _guard = test_lock();
        let data = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        assert!(list_snapshots().unwrap().is_empty());
        assert!(!snapshots_root().exists());
        std::env::remove_var("XZRAM_DATA_DIR");
    }

    #[test]
    fn delete_and_prune_snapshots() {
        let _guard = test_lock();
        let env = test_env();
        fs::write(env._etc.path().join("fstab"), "a\n").unwrap();
        create_snapshot(SnapshotTrigger::Manual, Some("one"), None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        fs::write(env._etc.path().join("fstab"), "b\n").unwrap();
        create_snapshot(SnapshotTrigger::Manual, Some("two"), None).unwrap();

        assert_eq!(list_snapshots().unwrap().len(), 2);
        let removed = prune_snapshots(1).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(list_snapshots().unwrap().len(), 1);

        cleanup_test_env();
    }

    #[test]
    fn index_world_readable_payload_private() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = test_lock();
        let env = test_env();
        fs::write(env._etc.path().join("fstab"), "a\n").unwrap();
        let meta = create_snapshot(SnapshotTrigger::Manual, Some("m"), None).unwrap();
        let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&snapshots_root()), 0o711);
        assert_eq!(mode(&index_path()), 0o644);
        assert_eq!(mode(&snapshots_root().join(&meta.id)), 0o700);
        cleanup_test_env();
    }

    #[test]
    fn load_index_surfaces_permission_denied() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = test_lock();
        let data = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_DATA_DIR", data.path());
        write_index_unlocked(&[]).unwrap();
        fs::set_permissions(snapshots_root(), fs::Permissions::from_mode(0o000)).unwrap();
        let result = load_index();
        fs::set_permissions(snapshots_root(), fs::Permissions::from_mode(0o711)).unwrap();
        std::env::remove_var("XZRAM_DATA_DIR");
        // Root bypasses DAC, so only assert when running unprivileged.
        if unsafe { libc::geteuid() } != 0 {
            match result {
                Err(XzramError::Io(e)) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied)
                }
                other => panic!("expected PermissionDenied, got {other:?}"),
            }
        }
    }
}
