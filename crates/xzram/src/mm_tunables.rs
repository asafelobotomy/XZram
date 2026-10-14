//! Kernel memory-management knobs that live in sysfs rather than sysctl: MGLRU thrashing
//! prevention and THP defrag policy.
//!
//! Like zswap, they are persisted with a tmpfiles.d `w-` drop-in (written early at boot on
//! any systemd distro) and also written to sysfs for the running kernel. Snapshots record
//! the runtime values, so restore removes the drop-in and writes the old values back.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, XzramError};
use crate::snapshot::paths::etc_path;

pub(crate) const MM_TMPFILES: &str = "tmpfiles.d/xzram-mm.conf";
const MGLRU_ENABLED: &str = "/sys/kernel/mm/lru_gen/enabled";
const MGLRU_MIN_TTL: &str = "/sys/kernel/mm/lru_gen/min_ttl_ms";
const THP_ENABLED: &str = "/sys/kernel/mm/transparent_hugepage/enabled";
const THP_DEFRAG: &str = "/sys/kernel/mm/transparent_hugepage/defrag";

/// Kernel-doc value: protects ~1 s of working set, "usually eliminates intolerable janks".
pub const RECOMMENDED_MIN_TTL_MS: u32 = 1000;
/// Background compaction, direct compaction only for MADV_HUGEPAGE regions (CachyOS).
pub const RECOMMENDED_THP_DEFRAG: &str = "defer+madvise";
const THP_DEFRAG_MODES: [&str; 5] = ["always", "defer", "defer+madvise", "madvise", "never"];
const MAX_MIN_TTL_MS: u32 = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MmTunables {
    /// `/sys/kernel/mm/lru_gen/min_ttl_ms` (0 disables thrashing prevention).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mglru_min_ttl_ms: Option<u32>,
    /// `/sys/kernel/mm/transparent_hugepage/defrag` mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thp_defrag: Option<String>,
}

impl MmTunables {
    pub fn is_empty(&self) -> bool {
        self.mglru_min_ttl_ms.is_none() && self.thp_defrag.is_none()
    }

    /// Keys set in `other` win.
    pub fn merged(&self, other: &MmTunables) -> MmTunables {
        MmTunables {
            mglru_min_ttl_ms: other.mglru_min_ttl_ms.or(self.mglru_min_ttl_ms),
            thp_defrag: other.thp_defrag.clone().or_else(|| self.thp_defrag.clone()),
        }
    }

    fn entries(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(ms) = self.mglru_min_ttl_ms {
            out.push((MGLRU_MIN_TTL, ms.to_string()));
        }
        if let Some(ref mode) = self.thp_defrag {
            out.push((THP_DEFRAG, mode.clone()));
        }
        out
    }
}

pub fn validate(t: &MmTunables) -> Result<()> {
    if let Some(ms) = t.mglru_min_ttl_ms.filter(|ms| *ms > MAX_MIN_TTL_MS) {
        return Err(XzramError::Validation(format!(
            "lru_gen min_ttl_ms must be 0–{MAX_MIN_TTL_MS} (got {ms})"
        )));
    }
    if let Some(mode) = t
        .thp_defrag
        .as_deref()
        .filter(|m| !THP_DEFRAG_MODES.contains(m))
    {
        return Err(XzramError::Validation(format!(
            "THP defrag must be one of {} (got {mode})",
            THP_DEFRAG_MODES.join(", ")
        )));
    }
    Ok(())
}

/// Live kernel values; `None` per field when the knob is absent.
pub fn current() -> MmTunables {
    MmTunables {
        mglru_min_ttl_ms: read_sysfs(MGLRU_MIN_TTL).and_then(|v| v.parse().ok()),
        thp_defrag: read_sysfs(THP_DEFRAG).and_then(|v| bracketed(&v)),
    }
}

/// MGLRU is built in and its main switch (bit 0x0001) is on.
pub fn mglru_enabled() -> bool {
    read_sysfs(MGLRU_ENABLED)
        .and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok())
        .is_some_and(|bits| bits & 1 == 1)
}

/// Active THP mode (`always` / `madvise` / `never`).
pub fn thp_enabled_mode() -> Option<String> {
    read_sysfs(THP_ENABLED).and_then(|v| bracketed(&v))
}

