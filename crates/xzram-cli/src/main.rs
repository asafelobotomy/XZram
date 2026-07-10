use std::process::Command;

use clap::{Parser, Subcommand};
use xzram::apply::{SwapfileConfig, ZramConfig};
use xzram::backend::{available_swapfile_backend, available_zram_backend};
use xzram::detect;
use xzram::doctor;
use xzram::status::{self, format_bytes};
use xzram::sysctl::{self, SysctlValues};

#[derive(Parser)]
#[command(name = "xzram", about = "Cross-distro Linux swap management", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,
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
    /// Apply pending configuration changes
    Apply,
    /// Restore last known-good configuration
    Rollback,
}

#[derive(Subcommand)]
enum ZramCommands {
    /// Show current zram configuration
    Show,
    /// Set zram configuration
    Set {
        #[arg(long, default_value = "zram0")]
        device: String,
        #[arg(long, default_value = "min(ram / 2, 4096)")]
        size: String,
        #[arg(long, default_value = "zstd")]
        algorithm: String,
        #[arg(long, default_value = "100")]
        priority: i32,
    },
    /// Disable zram
    Disable,
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
    },
    /// Resize an existing swap file
    Resize {
        path: String,
        #[arg(long)]
        size_mb: u64,
    },
    /// Remove a swap file
    Remove { path: String },
}

#[derive(Subcommand)]
enum SwapCommands {
    /// List active swap devices
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
                let backend = available_zram_backend();
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
            } => {
                let config = ZramConfig {
                    device,
                    zram_size: Some(size),
                    compression_algorithm: Some(algorithm),
                    swap_priority: Some(priority),
                    fs_type: None,
                    mount_point: None,
                };
                run_privileged("zram.configure", &serde_json::to_string(&config)?)?;
            }
            ZramCommands::Disable => {
                run_privileged("zram.disable", "{}")?;
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
            } => {
                let config = SwapfileConfig {
                    path,
                    size_mb,
                    priority,
                };
                run_privileged("swapfile.create", &serde_json::to_string(&config)?)?;
            }
            SwapfileCommands::Resize { path, size_mb } => {
                let payload = serde_json::json!({ "path": path, "size_mb": size_mb });
                run_privileged("swapfile.resize", &payload.to_string())?;
            }
            SwapfileCommands::Remove { path } => {
                let payload = serde_json::json!({ "path": path });
                run_privileged("swapfile.remove", &payload.to_string())?;
            }
        },
        Commands::Swap { command } => match command {
            SwapCommands::List => {
                let report = status::status()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&report.swaps)?);
                } else {
                    for s in &report.swaps {
                        println!(
                            "{}  {}  {} used  priority {}",
                            s.name,
                            format_bytes(s.size_bytes),
                            format_bytes(s.used_bytes),
                            s.priority
                        );
                    }
                }
            }
            SwapCommands::On { device } => {
                let payload = serde_json::json!({ "action": "on", "device": device });
                run_privileged("swap.activate", &payload.to_string())?;
            }
            SwapCommands::Off { device } => {
                let payload = serde_json::json!({ "action": "off", "device": device });
                run_privileged("swap.activate", &payload.to_string())?;
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
                run_privileged("sysctl.set", &serde_json::to_string(&values)?)?;
            }
        },
        Commands::Apply => {
            run_privileged("apply", "{}")?;
        }
        Commands::Rollback => {
            run_privileged("rollback", "{}")?;
        }
    }

    Ok(())
}

fn run_privileged(action: &str, payload: &str) -> anyhow::Result<()> {
    let helper = find_helper()?;
    let action_id = privileged_action_id(action);
    let status = Command::new("pkexec")
        .arg(format!("--action-id={action_id}"))
        .arg(&helper)
        .arg(action)
        .arg(payload)
        .status()?;

    if !status.success() {
        anyhow::bail!("privileged operation failed (pkexec exit {status})");
    }
    Ok(())
}

fn privileged_action_id(action: &str) -> String {
    format!("io.github.xzram.{action}")
}

fn find_helper() -> anyhow::Result<String> {
    for path in [
        "/usr/libexec/xzram-helper",
        "/usr/local/libexec/xzram-helper",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xzram-helper/../../target/release/xzram-helper"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xzram-helper/../../target/debug/xzram-helper"
        ),
    ] {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }
    anyhow::bail!("xzram-helper not found; build with 'cargo build --release'")
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
