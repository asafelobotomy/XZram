mod dbus_client;

use std::process::Command;

use clap::{Parser, Subcommand};
use tracing::info;
use xzram::apply::{load_pending, PendingConfig, SwapfileConfig, SwapfileResizeConfig, ZramConfig};
use xzram::backend::{available_swapfile_backend, available_zram_backend};
use xzram::config::default_zram_config;
use xzram::detect;
use xzram::doctor;
use xzram::recommend;
use xzram::snapshot::{self, SnapshotTrigger};
use xzram::status::{self, format_bytes};
use xzram::swap_partition;
use xzram::swapfile_btrfs;
use xzram::sysctl::{self, SysctlValues};

#[derive(Parser)]
#[command(name = "xzram", about = "Cross-distro Linux swap management", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Route privileged operations via D-Bus daemon when available
    #[arg(long, global = true)]
    dbus: bool,
}

#[derive(Subcommand)]
enum Commands {
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
enum SnapshotCommands {
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
enum DefaultsCommands {
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
enum DaemonCommands {
    /// Enable and start xzramd on the system bus
    Start,
}

#[derive(Subcommand)]
enum PendingCommands {
    /// Show staged configuration
    Show,
    /// Clear staged configuration
    Clear,
}

#[derive(Subcommand)]
enum ZramCommands {
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
enum SwapfileCommands {
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
enum SwapCommands {
    /// List active and configured swap devices
    List,
    /// Activate swap
    On { device: String },
    /// Deactivate swap
    Off { device: String },
}

#[derive(Subcommand)]
enum SysctlCommands {
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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            let report = status::status()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_status(&report);
            }
        }
        Commands::Detect => {
            let report = detect::detect()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_detect(&report);
            }
        }
        Commands::Doctor => {
            let report = doctor::doctor()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_doctor(&report);
            }
            if !report.healthy {
                std::process::exit(1);
            }
        }
        Commands::Zram { command } => match command {
            ZramCommands::Show => {
                let backend = available_zram_backend()?;
                let config = backend.show()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&config)?);
                } else if let Some(c) = config {
                    print_zram_config(&c);
                } else {
                    println!("No zram configuration found");
                }
            }
            ZramCommands::Set {
                device,
                size,
                algorithm,
                priority,
                now,
            } => {
                let defaults = default_zram_config();
                let config = ZramConfig {
                    device: device.unwrap_or(defaults.name),
                    zram_size: Some(size.unwrap_or_else(|| {
                        defaults
                            .zram_size
                            .unwrap_or_else(|| "min(ram / 2, 4096)".into())
                    })),
                    zram_resident_limit: None,
                    compression_algorithm: Some(algorithm.unwrap_or_else(|| {
                        defaults
                            .compression_algorithm
                            .unwrap_or_else(|| "zstd".into())
                    })),
                    swap_priority: Some(priority.unwrap_or(defaults.swap_priority.unwrap_or(100))),
                    fs_type: None,
                    mount_point: None,
                };
                let pending = PendingConfig {
                    zram: Some(config.clone()),
                    ..Default::default()
                };
                if now {
                    run_privileged(cli.dbus, "zram.configure", &serde_json::to_string(&config)?)?;
                } else {
                    run_privileged(cli.dbus, "stage", &serde_json::to_string(&pending)?)?;
                    println!("Staged zram configuration; run 'xzram apply' to apply");
                }
            }
            ZramCommands::Disable { now } => {
                if now {
                    run_privileged(cli.dbus, "zram.disable", "{}")?;
                } else {
                    let pending = PendingConfig {
                        disable_zram: true,
                        ..Default::default()
                    };
                    run_privileged(cli.dbus, "stage", &serde_json::to_string(&pending)?)?;
                    println!("Staged zram disable; run 'xzram apply' to apply");
                }
            }
            ZramCommands::Migrate { now } => {
                run_privileged(cli.dbus, "zram.migrate", "{}")?;
                if now {
                    run_privileged(cli.dbus, "apply", "{}")?;
                }
            }
        },
        Commands::Swapfile { command } => match command {
            SwapfileCommands::List => {
                let backend = available_swapfile_backend();
                let files = backend.list()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&files)?);
                } else {
                    for f in &files {
                        println!("{}  {} MiB  priority {}", f.path, f.size_mb, f.priority);
                    }
                }
            }
            SwapfileCommands::Create {
                path,
                size_mb,
                priority,
                now,
            } => {
                let config = SwapfileConfig {
                    path,
                    size_mb,
                    priority,
                };
                let pending = PendingConfig {
                    swapfile: Some(config.clone()),
                    ..Default::default()
                };
                if now {
                    run_privileged(
                        cli.dbus,
                        "swapfile.create",
                        &serde_json::to_string(&config)?,
                    )?;
                } else {
                    run_privileged(cli.dbus, "stage", &serde_json::to_string(&pending)?)?;
                    println!("Staged swapfile create; run 'xzram apply' to apply");
                }
            }
            SwapfileCommands::Resize { path, size_mb, now } => {
                let resize = SwapfileResizeConfig { path, size_mb };
                let pending = PendingConfig {
                    swapfile_resize: Some(resize.clone()),
                    ..Default::default()
                };
                if now {
                    run_privileged(
                        cli.dbus,
                        "swapfile.resize",
                        &serde_json::to_string(&resize)?,
                    )?;
                } else {
                    run_privileged(cli.dbus, "stage", &serde_json::to_string(&pending)?)?;
                    println!("Staged swapfile resize; run 'xzram apply' to apply");
                }
            }
            SwapfileCommands::Remove { path, now } => {
                let pending = PendingConfig {
                    remove_swapfile: Some(path.clone()),
                    ..Default::default()
                };
                if now {
                    let payload = serde_json::json!({ "path": path });
                    run_privileged(cli.dbus, "swapfile.remove", &payload.to_string())?;
                } else {
                    run_privileged(cli.dbus, "stage", &serde_json::to_string(&pending)?)?;
                    println!("Staged swapfile remove; run 'xzram apply' to apply");
                }
            }
            SwapfileCommands::Check { path } => {
                let status = swapfile_btrfs::check_nodatacow(std::path::Path::new(&path))?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    print_nodatacow_status(&status);
                }
            }
            SwapfileCommands::Prepare { path, mkdir } => {
                let payload = serde_json::json!({
                    "path": path,
                    "mkdir_parents": mkdir,
                });
                run_privileged(cli.dbus, "swapfile.prepare", &payload.to_string())?;
                let status = swapfile_btrfs::check_nodatacow(std::path::Path::new(&path))?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    print_nodatacow_status(&status);
                }
            }
        },
        Commands::Swap { command } => match command {
            SwapCommands::List => {
                let entries = swap_partition::list_swaps_merged()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                } else {
                    for s in &entries {
                        let state = if s.active { "active" } else { "inactive" };
                        println!(
                            "{} [{}] {}  {} used / {}  priority {}  ({})",
                            s.name,
                            s.swap_type,
                            state,
                            format_bytes(s.used_bytes),
                            format_bytes(s.size_bytes),
                            s.priority,
                            format!("{:?}", s.source).to_lowercase()
                        );
                    }
                }
            }
            SwapCommands::On { device } => {
                let payload = serde_json::json!({ "action": "on", "device": device });
                run_privileged(cli.dbus, "swap.activate", &payload.to_string())?;
            }
            SwapCommands::Off { device } => {
                let payload = serde_json::json!({ "action": "off", "device": device });
                run_privileged(cli.dbus, "swap.activate", &payload.to_string())?;
            }
        },
        Commands::Sysctl { command } => match command {
            SysctlCommands::Show => {
                let values = sysctl::show()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&values)?);
                } else {
                    print_sysctl(&values);
                }
            }
            SysctlCommands::Set {
                swappiness,
                watermark_boost_factor,
                watermark_scale_factor,
                page_cluster,
                zram_defaults,
                now,
            } => {
                let values = if zram_defaults {
                    sysctl::zram_tuning_defaults()
                } else {
                    SysctlValues {
                        swappiness,
                        watermark_boost_factor,
                        watermark_scale_factor,
                        page_cluster,
                    }
                };
                let pending = PendingConfig {
                    sysctl: Some(values.clone()),
                    ..Default::default()
                };
                if now {
                    run_privileged(cli.dbus, "sysctl.set", &serde_json::to_string(&values)?)?;
                } else {
                    run_privileged(cli.dbus, "stage", &serde_json::to_string(&pending)?)?;
                    println!("Staged sysctl changes; run 'xzram apply' to apply");
                }
            }
        },
        Commands::Pending { command } => match command {
            PendingCommands::Show => {
                let pending = load_pending()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&pending)?);
                } else if let Some(p) = pending {
                    println!("{}", serde_json::to_string_pretty(&p)?);
                } else {
                    println!("No pending configuration");
                }
            }
            PendingCommands::Clear => {
                run_privileged(cli.dbus, "pending.clear", "{}")?;
            }
        },
        Commands::Daemon { command } => match command {
            DaemonCommands::Start => {
                run_privileged_pkexec("daemon.start", "{}")?;
            }
        },
        Commands::Defaults { command } => match command {
            DefaultsCommands::Recommend => {
                let report = recommend::recommend()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_recommended_defaults(&report);
                }
            }
            DefaultsCommands::Stage => {
                let report = recommend::recommend()?;
                if !pending_has_changes(&report.pending) {
                    println!("System already matches recommended defaults; nothing to stage");
                    return Ok(());
                }
                run_privileged(cli.dbus, "stage", &serde_json::to_string(&report.pending)?)?;
                println!(
                    "Recommended defaults staged; run 'xzram apply' or review tabs in the GUI"
                );
            }
            DefaultsCommands::Apply { yes } => {
                let report = recommend::recommend()?;
                if !pending_has_changes(&report.pending) {
                    println!("System already matches recommended defaults; nothing to apply");
                    return Ok(());
                }
                if !yes {
                    print_recommended_defaults(&report);
                    if !confirm_apply_defaults()? {
                        println!("Cancelled");
                        return Ok(());
                    }
                }
                run_privileged(cli.dbus, "stage", &serde_json::to_string(&report.pending)?)?;
                run_privileged(cli.dbus, "apply", "{}")?;
                println!("Recommended defaults applied");
            }
        },
        Commands::Apply => {
            run_privileged(cli.dbus, "apply", "{}")?;
        }
        Commands::Rollback => {
            run_privileged(cli.dbus, "rollback", "{}")?;
        }
        Commands::Snapshot { command } => match command {
            SnapshotCommands::List => {
                let list = snapshot::list_snapshots()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    if list.is_empty() {
                        println!("No snapshots stored");
                    } else {
                        println!("{:<40} {:<12} {}", "ID", "TRIGGER", "LABEL");
                        for s in list {
                            println!(
                                "{:<40} {:<12} {}",
                                s.id,
                                s.trigger.as_str(),
                                s.label
                            );
                        }
                    }
                }
            }
            SnapshotCommands::Create { label } => {
                let meta = if cli.dbus && dbus_client::is_available() {
                    run_snapshot_create_dbus(label.as_deref())?
                } else {
                    run_snapshot_create_pkexec(label.as_deref())?
                };
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&meta)?);
                } else {
                    println!("Created snapshot: {} ({})", meta.label, meta.id);
                }
            }
            SnapshotCommands::Restore { id } => {
                let snapshot_id = resolve_snapshot_id(&id)?;
                run_privileged(
                    cli.dbus,
                    "snapshot.restore",
                    &serde_json::json!({ "id": snapshot_id }).to_string(),
                )?;
            }
            SnapshotCommands::Delete { id, yes } => {
                if !yes && !confirm("Delete snapshot? This cannot be undone.")? {
                    println!("Cancelled");
                    return Ok(());
                }
                run_privileged(
                    cli.dbus,
                    "snapshot.delete",
                    &serde_json::json!({ "id": id }).to_string(),
                )?;
                println!("Deleted snapshot {id}");
            }
            SnapshotCommands::Prune { keep, yes } => {
                if !yes && !confirm(&format!("Prune snapshots, keeping newest {keep}?"))? {
                    println!("Cancelled");
                    return Ok(());
                }
                run_privileged(
                    cli.dbus,
                    "snapshot.prune",
                    &serde_json::json!({ "keep": keep }).to_string(),
                )?;
            }
        },
    }

    Ok(())
}

