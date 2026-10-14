use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tracing::info;

use super::index::{
    create_private_dir, ensure_snapshots_initialized_unlocked, load_index, write_index_unlocked,
    write_manifest,
};
use super::paths::{
    etc_path, managed_etc_files, snapshots_root, FSTAB, SYSCTL_FILE, ZRAMSWAP_FILE, ZRAM_CONF,
};
use super::types::{
    SnapshotArtifact, SnapshotArtifacts, SnapshotMeta, SnapshotSwapfile, SnapshotTrigger,
};
use crate::apply::store_lock::with_store_lock;
use crate::apply::types::PendingConfig;
use crate::backend::available_swapfile_backend;
use crate::error::Result;
use crate::status::{self, ZramDevice};

pub fn create_snapshot(
    trigger: SnapshotTrigger,
    label: Option<&str>,
    pending: Option<&PendingConfig>,
) -> Result<SnapshotMeta> {
    with_store_lock(|| {
        fs::create_dir_all(snapshots_root())?;
        ensure_snapshots_initialized_unlocked()?;

        let captured = capture_system_state()?;

        if trigger == SnapshotTrigger::AppOpen {
            if let Some(latest) = load_index()?.into_iter().next() {
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
            zswap_enabled: crate::checks::zswap_enabled(),
            oomd_enabled: std::env::var_os("XZRAM_ETC_ROOT")
                .is_none()
                .then(crate::oom::oomd_enabled)
                .flatten(),
        };

        let dir = snapshots_root().join(&id);
        create_private_dir(&dir)?;
        for (relative, filename) in managed_etc_files() {
            let src = etc_path(relative);
            if src.exists() {
                fs::copy(&src, dir.join(filename))?;
            }
        }
        write_manifest(&dir, &meta)?;

        let mut index = load_index()?;
        index.insert(0, meta.clone());
        write_index_unlocked(&index)?;

        info!(id = %meta.id, trigger = %trigger.as_str(), "created snapshot");
        Ok(meta)
    })
}

pub(crate) struct CapturedState {
    pub(crate) state_hash: String,
    pub(crate) artifacts: SnapshotArtifacts,
    pub(crate) swapfiles: Vec<SnapshotSwapfile>,
    pub(crate) zram_devices: Vec<ZramDevice>,
}

pub(crate) fn capture_system_state() -> Result<CapturedState> {
    let mut hash = Sha256::new();
    let mut artifacts = SnapshotArtifacts {
        zram_generator_conf: SnapshotArtifact::default(),
        fstab: SnapshotArtifact::default(),
        sysctl: SnapshotArtifact::default(),
        zramswap: SnapshotArtifact::default(),
        zswap: SnapshotArtifact::default(),
        oomd_root: SnapshotArtifact::default(),
        oomd_user: SnapshotArtifact::default(),
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
            crate::zswap::ZSWAP_TMPFILES => artifacts.zswap = artifact,
            crate::oom::OOMD_ROOT_DROPIN => artifacts.oomd_root = artifact,
            crate::oom::OOMD_USER_DROPIN => artifacts.oomd_user = artifact,
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
    let mut swapfiles = backend
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|sf| SnapshotSwapfile {
            path: sf.path.clone(),
            size_mb: sf.size_mb,
            priority: sf.priority,
            present_on_disk: Path::new(&sf.path).exists(),
        })
        .collect::<Vec<_>>();
    swapfiles.sort_by(|a, b| a.path.cmp(&b.path));

    hash.update(b"runtime:swapfiles");
    for sf in &swapfiles {
        hash.update(sf.path.as_bytes());
        hash.update(sf.size_mb.to_le_bytes());
        hash.update(sf.priority.to_le_bytes());
        hash.update([u8::from(sf.present_on_disk)]);
    }

    let mut zram_devices = status.zram_devices;
    zram_devices.sort_by(|a, b| a.name.cmp(&b.name));
    hash.update(b"runtime:zram");
    for z in &zram_devices {
        hash.update(z.name.as_bytes());
        hash.update(z.algorithm.as_bytes());
        hash.update(z.disk_size_bytes.to_le_bytes());
        hash.update(z.mount_point.as_bytes());
    }

    Ok(CapturedState {
        state_hash: format!("{:x}", hash.finalize()),
        artifacts,
        swapfiles,
        zram_devices,
    })
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
    if !pending.prepare_swapfiles.is_empty() {
        parts.push("btrfs nodatacow".into());
    }
    if pending.disable_zswap {
        parts.push("disable zswap".into());
    }
    if pending.enable_oomd {
        parts.push("enable systemd-oomd".into());
    }
    parts.join(", ")
}

pub(crate) fn chrono_like_id() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();
    format!("{secs:08}{nanos:09}")
}

pub(crate) fn rfc3339_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{secs}")
}

fn human_date() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_utc(secs)
}

/// Format seconds since the Unix epoch as `DD MM YYYY, HH:MM UTC`.
fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let time_secs = secs % 86_400;
    let hour = time_secs / 3600;
    let minute = (time_secs % 3600) / 60;
    format!("{day:02} {month:02} {year}, {hour:02}:{minute:02} UTC")
}

/// Convert days since 1970-01-01 to a proleptic Gregorian (year, month, day).
///
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::types::{SwapfileConfig, ZramConfig};
    use crate::snapshot::test_lock;

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
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(31), (1970, 2, 1));
        assert_eq!(civil_from_days(365), (1971, 1, 1));
        // 2000 is a leap year (divisible by 400).
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        // 2024 leap day; 2023-12-31.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(19_722), (2023, 12, 31));
        // 2100 is not a leap year.
        assert_eq!(civil_from_days(47_540), (2100, 2, 28));
        assert_eq!(civil_from_days(47_541), (2100, 3, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn format_utc_formats_timestamp() {
        // 2024-02-29T13:45:00Z
        assert_eq!(format_utc(1_709_214_300), "29 02 2024, 13:45 UTC");
        assert_eq!(format_utc(0), "01 01 1970, 00:00 UTC");
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
        let _guard = test_lock();
        let env = test_env();
        let etc = env._etc.path();
        fs::create_dir_all(etc.join("systemd")).unwrap();
        fs::write(etc.join("systemd/zram-generator.conf"), "[zram0]\n").unwrap();
        fs::write(etc.join("fstab"), "/ swap ext4 defaults 0 1\n").unwrap();

        let meta = create_snapshot(SnapshotTrigger::Manual, None, None).unwrap();
        assert!(!meta.id.is_empty());
        let list = super::super::index::list_snapshots().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, meta.id);

        cleanup_test_env();
    }

    #[test]
    fn app_open_dedup_skips_identical_hash() {
        let _guard = test_lock();
        let env = test_env();
        fs::write(env._etc.path().join("fstab"), "test\n").unwrap();

        let first = create_snapshot(SnapshotTrigger::AppOpen, None, None).unwrap();
        let second = create_snapshot(SnapshotTrigger::AppOpen, None, None).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(super::super::index::list_snapshots().unwrap().len(), 1);

        cleanup_test_env();
    }
}
