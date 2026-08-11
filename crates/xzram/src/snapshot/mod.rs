//! Configuration snapshots: create, list, restore, and prune.

pub mod create;
pub mod index;
pub mod paths;
pub mod restore;
pub mod types;

pub use create::*;
pub use index::*;
pub use paths::*;
pub use restore::*;
pub use types::*;

#[cfg(test)]
pub(crate) fn test_lock() -> &'static std::sync::Mutex<()> {
    crate::test_env_lock()
}
