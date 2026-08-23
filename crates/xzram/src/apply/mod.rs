//! Privileged apply pipeline: pending staging, apply engine, and helper commands.

pub mod commands;
pub mod engine;
pub mod pending;
pub mod store_lock;
pub mod types;

pub use commands::*;
pub use engine::*;
pub use pending::*;
pub use store_lock::with_store_lock;
pub use types::*;

#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    crate::test_env_lock()
}
