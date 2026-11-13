//! Btrfs swapfiles vs snapshots.
//!
//! The kernel refuses to snapshot a btrfs subvolume that holds an active swapfile
//! ("BTRFS warning: cannot snapshot subvolume with active swapfile", ETXTBSY), so a swapfile
//! in the root subvolume silently breaks snapper timelines, snap-pac pre/post snapshots and
//! Timeshift. Swap belongs in a dedicated subvolume (conventionally top-level `@swap`
//! mounted at `/swap`), which snapshots of `/` do not include.
//!
//! Detection uses only `stat`: a btrfs subvolume root is the directory with inode 256 on
//! the subvolume's own device. Snapper configs name the subvolumes they snapshot.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, XzramError};

/// Inode number of every btrfs subvolume's root directory (BTRFS_FIRST_FREE_OBJECTID).
pub const SUBVOLUME_ROOT_INO: u64 = 256;

/// Why a subvolume counts as snapshotted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SnapshotConflict {
    /// The subvolume mounted at `/` (snapper `root`, snap-pac, Timeshift).
    RootSubvolume,
    /// The subvolume mounted at `/home` (Timeshift and snapper `home` configs).
    HomeSubvolume,
    /// A snapper config lists this subvolume as `SUBVOLUME=`.
    SnapperConfig { config: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapfilePlacement {
    pub path: String,
    /// Mount-namespace path of the subvolume holding `path`.
    pub subvolume: String,
    pub conflict: SnapshotConflict,
    /// fstab spec of the btrfs filesystem (`UUID=...`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

impl SwapfilePlacement {
    pub fn message(&self) -> String {
        let why = match &self.conflict {
            SnapshotConflict::RootSubvolume => "the root subvolume".to_string(),
            SnapshotConflict::HomeSubvolume => "the /home subvolume".to_string(),
            SnapshotConflict::SnapperConfig { config } => {
                format!("snapshotted by snapper config '{config}'")
            }
        };
        format!(
            "Swapfile {} is in the btrfs subvolume at {} ({why}); btrfs refuses to snapshot a \
             subvolume with an active swapfile (\"cannot snapshot subvolume with active \
             swapfile\"), so snapper, snap-pac and Timeshift snapshots of it fail",
            self.path, self.subvolume
        )
    }

    /// [`fix_hint`] with this filesystem's device and the swapfile path filled in.
    pub fn fix(&self) -> String {
        let device = self.device.as_deref().unwrap_or("<device>");
        fix_hint()
            .replace("<device>", device)
            .replace("<path>", &self.path)
    }
}

/// How to move swap into its own subvolume.
pub fn fix_hint() -> &'static str {
    "Put swap in a dedicated subvolume: 'xzram swapfile remove <path>', mount the btrfs top \
     level ('mount -o subvolid=5 <device> /mnt'), 'btrfs subvolume create /mnt/@swap', \
     'umount /mnt', add '<device> /swap btrfs subvol=/@swap,noatime 0 0' to /etc/fstab, \
     'mount /swap', then recreate the swapfile there ('xzram defaults apply'). See \
     docs/RECOMMENDATIONS.md#btrfs-swap-subvolume"
}

/// Nearest ancestor of `path` (inclusive; starting at the first one that exists) that is a
/// btrfs subvolume root. `stat` returns `(st_dev, st_ino)`.
pub fn subvolume_root_with(
    path: &Path,
    stat: impl Fn(&Path) -> Option<(u64, u64)>,
) -> Option<PathBuf> {
    let start = path.ancestors().find(|p| stat(p).is_some())?;
    let (dev, _) = stat(start)?;
    for p in start.ancestors() {
        let (d, ino) = stat(p)?;
        if d != dev {
            return None;
        }
        if ino == SUBVOLUME_ROOT_INO {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// `SUBVOLUME=` from one snapper config file.
pub fn parse_snapper_subvolume(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let value = line.trim().strip_prefix("SUBVOLUME=")?;
        let value = value.trim().trim_matches('"').trim_matches('\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// `(config name, subvolume)` for every snapper config.
pub fn snapper_subvolumes() -> Vec<(String, String)> {
    let dir = crate::snapshot::paths::etc_path("snapper/configs");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let content = std::fs::read_to_string(e.path()).ok()?;
            Some((
                e.file_name().to_string_lossy().into_owned(),
                parse_snapper_subvolume(&content)?,
            ))
        })
        .collect();
    out.sort();
    out
}

/// Whether the subvolume at `subvolume` gets snapshotted.
pub fn conflict_for(subvolume: &str, snapper: &[(String, String)]) -> Option<SnapshotConflict> {
    let normalized = subvolume.trim_end_matches('/');
    let normalized = if normalized.is_empty() {
        "/"
    } else {
        normalized
    };
    if let Some((config, _)) = snapper
        .iter()
        .find(|(_, s)| s.trim_end_matches('/') == normalized.trim_end_matches('/'))
    {
        return Some(SnapshotConflict::SnapperConfig {
            config: config.clone(),
        });
    }
    match normalized {
        "/" => Some(SnapshotConflict::RootSubvolume),
        "/home" => Some(SnapshotConflict::HomeSubvolume),
        _ => None,
    }
}

/// Pure placement check over injected probes (`subvolume` of the path, snapper configs).
pub fn placement_for(
    path: &str,
    subvolume: Option<&Path>,
    snapper: &[(String, String)],
) -> Option<SwapfilePlacement> {
    let subvolume = subvolume?.to_string_lossy().into_owned();
    let conflict = conflict_for(&subvolume, snapper)?;
    Some(SwapfilePlacement {
        path: path.into(),
        subvolume,
        conflict,
        device: None,
    })
}

fn live_stat(p: &Path) -> Option<(u64, u64)> {
    std::fs::metadata(p).ok().map(|m| (m.dev(), m.ino()))
}

/// `(fstype, UUID)` of the filesystem holding `path` (or its nearest existing ancestor).
fn filesystem_of(path: &Path) -> Option<(String, Option<String>)> {
    let existing = path.ancestors().find(|p| p.exists())?;
    let out = std::process::Command::new("findmnt")
        .args(["-nro", "FSTYPE,UUID", "-T"])
        .arg(existing)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut fields = text.lines().next()?.split_whitespace();
    let fstype = fields.next()?.to_string();
    Some((fstype, fields.next().map(|u| format!("UUID={u}"))))
}

/// Live check for a swapfile at `path` (existing or planned). `None` when it is not on
/// btrfs or its subvolume is not snapshotted.
pub fn swapfile_placement(path: &str) -> Option<SwapfilePlacement> {
    let p = Path::new(path);
    let (fstype, device) = filesystem_of(p)?;
    if fstype != "btrfs" {
        return None;
    }
    let subvolume = subvolume_root_with(p, live_stat);
    let mut placement = placement_for(path, subvolume.as_deref(), &snapper_subvolumes())?;
    placement.device = device;
    Some(placement)
}

/// Refuse to create a swapfile where it would block snapshots.
pub fn ensure_swapfile_placement(path: &str) -> Result<()> {
    match swapfile_placement(path) {
        None => Ok(()),
        Some(placement) => Err(XzramError::Validation(format!(
            "{}. Refusing to create it there. {}",
            placement.message(),
            placement.fix()
        ))),
    }
}

/// First of `paths` that is missing (per `exists`) and would be created in a snapshotted
/// subvolume (per `probe`). Existing files are not this check's concern; doctor reports them.
fn first_new_conflict<'a>(
    paths: impl IntoIterator<Item = &'a str>,
    exists: impl Fn(&str) -> bool,
    probe: impl Fn(&str) -> Option<SwapfilePlacement>,
) -> Option<SwapfilePlacement> {
    paths.into_iter().filter(|p| !exists(p)).find_map(probe)
}

/// Snapshot restore: refuse up front, before anything changes, when it would recreate a
/// missing swapfile where it blocks btrfs snapshots.
pub fn ensure_restore_swapfiles_placement<'a>(
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
    match first_new_conflict(paths, |p| Path::new(p).exists(), swapfile_placement) {
        None => Ok(()),
        Some(placement) => Err(XzramError::Validation(format!(
            "{}. Restoring this snapshot would recreate it there; nothing was changed. {}",
            placement.message(),
            placement.fix()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// `/` is subvolume @ (dev 40); `/swap` a plain directory in it; `/srv/swap` a nested
    /// subvolume (dev 41); `/home` subvolume @home (dev 42).
    fn fake_stat(p: &Path) -> Option<(u64, u64)> {
        let table: HashMap<&str, (u64, u64)> = HashMap::from([
            ("/", (40, 256)),
            ("/swap", (40, 1234)),
            ("/swap/swapfile", (40, 1235)),
            ("/srv", (40, 900)),
            ("/srv/swap", (41, 256)),
            ("/srv/swap/swapfile", (41, 257)),
            ("/home", (42, 256)),
            ("/home/me", (42, 300)),
        ]);
        table.get(p.to_str()?).copied()
    }

    #[test]
    fn finds_subvolume_roots() {
        let root = |p: &str| subvolume_root_with(Path::new(p), fake_stat);
        assert_eq!(root("/swap/swapfile"), Some(PathBuf::from("/")));
        // Planned file in a missing directory: walks up to `/`.
        assert_eq!(root("/swap/new/swapfile"), Some(PathBuf::from("/")));
        assert_eq!(root("/srv/swap/swapfile"), Some(PathBuf::from("/srv/swap")));
        assert_eq!(root("/home/me/swapfile"), Some(PathBuf::from("/home")));
    }

    #[test]
    fn issue_layout_swapfile_in_root_subvolume_conflicts() {
        let sub = subvolume_root_with(Path::new("/swap/swapfile"), fake_stat);
        let placement = placement_for("/swap/swapfile", sub.as_deref(), &[]).unwrap();
        assert_eq!(placement.conflict, SnapshotConflict::RootSubvolume);
        let msg = placement.message();
        assert!(
            msg.contains("cannot snapshot subvolume with active swapfile"),
            "{msg}"
        );
        assert!(fix_hint().contains("@swap"));
        let fix = SwapfilePlacement {
            device: Some("UUID=abc".into()),
            ..placement
        }
        .fix();
        assert!(fix.contains("UUID=abc /swap btrfs subvol=/@swap"), "{fix}");
        assert!(
            fix.contains("xzram swapfile remove /swap/swapfile"),
            "{fix}"
        );
    }

    #[test]
    fn dedicated_subvolume_is_fine_unless_snapper_tracks_it() {
        let sub = subvolume_root_with(Path::new("/srv/swap/swapfile"), fake_stat);
        assert!(placement_for("/srv/swap/swapfile", sub.as_deref(), &[]).is_none());
        let snapper = vec![("srv".to_string(), "/srv/swap".to_string())];
        let placement = placement_for("/srv/swap/swapfile", sub.as_deref(), &snapper).unwrap();
        assert_eq!(
            placement.conflict,
            SnapshotConflict::SnapperConfig {
                config: "srv".into()
            }
        );
        // Unknown subvolume (stat failed): no claim either way.
        assert!(placement_for("/x", None, &snapper).is_none());
    }

    #[test]
    fn restore_check_only_flags_missing_swapfiles_in_snapshotted_subvolumes() {
        let probe = |p: &str| {
            let sub = subvolume_root_with(Path::new(p), fake_stat);
            placement_for(p, sub.as_deref(), &[])
        };
        let paths = ["/srv/swap/swapfile", "/swap/swapfile"];
        // Both missing: the one in `/` (subvolume @) is refused, the nested subvolume is fine.
        let hit = first_new_conflict(paths, |_| false, probe).unwrap();
        assert_eq!(hit.path, "/swap/swapfile");
        assert_eq!(hit.conflict, SnapshotConflict::RootSubvolume);
        // Already present (restore leaves it alone; doctor reports it): no refusal.
        assert!(first_new_conflict(paths, |p| p == "/swap/swapfile", probe).is_none());
    }

    #[test]
    fn root_and_home_conflict_by_default() {
        assert_eq!(
            conflict_for("/", &[]),
            Some(SnapshotConflict::RootSubvolume)
        );
        assert_eq!(
            conflict_for("/home/", &[]),
            Some(SnapshotConflict::HomeSubvolume)
        );
        assert_eq!(conflict_for("/swap", &[]), None);
        let snapper = vec![("root".to_string(), "/".to_string())];
        assert_eq!(
            conflict_for("/", &snapper),
            Some(SnapshotConflict::SnapperConfig {
                config: "root".into()
            })
        );
    }

    #[test]
    fn parses_snapper_configs() {
        let cfg = "# subvolume to snapshot\nSUBVOLUME=\"/\"\nFSTYPE=\"btrfs\"\n";
        assert_eq!(parse_snapper_subvolume(cfg), Some("/".into()));
        assert_eq!(
            parse_snapper_subvolume("SUBVOLUME=/home\n"),
            Some("/home".into())
        );
        assert_eq!(parse_snapper_subvolume("FSTYPE=\"btrfs\"\n"), None);
    }
}
