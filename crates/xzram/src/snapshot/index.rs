use std::fs;

use tracing::info;

use super::create::{capture_system_state, chrono_like_id, rfc3339_now};
use super::paths::{index_path, snapshots_root};
use super::types::{SnapshotMeta, SnapshotTrigger};
use crate::apply::pending::data_dir;
use crate::error::{Result, XzramError};

pub fn ensure_snapshots_initialized() -> Result<()> {
    fs::create_dir_all(snapshots_root())?;
    if !index_path().exists() {
        write_index(&[])?;
    }
    migrate_legacy_backup()?;
    Ok(())
}

pub fn list_snapshots() -> Result<Vec<SnapshotMeta>> {
    ensure_snapshots_initialized()?;
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
    let mut index = load_index()?;
    let pos = index
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| XzramError::NotFound(format!("snapshot not found: {id}")))?;
    index.remove(pos);
    write_index(&index)?;

    let dir = snapshots_root().join(id);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    info!(id, "deleted snapshot");
    Ok(())
}

pub fn prune_snapshots(keep: usize) -> Result<u32> {
    let index = load_index()?;
    if index.len() <= keep {
        return Ok(0);
    }
    let to_remove: Vec<String> = index.iter().skip(keep).map(|s| s.id.clone()).collect();
    let count = to_remove.len() as u32;
    for id in to_remove {
        delete_snapshot(&id)?;
    }
    Ok(count)
}

pub fn migrate_legacy_backup() -> Result<Option<SnapshotMeta>> {
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
    fs::create_dir_all(&dir)?;

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
    };
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| XzramError::Parse(e.to_string()))?,
    )?;

    let mut index = load_index()?;
    index.push(meta.clone());
    write_index(&index)?;

    fs::remove_dir_all(&legacy)?;
    info!(id = %meta.id, "migrated legacy backup to snapshot");
    Ok(Some(meta))
}

pub(crate) fn load_index() -> Result<Vec<SnapshotMeta>> {
    if !index_path().exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(index_path())?;
    if content.trim().is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_str(&content).map_err(|e| XzramError::Parse(format!("snapshot index: {e}")))
}

pub(crate) fn write_index(index: &[SnapshotMeta]) -> Result<()> {
    fs::create_dir_all(snapshots_root())?;
    let content =
        serde_json::to_string_pretty(index).map_err(|e| XzramError::Parse(e.to_string()))?;
    fs::write(index_path(), content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(index_path(), fs::Permissions::from_mode(0o644));
    }
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
    fn delete_and_prune_snapshots() {
        let _guard = test_lock().lock().unwrap();
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
}
