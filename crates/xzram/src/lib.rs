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
