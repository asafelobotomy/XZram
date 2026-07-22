use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::apply::{self, ApplyResult, PendingConfig};
use crate::backend::available_swapfile_backend;
use crate::error::{Result, XzramError};
use crate::status::{self, ZramDevice};
use crate::swapfile_btrfs;

pub const SNAPSHOTS_DIR: &str = "snapshots";
pub const DEFAULT_KEEP: usize = 50;

const ZRAM_CONF: &str = "systemd/zram-generator.conf";
const FSTAB: &str = "fstab";
const SYSCTL_FILE: &str = "sysctl.d/99-xzram.conf";
const ZRAMSWAP_FILE: &str = "default/zramswap";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTrigger {
    AppOpen,
    PreApply,
    Manual,
}

impl SnapshotTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppOpen => "app_open",
            Self::PreApply => "pre_apply",
            Self::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "app_open" => Ok(Self::AppOpen),
            "pre_apply" => Ok(Self::PreApply),
            "manual" => Ok(Self::Manual),
            _ => Err(XzramError::Validation(format!(
                "unknown snapshot trigger: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotArtifact {
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSwapfile {
    pub path: String,
    pub size_mb: u64,
    pub priority: i32,
    pub present_on_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub created_at: String,
    pub label: String,
    pub trigger: SnapshotTrigger,
    pub state_hash: String,
    pub xzram_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_summary: Option<String>,
    pub artifacts: SnapshotArtifacts,
    pub swapfiles: Vec<SnapshotSwapfile>,
    pub zram_devices: Vec<ZramDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotArtifacts {
    pub zram_generator_conf: SnapshotArtifact,
    pub fstab: SnapshotArtifact,
    pub sysctl: SnapshotArtifact,
    pub zramswap: SnapshotArtifact,
}

pub fn etc_root() -> PathBuf {
    std::env::var("XZRAM_ETC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/etc"))
}

pub fn snapshots_root() -> PathBuf {
    apply::data_dir().join(SNAPSHOTS_DIR)
}

fn index_path() -> PathBuf {
    snapshots_root().join("index.json")
}

fn etc_path(relative: &str) -> PathBuf {
    etc_root().join(relative)
}

fn managed_etc_files() -> [(&'static str, &'static str); 4] {
    [
        (ZRAM_CONF, "zram-generator.conf"),
        (FSTAB, "fstab"),
        (SYSCTL_FILE, "99-xzram.conf"),
        (ZRAMSWAP_FILE, "zramswap"),
    ]
}

pub fn ensure_snapshots_initialized() -> Result<()> {
    fs::create_dir_all(snapshots_root())?;
    if !index_path().exists() {
        write_index(&[])?;
    }
    migrate_legacy_backup()?;
    Ok(())
}

pub fn create_snapshot(
    trigger: SnapshotTrigger,
    label: Option<&str>,
    pending: Option<&PendingConfig>,
) -> Result<SnapshotMeta> {
    ensure_snapshots_initialized()?;

    let captured = capture_system_state()?;

    if trigger == SnapshotTrigger::AppOpen {
        if let Some(latest) = list_snapshots()?.into_iter().next() {
            if latest.state_hash == captured.state_hash {
                info!(id = %latest.id, "skipping duplicate app_open snapshot");
                return Ok(latest);
            }
        }
    }

    let id = format!("{}-{}", chrono_like_id(), trigger.as_str());
    let label = label
        .map(str::to_string)
        .unwrap_or_else(|| default_label(trigger, pending));

    let meta = SnapshotMeta {
        id: id.clone(),
        created_at: rfc3339_now(),
        label,
        trigger,
        state_hash: captured.state_hash.clone(),
        xzram_version: env!("CARGO_PKG_VERSION").to_string(),
        pending_summary: pending.map(pending_summary),
        artifacts: captured.artifacts,
        swapfiles: captured.swapfiles,
        zram_devices: captured.zram_devices,
    };

    let dir = snapshots_root().join(&id);
    fs::create_dir_all(&dir)?;
    for (relative, filename) in managed_etc_files() {
        let src = etc_path(relative);
        if src.exists() {
            fs::copy(&src, dir.join(filename))?;
        }
    }
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| XzramError::Parse(e.to_string()))?,
    )?;

    let mut index = load_index()?;
    index.insert(0, meta.clone());
    write_index(&index)?;

    info!(id = %meta.id, trigger = %trigger.as_str(), "created snapshot");
    Ok(meta)
}

struct CapturedState {
    state_hash: String,
    artifacts: SnapshotArtifacts,
    swapfiles: Vec<SnapshotSwapfile>,
    zram_devices: Vec<ZramDevice>,
}

fn capture_system_state() -> Result<CapturedState> {
    let mut hash = Sha256::new();
    let mut artifacts = SnapshotArtifacts {
        zram_generator_conf: SnapshotArtifact {
            present: false,
            filename: None,
        },
        fstab: SnapshotArtifact {
            present: false,
            filename: None,
        },
        sysctl: SnapshotArtifact {
            present: false,
            filename: None,
        },
        zramswap: SnapshotArtifact {
            present: false,
            filename: None,
        },
    };

    for (relative, filename) in managed_etc_files() {
        let path = etc_path(relative);
        let present = path.exists();
        if present {
            let bytes = fs::read(&path)?;
            hash.update(filename.as_bytes());
            hash.update(&bytes);
        }
        let artifact = SnapshotArtifact {
            present,
            filename: if present {
                Some(filename.to_string())
            } else {
                None
            },
        };
        match relative {
            ZRAM_CONF => artifacts.zram_generator_conf = artifact,
            FSTAB => artifacts.fstab = artifact,
            SYSCTL_FILE => artifacts.sysctl = artifact,
            ZRAMSWAP_FILE => artifacts.zramswap = artifact,
            _ => {}
        }
    }

    let status = status::status().unwrap_or(status::StatusReport {
        swaps: vec![],
        zram_devices: vec![],
        memory: status::MemoryInfo {
            mem_total_kb: 0,
            mem_available_kb: 0,
            swap_total_kb: 0,
            swap_free_kb: 0,
        },
    });

    let backend = available_swapfile_backend();
    let swapfiles = backend
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|sf| SnapshotSwapfile {
            path: sf.path.clone(),
            size_mb: sf.size_mb,
            priority: sf.priority,
            present_on_disk: Path::new(&sf.path).exists(),
        })
        .collect();

    Ok(CapturedState {
        state_hash: format!("{:x}", hash.finalize()),
        artifacts,
        swapfiles,
        zram_devices: status.zram_devices,
    })
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

pub fn restore_snapshot(id: &str) -> Result<ApplyResult> {
    let meta = get_snapshot(id)?;
    let dir = snapshots_root().join(&meta.id);
    if !dir.exists() {
        return Err(XzramError::NotFound(format!(
            "snapshot directory missing: {id}"
        )));
    }

    let mut messages = Vec::new();

    swapoff_managed_swaps(&meta)?;

    restore_etc_file(
        &dir,
        "zram-generator.conf",
        etc_path(ZRAM_CONF),
        meta.artifacts.zram_generator_conf.present,
        &mut messages,
        "zram-generator.conf",
    )?;

    restore_etc_file(
        &dir,
        "fstab",
        etc_path(FSTAB),
        meta.artifacts.fstab.present,
        &mut messages,
        "/etc/fstab",
    )?;

    restore_etc_file(
        &dir,
        "99-xzram.conf",
        etc_path(SYSCTL_FILE),
        meta.artifacts.sysctl.present,
        &mut messages,
        "sysctl configuration",
    )?;

    restore_etc_file(
        &dir,
        "zramswap",
        etc_path(ZRAMSWAP_FILE),
        meta.artifacts.zramswap.present,
        &mut messages,
        "zram-tools configuration",
    )?;

    cleanup_extra_swapfiles(&meta, &mut messages)?;
    recreate_missing_swapfiles(&meta, &mut messages)?;

    apply::run_systemctl(&["daemon-reload"])?;
    messages.push("Reloaded systemd".into());

    if meta.artifacts.zram_generator_conf.present {
        let backup = dir.join("zram-generator.conf");
        if backup.exists() {
            restart_zram_units_from_config(backup.to_str().unwrap())?;
            messages.push("Restarted zram units".into());
        }
    } else {
        for i in 0..8 {
            let device = format!("/dev/zram{i}");
            let _ = apply::run_command("swapoff", &[&device]);
            let _ = apply::run_systemctl(&["stop", &format!("systemd-zram-setup@zram{i}.service")]);
        }
    }

    if meta.artifacts.sysctl.present || etc_path(SYSCTL_FILE).exists() {
        let _ = apply::run_command("sysctl", &["--system"]);
        messages.push("Reloaded sysctl".into());
    }

    swapon_from_fstab(&mut messages)?;

    info!(id = %meta.id, "restored snapshot");
    Ok(ApplyResult {
        success: true,
        messages,
    })
}

pub fn rollback() -> Result<ApplyResult> {
    let id = latest_pre_apply_id()?;
    restore_snapshot(&id)
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
    let legacy = apply::data_dir().join("backup");
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

pub fn label_from_pending(pending: &PendingConfig) -> String {
    default_label(SnapshotTrigger::PreApply, Some(pending))
}

fn default_label(trigger: SnapshotTrigger, pending: Option<&PendingConfig>) -> String {
    let date = human_date();
    match trigger {
        SnapshotTrigger::AppOpen => format!("Startup baseline — {date}"),
        SnapshotTrigger::Manual => format!("Manual snapshot — {date}"),
        SnapshotTrigger::PreApply => {
            let summary = pending.map(pending_summary).unwrap_or_default();
            if summary.is_empty() {
                format!("Before apply — {date}")
            } else {
                format!("Before apply — {summary}")
            }
        }
    }
}

pub fn pending_summary(pending: &PendingConfig) -> String {
    let mut parts = Vec::new();
    if pending.disable_zram {
        parts.push("disable ZRAM".into());
    }
    if let Some(ref zram) = pending.zram {
        let algo = zram.compression_algorithm.as_deref().unwrap_or("default");
        let size = zram.zram_size.as_deref().unwrap_or("default size");
        parts.push(format!("ZRAM ({algo}, {size})"));
    }
    if let Some(ref sf) = pending.swapfile {
        parts.push(format!("swapfile {}", sf.path));
    }
    if let Some(ref resize) = pending.swapfile_resize {
        parts.push(format!("resize {} to {} MiB", resize.path, resize.size_mb));
    }
    if let Some(ref path) = pending.remove_swapfile {
        parts.push(format!("remove swapfile {path}"));
    }
    if pending.sysctl.is_some() {
        parts.push("sysctl".into());
    }
    parts.join(", ")
}

fn swapoff_managed_swaps(meta: &SnapshotMeta) -> Result<()> {
    for swap in &meta.swapfiles {
        let _ = apply::run_command("swapoff", &[&swap.path]);
    }
    for device in &meta.zram_devices {
        let path = format!("/dev/{}", device.name);
        let _ = apply::run_command("swapoff", &[&path]);
    }
    for i in 0..8 {
        let path = format!("/dev/zram{i}");
        let _ = apply::run_command("swapoff", &[&path]);
    }
    Ok(())
}

fn restore_etc_file(
    snapshot_dir: &Path,
    filename: &str,
    target: PathBuf,
    was_present: bool,
    messages: &mut Vec<String>,
    label: &str,
) -> Result<()> {
    let backup = snapshot_dir.join(filename);
    if was_present {
        if backup.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&backup, &target)?;
            messages.push(format!("Restored {label}"));
        }
    } else if target.exists() {
        fs::remove_file(&target)?;
        messages.push(format!("Removed {label} (was absent in snapshot)"));
    }
    Ok(())
}

