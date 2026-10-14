//! Disk-swap placement: detect when swap would land on a rotational (HDD) device.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `Some(true)` when the block device backing `path` (or its nearest existing ancestor) is
/// rotational. `None` when it can't be determined (tmpfs, network fs, missing sysfs, ...).
pub fn is_rotational_path(path: &str) -> Option<bool> {
    let mut probe = PathBuf::from(path);
    while !probe.exists() {
        if !probe.pop() {
            return None;
        }
    }
    let output = Command::new("findmnt")
        .args(["-nvo", "SOURCE", "--target"])
        .arg(&probe)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let source = String::from_utf8_lossy(&output.stdout);
    is_rotational_device(source.lines().next()?.trim())
}

/// `true` when the swap entry name from `/proc/swaps` lives on a rotational device.
/// Handles both swap partitions (`/dev/sdXN`) and swapfiles.
pub fn is_rotational_swap(name: &str) -> Option<bool> {
    if name.starts_with("/dev/") {
        is_rotational_device(name)
    } else {
        is_rotational_path(name)
    }
}

fn is_rotational_device(dev: &str) -> Option<bool> {
    // btrfs subvolume mounts report `/dev/nvme0n1p2[/@swap]`.
    let dev = dev.split('[').next()?;
    let name = fs::canonicalize(dev).ok()?.file_name()?.to_owned();
    let sys = fs::canonicalize(Path::new("/sys/class/block").join(name)).ok()?;
    // Partitions carry no queue/ of their own; read the parent disk's.
    let disk = if sys.join("partition").exists() {
        sys.parent()?.to_path_buf()
    } else {
        sys
    };
    let flag = fs::read_to_string(disk.join("queue/rotational")).ok()?;
    Some(flag.trim() == "1")
}
