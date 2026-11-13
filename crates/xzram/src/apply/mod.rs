//! Privileged apply pipeline: pending staging, apply engine, and helper commands.

pub mod commands;
pub mod drain;
pub mod engine;
#[cfg(test)]
mod engine_tests;
pub mod guard;
pub mod pending;
pub mod store_lock;
pub mod types;

pub use commands::*;
pub use drain::{drain_overflow, plan_drain, DrainPlan, DrainTarget};
pub use engine::*;
pub use guard::{ApplyOptions, MemoryCheck, MEMORY_REFUSAL};
pub use pending::*;
pub use store_lock::{lock_helper_run, with_store_lock, HelperRunLock};
pub use types::*;

#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    crate::test_env_lock()
}