fn run_privileged_pkexec(action: &str, payload: &str) -> anyhow::Result<()> {
    let helper = find_helper()?;
    let status = Command::new("pkexec")
        .arg(&helper)
        .arg(action)
        .arg(payload)
        .status()?;

    if !status.success() {
        anyhow::bail!("privileged operation failed (pkexec exit {status})");
    }
    Ok(())
}

fn run_privileged(_use_dbus: bool, action: &str, payload: &str) -> anyhow::Result<()> {
    if _use_dbus {
        if let Err(e) = run_via_dbus(action, payload) {
            info!(error = %e, "D-Bus unavailable, falling back to pkexec");
        } else {
            return Ok(());
        }
    }

    run_privileged_pkexec(action, payload)
}

fn run_via_dbus(action: &str, payload: &str) -> anyhow::Result<()> {
    if !dbus_client::is_available() {
        anyhow::bail!("xzramd not running");
    }
    dbus_client::call(action, payload)
}

fn find_helper() -> anyhow::Result<String> {
    if let Ok(dev) = std::env::var("XZRAM_DEV_HELPER") {
        if std::path::Path::new(&dev).exists() {
            return Ok(dev);
        }
    }

    for path in [
        "/usr/libexec/xzram-helper",
        "/usr/local/libexec/xzram-helper",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xzram-helper/../../target/release/xzram-helper"
        ),
    ] {
        if std::path::Path::new(path).exists() {
            return Ok(path.into());
        }
    }

    let local_libexec = format!(
        "{}/.local/libexec/xzram-helper",
        std::env::var("HOME").unwrap_or_else(|_| "/root".into())
    );
    if std::path::Path::new(&local_libexec).exists() {
        return Ok(local_libexec);
    }

    anyhow::bail!("xzram-helper not found; install xzram or set XZRAM_DEV_HELPER")
}