fn read_sysfs(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// `always defer [madvise] never` → `madvise`; plain values pass through.
fn bracketed(raw: &str) -> Option<String> {
    match (raw.find('['), raw.find(']')) {
        (Some(a), Some(b)) if a < b => Some(raw[a + 1..b].to_string()),
        _ => (!raw.is_empty() && !raw.contains(' ')).then(|| raw.to_string()),
    }
}

fn read_drop_in() -> MmTunables {
    let content = std::fs::read_to_string(etc_path(MM_TMPFILES)).unwrap_or_default();
    let mut t = MmTunables::default();
    for line in content.lines().filter(|l| l.starts_with("w")) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(path), Some(value)) = (fields.get(1), fields.get(6)) else {
            continue;
        };
        match *path {
            MGLRU_MIN_TTL => t.mglru_min_ttl_ms = value.parse().ok(),
            THP_DEFRAG => t.thp_defrag = Some(value.to_string()),
            _ => {}
        }
    }
    t
}

/// Merge `wanted` into the drop-in, then write the values to the running kernel.
pub fn apply(wanted: &MmTunables) -> Result<Vec<String>> {
    validate(wanted)?;
    let merged = read_drop_in().merged(wanted);
    let mut content = String::from("# Managed by xzram: kernel memory-management tunables\n");
    for (path, value) in merged.entries() {
        content.push_str(&format!("w- {path} - - - - {value}\n"));
    }
    let path = etc_path(MM_TMPFILES);
    crate::fsutil::atomic_write(&path, content.as_bytes(), 0o644)?;
    let mut messages = vec![format!("Persisted memory tunables via {}", path.display())];

    // Skip live sysfs writes under hermetic etc roots (unit tests / fixtures).
    if std::env::var_os("XZRAM_ETC_ROOT").is_none() {
        messages.extend(write_runtime(wanted)?);
    }
    Ok(messages)
}

fn write_runtime(t: &MmTunables) -> Result<Vec<String>> {
    let mut messages = Vec::new();
    for (path, value) in t.entries() {
        if Path::new(path).exists() {
            std::fs::write(path, &value)?;
            messages.push(format!("Set {path} = {value}"));
        }
    }
    Ok(messages)
}

/// After restore removed or replaced the drop-in, put back the snapshot's runtime values
/// for any knob the restored drop-in no longer sets.
pub(crate) fn restore_runtime(snapshot: Option<&MmTunables>, messages: &mut Vec<String>) {
    let Some(snapshot) = snapshot else {
        return;
    };
    let persisted = read_drop_in();
    let live = current();
    let revert = MmTunables {
        mglru_min_ttl_ms: snapshot
            .mglru_min_ttl_ms
            .filter(|v| persisted.mglru_min_ttl_ms.is_none() && live.mglru_min_ttl_ms != Some(*v)),
        thp_defrag: snapshot
            .thp_defrag
            .clone()
            .filter(|v| persisted.thp_defrag.is_none() && live.thp_defrag.as_deref() != Some(v)),
    };
    if let Ok(done) = write_runtime(&revert) {
        messages.extend(done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bracketed_sysfs_modes() {
        assert_eq!(
            bracketed("always defer defer+madvise [madvise] never").as_deref(),
            Some("madvise")
        );
        assert_eq!(
            bracketed("[always] madvise never").as_deref(),
            Some("always")
        );
        assert_eq!(bracketed("1000").as_deref(), Some("1000"));
        assert_eq!(bracketed(""), None);
    }

    #[test]
    fn validates_ranges_and_modes() {
        let ok = MmTunables {
            mglru_min_ttl_ms: Some(RECOMMENDED_MIN_TTL_MS),
            thp_defrag: Some(RECOMMENDED_THP_DEFRAG.into()),
        };
        assert!(validate(&ok).is_ok());
        assert!(validate(&MmTunables {
            mglru_min_ttl_ms: Some(60_000),
            ..Default::default()
        })
        .is_err());
        assert!(validate(&MmTunables {
            thp_defrag: Some("sometimes".into()),
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn apply_merges_drop_in_under_etc_root() {
        let _guard = crate::test_env_lock();
        let etc = tempfile::tempdir().unwrap();
        std::env::set_var("XZRAM_ETC_ROOT", etc.path());

        apply(&MmTunables {
            mglru_min_ttl_ms: Some(1000),
            ..Default::default()
        })
        .unwrap();
        let messages = apply(&MmTunables {
            thp_defrag: Some("defer+madvise".into()),
            ..Default::default()
        })
        .unwrap();
        let content = std::fs::read_to_string(etc.path().join(MM_TMPFILES)).unwrap();
        let reread = read_drop_in();
        std::env::remove_var("XZRAM_ETC_ROOT");

        assert!(content.contains("w- /sys/kernel/mm/lru_gen/min_ttl_ms - - - - 1000"));
        assert!(
            content.contains("w- /sys/kernel/mm/transparent_hugepage/defrag - - - - defer+madvise")
        );
        assert_eq!(reread.mglru_min_ttl_ms, Some(1000));
        assert_eq!(messages.len(), 1, "hermetic run must not touch sysfs");
    }
}
