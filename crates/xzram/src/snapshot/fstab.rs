//! fstab helpers for snapshot restore: swap-entry merging and swapfile discovery.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::time::{Duration, UNIX_EPOCH};

use super::paths::{etc_path, FSTAB};
use super::types::SnapshotMeta;
use crate::error::Result;

/// True for an active (non-comment) fstab entry whose type field is `swap`.
pub(crate) fn is_swap_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    trimmed.split_whitespace().nth(2) == Some("swap")
}

/// True when an fstab/`/proc/swaps` source names a regular file rather than a device.
pub(crate) fn is_swapfile_source(source: &str) -> bool {
    source.starts_with('/') && !source.starts_with("/dev/")
}

/// Swapfile entries in `content` as `(path, options)`; skips devices, UUIDs and labels.
pub(crate) fn swapfile_entries(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter(|line| is_swap_line(line))
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let source = parts[0];
            is_swapfile_source(source).then(|| {
                (
                    source.to_string(),
                    parts.get(3).copied().unwrap_or("defaults").to_string(),
                )
            })
        })
        .collect()
}

/// Keep every non-swap line of `current` and replace its swap entries with those of
/// `snapshot`, so mounts added after the snapshot survive a restore.
pub(crate) fn merge_swap_entries(current: &str, snapshot: &str) -> String {
    let mut lines: Vec<&str> = current.lines().filter(|l| !is_swap_line(l)).collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.extend(snapshot.lines().filter(|l| is_swap_line(l)));
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// File-backed swaps currently listed in `/proc/swaps`.
pub(crate) fn active_file_swaps() -> Vec<String> {
    let Ok(content) = std::fs::read_to_string("/proc/swaps") else {
        return vec![];
    };
    parse_proc_swaps_files(&content)
}

fn parse_proc_swaps_files(content: &str) -> Vec<String> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let kind = parts.next()?;
            (kind == "file" && is_swapfile_source(name)).then(|| name.to_string())
        })
        .collect()
}

/// Swapfiles present now but absent from the snapshot.
///
/// Sources: swapfile entries in the current fstab, plus (non-hermetic) file swaps active in
/// `/proc/swaps`. An active swap missing from fstab is only treated as ours when the file
/// is newer than the snapshot, so a manually activated pre-existing swapfile is never deleted.
pub(crate) fn extra_swapfiles(meta: &SnapshotMeta, hermetic: bool) -> Result<Vec<String>> {
    let snapshot_paths: HashSet<&str> = meta.swapfiles.iter().map(|s| s.path.as_str()).collect();
    let fstab = etc_path(FSTAB);
    let fstab_paths: BTreeSet<String> = if fstab.try_exists()? {
        swapfile_entries(&fs::read_to_string(&fstab)?)
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    } else {
        BTreeSet::new()
    };

    let mut extra: BTreeSet<String> = fstab_paths
        .iter()
        .filter(|p| !snapshot_paths.contains(p.as_str()))
        .cloned()
        .collect();

    if !hermetic {
        let snapshot_secs = meta.created_at.parse::<u64>().ok();
        for path in active_file_swaps() {
            if snapshot_paths.contains(path.as_str()) || fstab_paths.contains(&path) {
                continue;
            }
            if file_newer_than(&path, snapshot_secs) {
                extra.insert(path);
            }
        }
    }
    Ok(extra.into_iter().collect())
}

fn file_newer_than(path: &str, epoch_secs: Option<u64>) -> bool {
    let Some(secs) = epoch_secs else {
        return false;
    };
    let threshold = UNIX_EPOCH + Duration::from_secs(secs);
    fs::metadata(path)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .map(|t| t >= threshold)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_mounts_and_replaces_swap() {
        let current = "# header\nUUID=a / ext4 defaults 0 1\n/swap/new none swap sw 0 0\n\
                       UUID=b /data ext4 defaults 0 2\n";
        let snapshot = "UUID=a / ext4 defaults 0 1\n/swapfile none swap sw,pri=10 0 0\n";
        let merged = merge_swap_entries(current, snapshot);
        assert_eq!(
            merged,
            "# header\nUUID=a / ext4 defaults 0 1\nUUID=b /data ext4 defaults 0 2\n\
             /swapfile none swap sw,pri=10 0 0\n"
        );
    }

    #[test]
    fn merge_with_no_snapshot_swap_drops_swap_lines() {
        let merged = merge_swap_entries("/swapfile none swap sw 0 0\n", "");
        assert_eq!(merged, "");
    }

    #[test]
    fn swapfile_entries_skip_devices_and_comments() {
        let content = "# /old none swap sw 0 0\n/dev/sda2 none swap sw 0 0\n\
                       UUID=x none swap sw 0 0\n/swapfile none swap pri=5 0 0\n";
        assert_eq!(
            swapfile_entries(content),
            vec![("/swapfile".to_string(), "pri=5".to_string())]
        );
    }

    #[test]
    fn proc_swaps_files_only() {
        let content = "Filename\tType\tSize\tUsed\tPriority\n\
                       /dev/zram0 partition 100 0 100\n\
                       /swap/swapfile file 200 0 10\n";
        assert_eq!(parse_proc_swaps_files(content), vec!["/swap/swapfile"]);
    }
}