fn print_status(report: &status::StatusReport) {
    println!("=== Memory ===");
    println!(
        "  Total: {}  Available: {}",
        format_bytes(report.memory.mem_total_kb * 1024),
        format_bytes(report.memory.mem_available_kb * 1024)
    );
    println!(
        "  Swap:  {} total, {} free",
        format_bytes(report.memory.swap_total_kb * 1024),
        format_bytes(report.memory.swap_free_kb * 1024)
    );

    println!("\n=== Swap devices ===");
    if report.swaps.is_empty() {
        println!("  (none)");
    }
    for s in &report.swaps {
        println!(
            "  {} [{}] {} used / {}  priority {}",
            s.name,
            s.swap_type,
            format_bytes(s.used_bytes),
            format_bytes(s.size_bytes),
            s.priority
        );
    }

    println!("\n=== ZRAM devices ===");
    if report.zram_devices.is_empty() {
        println!("  (none)");
    }
    for z in &report.zram_devices {
        let ratio = if z.data_bytes > 0 {
            z.data_bytes as f64 / z.compressed_bytes.max(1) as f64
        } else {
            0.0
        };
        println!(
            "  {}  {}  {} / {}  ratio {:.1}x  streams {}  {}",
            z.name,
            z.algorithm,
            format_bytes(z.compressed_bytes),
            format_bytes(z.disk_size_bytes),
            ratio,
            z.streams,
            z.mount_point
        );
    }
}

