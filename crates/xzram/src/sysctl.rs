use crate::apply::{self};
use crate::error::{Result, XzramError};
use crate::snapshot::paths::{etc_path, SYSCTL_FILE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SysctlValues {
    pub swappiness: Option<u32>,
    pub watermark_boost_factor: Option<u32>,
    pub watermark_scale_factor: Option<u32>,
    pub page_cluster: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vfs_cache_pressure: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_background_bytes: Option<u64>,
}

/// Every managed key as (sysctl name, field accessor). Values are carried as u64.
type Field = fn(&mut SysctlValues) -> FieldRef<'_>;

enum FieldRef<'a> {
    U32(&'a mut Option<u32>),
    U64(&'a mut Option<u64>),
}

impl FieldRef<'_> {
    fn get(&self) -> Option<u64> {
        match self {
            FieldRef::U32(v) => v.map(u64::from),
            FieldRef::U64(v) => **v,
        }
    }

    fn set(&mut self, value: Option<u64>) {
        match self {
            FieldRef::U32(v) => **v = value.and_then(|x| u32::try_from(x).ok()),
            FieldRef::U64(v) => **v = value,
        }
    }
}

const KEYS: [(&str, Field); 7] = [
    ("vm.swappiness", |v| FieldRef::U32(&mut v.swappiness)),
    ("vm.watermark_boost_factor", |v| {
        FieldRef::U32(&mut v.watermark_boost_factor)
    }),
    ("vm.watermark_scale_factor", |v| {
        FieldRef::U32(&mut v.watermark_scale_factor)
    }),
    ("vm.page-cluster", |v| FieldRef::U32(&mut v.page_cluster)),
    ("vm.vfs_cache_pressure", |v| {
        FieldRef::U32(&mut v.vfs_cache_pressure)
    }),
    ("vm.dirty_bytes", |v| FieldRef::U64(&mut v.dirty_bytes)),
    ("vm.dirty_background_bytes", |v| {
        FieldRef::U64(&mut v.dirty_background_bytes)
    }),
];

impl SysctlValues {
    /// (sysctl name, value) for every set key, in drop-in order.
    pub fn entries(&self) -> Vec<(&'static str, u64)> {
        let mut copy = self.clone();
        KEYS.iter()
            .filter_map(|(key, field)| field(&mut copy).get().map(|v| (*key, v)))
            .collect()
    }

    fn set_key(&mut self, key: &str, value: u64) {
        if let Some((_, field)) = KEYS.iter().find(|(k, _)| *k == key) {
            field(self).set(Some(value));
        }
    }
}

pub fn show() -> Result<SysctlValues> {
    let mut values = SysctlValues::default();
    for (key, _) in KEYS {
        if let Some(v) = read_sysctl(key) {
            values.set_key(key, v);
        }
    }
    Ok(values)
}

/// Map a sysctl dotted name to its `/proc/sys` path (`vm.page-cluster` → `vm/page-cluster`).
pub fn proc_sys_path(key: &str) -> String {
    format!("/proc/sys/{}", key.replace('.', "/"))
}

fn read_sysctl(key: &str) -> Option<u64> {
    std::fs::read_to_string(proc_sys_path(key))
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Parse known keys from an existing `/etc/sysctl.d/99-xzram.conf` drop-in.
fn read_drop_in(path: &std::path::Path) -> SysctlValues {
    let mut values = SysctlValues::default();
    let Ok(content) = std::fs::read_to_string(path) else {
        return values;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        if let Ok(v) = raw.trim().parse::<u64>() {
            values.set_key(key.trim(), v);
        }
    }
    values
}

/// Keys set in `overlay` win; unset keys keep `base`.
pub fn merge_sysctl_overlay(base: SysctlValues, overlay: &SysctlValues) -> SysctlValues {
    let mut merged = base;
    for (key, value) in overlay.entries() {
        merged.set_key(key, value);
    }
    merged
}

/// Kernel minimum for `vm.dirty_bytes` (two pages); `0` means "use the ratio instead".
const DIRTY_BYTES_MIN: u64 = 2 * 4096;
const DIRTY_BYTES_MAX: u64 = 64 << 30;

/// Validate sysctl values against kernel-aligned ranges (matches GUI spin caps).
pub fn validate_sysctl_values(values: &SysctlValues) -> Result<()> {
    let ranges: [(&str, Option<u64>, u64, u64); 5] = [
        ("vm.swappiness", values.swappiness.map(u64::from), 0, 200),
        (
            "vm.watermark_boost_factor",
            values.watermark_boost_factor.map(u64::from),
            0,
            10000,
        ),
        (
            "vm.watermark_scale_factor",
            values.watermark_scale_factor.map(u64::from),
            0,
            10000,
        ),
        ("vm.page-cluster", values.page_cluster.map(u64::from), 0, 8),
        // 0 lets dentry/inode caches grow until OOM; cap well above the 100 default.
        (
            "vm.vfs_cache_pressure",
            values.vfs_cache_pressure.map(u64::from),
            1,
            1000,
        ),
    ];
    for (key, value, min, max) in ranges {
        if let Some(v) = value.filter(|v| !(min..=max).contains(v)) {
            return Err(XzramError::Validation(format!(
                "{key} must be {min}–{max} (got {v})"
            )));
        }
    }
    for (key, value) in [
        ("vm.dirty_bytes", values.dirty_bytes),
        ("vm.dirty_background_bytes", values.dirty_background_bytes),
    ] {
        if let Some(v) =
            value.filter(|&v| v != 0 && !(DIRTY_BYTES_MIN..=DIRTY_BYTES_MAX).contains(&v))
        {
            return Err(XzramError::Validation(format!(
                "{key} must be 0 or {DIRTY_BYTES_MIN}–{DIRTY_BYTES_MAX} bytes (got {v})"
            )));
        }
    }
    if let (Some(fg), Some(bg)) = (values.dirty_bytes, values.dirty_background_bytes) {
        if fg != 0 && bg >= fg {
            return Err(XzramError::Validation(format!(
                "vm.dirty_background_bytes ({bg}) must be below vm.dirty_bytes ({fg})"
            )));
        }
    }
    Ok(())
}

pub fn set(values: &SysctlValues) -> Result<()> {
    validate_sysctl_values(values)?;
    let path = etc_path(SYSCTL_FILE);
    let merged = merge_sysctl_overlay(read_drop_in(&path), values);
    validate_sysctl_values(&merged)?;

    let lines: Vec<String> = merged
        .entries()
        .into_iter()
        .map(|(key, v)| format!("{key} = {v}"))
        .collect();
    if lines.is_empty() {
        return Err(XzramError::Validation("No sysctl values provided".into()));
    }

    let content = format!("{}\n", lines.join("\n"));
    crate::fsutil::atomic_write(&path, content.as_bytes(), 0o644)?;

    // Skip live reload under hermetic etc roots (unit tests / fixtures).
    if std::env::var_os("XZRAM_ETC_ROOT").is_none() {
        apply::run_command("sysctl", &["--system"])?;
    }
    Ok(())
}

pub fn zram_tuning_defaults() -> SysctlValues {
    SysctlValues {
        swappiness: Some(180),
        watermark_boost_factor: Some(0),
        watermark_scale_factor: Some(125),
        page_cluster: Some(0),
        ..Default::default()
    }
}

/// Desktop responsiveness knobs (CachyOS-Settings values): flush dirty pages in small
/// batches instead of up to 20% of RAM at once, and keep dentry/inode caches longer.
pub fn desktop_tuning_defaults() -> SysctlValues {
    SysctlValues {
        vfs_cache_pressure: Some(50),
        dirty_bytes: Some(256 << 20),
        dirty_background_bytes: Some(64 << 20),
        ..Default::default()
    }
}

/// Everything recommended defaults stage: zram tuning plus desktop tuning.
pub fn recommended_defaults() -> SysctlValues {
    merge_sysctl_overlay(zram_tuning_defaults(), &desktop_tuning_defaults())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_sys_path_maps_dots_to_slashes() {
        assert_eq!(proc_sys_path("vm.swappiness"), "/proc/sys/vm/swappiness");
        assert_eq!(
            proc_sys_path("vm.page-cluster"),
            "/proc/sys/vm/page-cluster"
        );
        assert_eq!(
            proc_sys_path("vm.watermark_scale_factor"),
            "/proc/sys/vm/watermark_scale_factor"
        );
    }

    #[test]
    fn set_merges_existing_drop_in_keys() {
        let _guard = crate::test_env_lock();
        let dir = tempfile::tempdir().unwrap();
        let etc = dir.path().join("etc");
        std::fs::create_dir_all(etc.join("sysctl.d")).unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", &etc);
        let path = etc.join("sysctl.d/99-xzram.conf");
        std::fs::write(
            &path,
            "vm.swappiness = 60\nvm.page-cluster = 0\nvm.watermark_scale_factor = 125\n",
        )
        .unwrap();

        set(&SysctlValues {
            swappiness: Some(180),
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: None,
            ..Default::default()
        })
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("vm.swappiness = 180"));
        assert!(content.contains("vm.page-cluster = 0"));
        assert!(content.contains("vm.watermark_scale_factor = 125"));
        std::env::remove_var("XZRAM_ETC_ROOT");
    }

    #[test]
    fn validate_sysctl_accepts_gui_ranges() {
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: Some(200),
            watermark_boost_factor: Some(10000),
            watermark_scale_factor: Some(0),
            page_cluster: Some(8),
            ..Default::default()
        })
        .is_ok());
        assert!(validate_sysctl_values(&zram_tuning_defaults()).is_ok());
        assert!(validate_sysctl_values(&recommended_defaults()).is_ok());
    }

    #[test]
    fn validate_sysctl_rejects_out_of_range() {
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: Some(201),
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: None,
            ..Default::default()
        })
        .is_err());
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: None,
            watermark_boost_factor: Some(10001),
            watermark_scale_factor: None,
            page_cluster: None,
            ..Default::default()
        })
        .is_err());
        assert!(validate_sysctl_values(&SysctlValues {
            swappiness: None,
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: Some(9),
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn dirty_and_vfs_validation() {
        let ok = recommended_defaults();
        assert!(validate_sysctl_values(&ok).is_ok());
        for bad in [
            SysctlValues {
                vfs_cache_pressure: Some(0),
                ..Default::default()
            },
            SysctlValues {
                dirty_bytes: Some(100),
                ..Default::default()
            },
            SysctlValues {
                dirty_bytes: Some(64 << 20),
                dirty_background_bytes: Some(64 << 20),
                ..Default::default()
            },
        ] {
            assert!(validate_sysctl_values(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn set_writes_and_rereads_byte_keys() {
        let _guard = crate::test_env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", dir.path());
        set(&recommended_defaults()).unwrap();
        let path = dir.path().join(SYSCTL_FILE);
        let content = std::fs::read_to_string(&path).unwrap();
        let reread = read_drop_in(&path);
        std::env::remove_var("XZRAM_ETC_ROOT");

        assert!(content.contains("vm.dirty_bytes = 268435456"));
        assert!(content.contains("vm.vfs_cache_pressure = 50"));
        assert_eq!(reread, recommended_defaults());
    }
}
