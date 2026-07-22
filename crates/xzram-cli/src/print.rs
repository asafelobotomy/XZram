use xzram::apply::ZramConfig;
use xzram::detect;
use xzram::doctor;
use xzram::recommend;
use xzram::status::{self, format_bytes};
use xzram::swapfile_btrfs;
use xzram::sysctl::SysctlValues;

pub(crate) fn print_status(report: &status::StatusReport) {
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

pub(crate) fn print_detect(report: &detect::DetectionReport) {
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

pub(crate) fn print_doctor(report: &doctor::DoctorReport) {
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

pub(crate) fn print_zram_config(c: &ZramConfig) {
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

pub(crate) fn confirm_apply_defaults() -> anyhow::Result<bool> {
    confirm("Apply recommended defaults now?")
}

pub(crate) fn confirm(prompt: &str) -> anyhow::Result<bool> {
    use std::io::{self, Write};
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub(crate) fn print_recommended_defaults(report: &recommend::RecommendedDefaults) {
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

pub(crate) fn print_nodatacow_status(status: &swapfile_btrfs::NodatacowStatus) {
    println!("Swapfile:     {}", status.swapfile_path);
    println!("Parent dir:   {}", status.parent_directory);
    println!("Filesystem:   {}", status.filesystem);
    println!("Ready:        {}", if status.ready { "yes" } else { "no" });
    println!("{}", status.message);
}

pub(crate) fn print_sysctl(v: &SysctlValues) {
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