fn cleanup_extra_swapfiles(meta: &SnapshotMeta, messages: &mut Vec<String>) -> Result<()> {
    let snapshot_paths: HashSet<&str> = meta.swapfiles.iter().map(|s| s.path.as_str()).collect();
    let backend = available_swapfile_backend();
    let current = backend.list().unwrap_or_default();
    for sf in current {
        if !snapshot_paths.contains(sf.path.as_str()) {
            let _ = apply::run_command("swapoff", &[&sf.path]);
            if Path::new(&sf.path).exists() {
                fs::remove_file(&sf.path)?;
                messages.push(format!("Removed swapfile {} (not in snapshot)", sf.path));
            }
        }
    }
    Ok(())
}

fn recreate_missing_swapfiles(meta: &SnapshotMeta, messages: &mut Vec<String>) -> Result<()> {
    for sf in &meta.swapfiles {
        let path = Path::new(&sf.path);
        if path.exists() {
            continue;
        }
        swapfile_btrfs::create_allocated_swapfile(path, sf.size_mb)?;
        fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        messages.push(format!(
            "Recreated swapfile {} ({} MiB)",
            sf.path, sf.size_mb
        ));
    }
    Ok(())
}

fn swapon_from_fstab(messages: &mut Vec<String>) -> Result<()> {
    let fstab = etc_path(FSTAB);
    if !fstab.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(fstab)?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 3 && parts[2] == "swap" {
            let device = parts[0];
            if device.starts_with('/') && !device.starts_with("/dev/") {
                let priority = crate::backend::swapfile::parse_fstab_priority(
                    parts.get(3).copied().unwrap_or("defaults"),
                );
                let _ = apply::run_command("swapon", &["-p", &priority.to_string(), device]);
            }
        }
    }
    messages.push("Activated swapfiles from fstab".into());
    Ok(())
}

