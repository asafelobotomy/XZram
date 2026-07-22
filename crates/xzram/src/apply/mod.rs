//! Privileged apply pipeline: pending staging, apply engine, and helper commands.

pub mod commands;
pub mod engine;
pub mod pending;
pub mod types;

pub use commands::*;
pub use engine::*;
pub use pending::*;
pub use types::*;

#[cfg(test)]
pub(crate) fn test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
