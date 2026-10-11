//! Process exit codes for the `xzram` CLI (stable; consumed by the Qt GUI and scripts).
//!
//! `2` (usage error) is produced by clap itself when argument parsing fails.

/// Command completed successfully.
pub(crate) const OK: u8 = 0;
/// Command failed (I/O, privileged helper, D-Bus, authorization cancelled/denied, ...).
pub(crate) const ERROR: u8 = 1;
/// `xzram doctor` ran successfully but reported the system as unhealthy.
pub(crate) const UNHEALTHY: u8 = 3;

/// Help text appended to `xzram --help`.
pub(crate) const HELP: &str = "\
Exit codes:
  0  success
  1  error (operation failed, authorization cancelled/denied, ...)
  2  usage error (invalid arguments)
  3  `xzram doctor` completed but reported the system as unhealthy";