fn restart_zram_units_from_config(path: &str) -> Result<()> {
    let conf = crate::config::parse_zram_generator_conf(path)?;
    for device in &conf.devices {
        let unit = format!("systemd-zram-setup@{}.service", device.name);
        let _ = apply::run_systemctl(&["try-restart", &unit]);
    }
    Ok(())
}

fn load_index() -> Result<Vec<SnapshotMeta>> {
    if !index_path().exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(index_path())?;
    if content.trim().is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_str(&content).map_err(|e| XzramError::Parse(format!("snapshot index: {e}")))
}

fn write_index(index: &[SnapshotMeta]) -> Result<()> {
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

fn chrono_like_id() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();
    format!("{secs:08}{nanos:09}")
}

fn rfc3339_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{secs}")
}

fn human_date() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple UTC formatting without chrono dependency
    let days = secs / 86400;
    let day = days % 31 + 1;
    let month = (days / 31) % 12 + 1;
    let year = 1970 + days / 365;
    let time_secs = secs % 86400;
    let hour = time_secs / 3600;
    let minute = (time_secs % 3600) / 60;
    format!("{day:02} {month:02} {year}, {hour:02}:{minute:02} UTC")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{SwapfileConfig, ZramConfig};

    struct TestEnv {
        _data: tempfile::TempDir,
        _etc: tempfile::TempDir,
    }

    fn test_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
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
    fn pending_summary_describes_changes() {
        let pending = PendingConfig {
            zram: Some(ZramConfig {
                device: "zram0".into(),
                zram_size: Some("4096".into()),
                zram_resident_limit: None,
                compression_algorithm: Some("zstd".into()),
                swap_priority: Some(100),
                fs_type: None,
                mount_point: None,
            }),
            swapfile: Some(SwapfileConfig {
                path: "/swap/swapfile".into(),
                size_mb: 8192,
                priority: 10,
            }),
            ..Default::default()
        };
        let summary = pending_summary(&pending);
        assert!(summary.contains("ZRAM"));
        assert!(summary.contains("/swap/swapfile"));
    }

    #[test]
    fn create_and_list_snapshot() {
        let _guard = test_lock().lock().unwrap();
        let env = test_env();
        let etc = env._etc.path();
        fs::create_dir_all(etc.join("systemd")).unwrap();
        fs::write(etc.join("systemd/zram-generator.conf"), "[zram0]\n").unwrap();
        fs::write(etc.join("fstab"), "/ swap ext4 defaults 0 1\n").unwrap();

        let meta = create_snapshot(SnapshotTrigger::Manual, None, None).unwrap();
        assert!(!meta.id.is_empty());
        let list = list_snapshots().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, meta.id);

        cleanup_test_env();
    }

    #[test]
    fn app_open_dedup_skips_identical_hash() {
        let _guard = test_lock().lock().unwrap();
        let env = test_env();
        fs::write(env._etc.path().join("fstab"), "test\n").unwrap();

        let first = create_snapshot(SnapshotTrigger::AppOpen, None, None).unwrap();
        let second = create_snapshot(SnapshotTrigger::AppOpen, None, None).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(list_snapshots().unwrap().len(), 1);

        cleanup_test_env();
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