fn print_detect(report: &detect::DetectionReport) {
    println!("Distro:       {}", report.distro.id);
    if let Some(ref name) = report.distro.pretty_name {
        println!("Name:         {name}");
    }
    println!("Family:       {:?}", report.distro.family);
    println!("Init:         {}", report.init_system);
    println!("Pkg manager:  {:?}", report.package_manager);
    println!("ZRAM backend: {:?}", report.zram_backend);
    println!(
        "zram-gen:     {}",
        if report.zram_generator_installed {
            "installed"
        } else {
            "not installed"
        }
    );
    if let Some(ref cfg) = report.zram_generator_config {
        println!("Config:       {cfg}");
    }
    if let Some(ref fs) = report.root_filesystem {
        println!("Root FS:      {fs}");
    }
}

fn print_doctor(report: &doctor::DoctorReport) {
    if report.healthy {
        println!("System healthy");
    } else {
        println!("Issues found:");
    }
    for issue in &report.issues {
        let icon = match issue.severity {
            doctor::IssueSeverity::Info => "INFO",
            doctor::IssueSeverity::Warning => "WARN",
            doctor::IssueSeverity::Error => "ERR ",
        };
        println!("  [{icon}] {}: {}", issue.code, issue.message);
        if let Some(ref suggestion) = issue.suggestion {
            println!("         -> {suggestion}");
        }
    }
}

