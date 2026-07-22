use std::path::PathBuf;

use super::types::SNAPSHOTS_DIR;
use crate::apply::pending::data_dir;

pub(crate) const ZRAM_CONF: &str = "systemd/zram-generator.conf";
pub(crate) const FSTAB: &str = "fstab";
pub(crate) const SYSCTL_FILE: &str = "sysctl.d/99-xzram.conf";
pub(crate) const ZRAMSWAP_FILE: &str = "default/zramswap";

pub fn etc_root() -> PathBuf {
    std::env::var("XZRAM_ETC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/etc"))
}

pub fn snapshots_root() -> PathBuf {
    data_dir().join(SNAPSHOTS_DIR)
}

pub(crate) fn index_path() -> PathBuf {
    snapshots_root().join("index.json")
}

pub(crate) fn etc_path(relative: &str) -> PathBuf {
    etc_root().join(relative)
}

pub(crate) fn managed_etc_files() -> [(&'static str, &'static str); 4] {
    [
        (ZRAM_CONF, "zram-generator.conf"),
        (FSTAB, "fstab"),
        (SYSCTL_FILE, "99-xzram.conf"),
        (ZRAMSWAP_FILE, "zramswap"),
    ]
}
