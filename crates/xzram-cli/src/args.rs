use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xzram", about = "Cross-distro Linux swap management", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,

    /// Route privileged operations via D-Bus daemon when available
    #[arg(long, global = true)]
    pub dbus: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show swap and zram status
    Status,
    /// Detect distro and backend configuration
    Detect,
    /// Run health checks and diagnostics
    Doctor,
    /// ZRAM management
    Zram {
        #[command(subcommand)]
        command: ZramCommands,
    },
    /// Swap file management
    Swapfile {
        #[command(subcommand)]
        command: SwapfileCommands,
    },
    /// Swap device control
    Swap {
        #[command(subcommand)]
        command: SwapCommands,
    },
    /// Sysctl tuning
    Sysctl {
        #[command(subcommand)]
        command: SysctlCommands,
    },
    /// Show or clear staged configuration
    Pending {
        #[command(subcommand)]
        command: PendingCommands,
    },
    /// XZram D-Bus daemon control
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    /// Apply pending configuration changes
    Apply,
    /// Recommended system defaults
    Defaults {
        #[command(subcommand)]
        command: DefaultsCommands,
    },
    /// Restore last known-good configuration
    Rollback,
    /// Configuration snapshots
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommands,
    },
}

#[derive(Subcommand)]
pub enum SnapshotCommands {
    /// List stored snapshots
    List,
    /// Create a manual snapshot
    Create {
        #[arg(long)]
        label: Option<String>,
    },
    /// Restore a snapshot
    Restore {
        /// Snapshot id, `latest`, or `last-apply`
        id: String,
    },
    /// Delete a snapshot (destructive)
    Delete {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Prune old snapshots, keeping the newest N
    Prune {
        #[arg(long, default_value_t = xzram::snapshot::DEFAULT_KEEP)]
        keep: usize,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum DefaultsCommands {
    /// Review hardware-aware recommended defaults (does not stage)
    Recommend,
    /// Stage recommended defaults for review
    Stage,
    /// Stage and apply recommended defaults
    Apply {
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Enable and start xzramd on the system bus
    Start,
}

#[derive(Subcommand)]
pub enum PendingCommands {
    /// Show staged configuration
    Show,
    /// Clear staged configuration
    Clear,
}

#[derive(Subcommand)]
pub enum ZramCommands {
    /// Show current zram configuration
    Show,
    /// Set zram configuration
    Set {
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        size: Option<String>,
        #[arg(long)]
        algorithm: Option<String>,
        #[arg(long)]
        priority: Option<i32>,
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
    /// Disable zram
    Disable {
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
    /// Migrate from legacy zram-tools configuration
    Migrate {
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
}

#[derive(Subcommand)]
pub enum SwapfileCommands {
    /// List swap files from fstab
    List,
    /// Create a new swap file
    Create {
        path: String,
        #[arg(long)]
        size_mb: u64,
        #[arg(long, default_value = "10")]
        priority: i32,
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
    /// Resize an existing swap file
    Resize {
        path: String,
        #[arg(long)]
        size_mb: u64,
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
    /// Remove a swap file
    Remove {
        path: String,
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
    /// Check btrfs nodatacow readiness for a swapfile path
    Check { path: String },
    /// Set nodatacow on btrfs parent directory (chattr +C)
    Prepare {
        path: String,
        /// Create parent directories if missing
        #[arg(long)]
        mkdir: bool,
    },
}

#[derive(Subcommand)]
pub enum SwapCommands {
    /// List active and configured swap devices
    List,
    /// Activate swap
    On { device: String },
    /// Deactivate swap
    Off { device: String },
}

#[derive(Subcommand)]
pub enum SysctlCommands {
    /// Show current sysctl values
    Show,
    /// Set sysctl values
    Set {
        #[arg(long)]
        swappiness: Option<u32>,
        #[arg(long)]
        watermark_boost_factor: Option<u32>,
        #[arg(long)]
        watermark_scale_factor: Option<u32>,
        #[arg(long)]
        page_cluster: Option<u32>,
        /// Apply recommended zram tuning defaults
        #[arg(long)]
        zram_defaults: bool,
        /// Apply immediately instead of staging
        #[arg(long)]
        now: bool,
    },
}