fn print_zram_config(c: &ZramConfig) {
    println!("Device:      {}", c.device);
    if let Some(ref s) = c.zram_size {
        println!("Size:        {s}");
    }
    if let Some(ref a) = c.compression_algorithm {
        println!("Algorithm:   {a}");
    }
    if let Some(p) = c.swap_priority {
        println!("Priority:    {p}");
    }
}

fn pending_has_changes(pending: &PendingConfig) -> bool {
    !xzram::apply::pending_is_empty(pending)
}

fn confirm_apply_defaults() -> anyhow::Result<bool> {
    confirm("Apply recommended defaults now?")
}

fn confirm(prompt: &str) -> anyhow::Result<bool> {
    use std::io::{self, Write};
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn resolve_snapshot_id(id: &str) -> anyhow::Result<String> {
    match id {
        "latest" => snapshot::list_snapshots()?
            .into_iter()
            .next()
            .map(|s| s.id)
            .ok_or_else(|| anyhow::anyhow!("no snapshots found")),
        "last-apply" => Ok(snapshot::latest_pre_apply_id()?),
        other => Ok(other.to_string()),
    }
}

fn run_snapshot_create_pkexec(label: Option<&str>) -> anyhow::Result<snapshot::SnapshotMeta> {
    let payload = serde_json::json!({
        "trigger": SnapshotTrigger::Manual.as_str(),
        "label": label,
    });
    let helper = find_helper()?;
    let output = Command::new("pkexec")
        .arg(&helper)
        .arg("snapshot.create")
        .arg(payload.to_string())
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let meta: snapshot::SnapshotMeta = serde_json::from_slice(&output.stdout)?;
    Ok(meta)
}

fn run_snapshot_create_dbus(label: Option<&str>) -> anyhow::Result<snapshot::SnapshotMeta> {
    let conn = zbus::blocking::Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "io.github.XZram1",
        "/io/github/XZram",
        "io.github.XZram.Manager",
    )?;
    let reply = proxy.call_method(
        "CreateSnapshot",
        &(SnapshotTrigger::Manual.as_str(), label.unwrap_or("")),
    )?;
    let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
        reply.body().deserialize()?;
    let json = map
        .get("json")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid CreateSnapshot response"))?;
    Ok(serde_json::from_str(&json)?)
}

fn print_recommended_defaults(report: &recommend::RecommendedDefaults) {
    println!("=== Recommended defaults for {} ===", report.context.distro);
    println!(
        "Memory: {} total, {} available",
        format_bytes(report.context.mem_total_bytes),
        format_bytes(report.context.mem_available_bytes)
    );
    println!(
        "ZRAM backend: {}  active zram: {}  disk swap: {}",
        report.context.zram_backend,
        if report.context.has_active_zram {
            "yes"
        } else {
            "no"
        },
        if report.context.has_disk_swap {
            "yes"
        } else {
            "no"
        }
    );
    if let Some(ref fs) = report.context.root_filesystem {
        println!("Root filesystem: {fs}");
    }
    println!("Profile: {}", report.context.profile);
    println!();
    for item in &report.items {
        let marker = if item.will_stage { "[stage]" } else { "[info]" };
        println!("{marker} {} — {}", item.category, item.summary);
        println!("       {}", item.detail);
        if let Some(ref reference) = item.reference {
            println!("       reference: docs/RECOMMENDATIONS.md#{reference}");
        }
    }
}

fn print_nodatacow_status(status: &swapfile_btrfs::NodatacowStatus) {
    println!("Swapfile:     {}", status.swapfile_path);
    println!("Parent dir:   {}", status.parent_directory);
    println!("Filesystem:   {}", status.filesystem);
    println!("Ready:        {}", if status.ready { "yes" } else { "no" });
    println!("{}", status.message);
}

fn print_sysctl(v: &SysctlValues) {
    if let Some(s) = v.swappiness {
        println!("vm.swappiness = {s}");
    }
    if let Some(s) = v.watermark_boost_factor {
        println!("vm.watermark_boost_factor = {s}");
    }
    if let Some(s) = v.watermark_scale_factor {
        println!("vm.watermark_scale_factor = {s}");
    }
    if let Some(s) = v.page_cluster {
        println!("vm.page-cluster = {s}");
    }
}
