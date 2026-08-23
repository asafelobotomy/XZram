pub mod apply;
pub mod backend;
pub mod checks;
pub mod config;
pub mod detect;
pub mod doctor;
pub mod error;
pub mod migrate;
pub mod recommend;
pub mod snapshot;
pub mod status;
pub mod swap_partition;
pub mod swapfile_btrfs;
pub mod sysctl;
pub mod validation;

pub use error::{Result, XzramError};

/// Serializes unit tests that mutate `XZRAM_DATA_DIR` / `XZRAM_ETC_ROOT`.
/// Recovers from a poisoned mutex so one panicking test does not cascade.
#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
