use serde::{Deserialize, Serialize};

use crate::error::{Result, XzramError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapEntry {
    pub name: String,
    pub swap_type: String,
    pub size_bytes: u64,
    pub used_bytes: u64,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramDevice {
    pub name: String,
    pub algorithm: String,
    pub disk_size_bytes: u64,
    pub data_bytes: u64,
    pub compressed_bytes: u64,
    pub total_bytes: u64,
    pub streams: u32,
    pub mount_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub swaps: Vec<SwapEntry>,
    pub zram_devices: Vec<ZramDevice>,
    pub memory: MemoryInfo,
}

pub fn status() -> Result<StatusReport> {
    Ok(StatusReport {
        swaps: parse_proc_swaps()?,
        zram_devices: parse_zram_devices()?,
        memory: parse_meminfo()?,
    })
}

fn parse_proc_swaps() -> Result<Vec<SwapEntry>> {
    let content = std::fs::read_to_string("/proc/swaps")?;
    let mut swaps = Vec::new();

    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        swaps.push(SwapEntry {
            name: parts[0].to_string(),
            swap_type: parts[1].to_string(),
            size_bytes: parts[2].parse().unwrap_or(0) * 1024,
            used_bytes: parts[3].parse().unwrap_or(0) * 1024,
            priority: parts[4].parse().unwrap_or(0),
        });
    }

    Ok(swaps)
}

pub fn parse_zram_devices() -> Result<Vec<ZramDevice>> {
    let mut devices = Vec::new();
    let block_dir = std::path::Path::new("/sys/block");

    let entries = std::fs::read_dir(block_dir).map_err(XzramError::Io)?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("zram") {
            continue;
        }

        let base = entry.path();
        let algorithm = read_sysfs_string(&base.join("comp_algorithm"))?
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .trim_matches(['[', ']'])
            .to_string();

        let disk_size = read_sysfs_u64(&base.join("disksize"))?;
        let mount_point = detect_zram_mount(&name);

        let (data, compr, total, streams) = read_mm_stat(&base.join("mm_stat"))?;

        devices.push(ZramDevice {
            name,
            algorithm,
            disk_size_bytes: disk_size,
            data_bytes: data,
            compressed_bytes: compr,
            total_bytes: total,
            streams,
            mount_point,
        });
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

fn read_sysfs_string(path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .map_err(XzramError::Io)
}

fn read_sysfs_u64(path: &std::path::Path) -> Result<u64> {
    let content = std::fs::read_to_string(path)?;
    content
        .trim()
        .parse()
        .map_err(|_| XzramError::Parse(format!("invalid u64 in {}", path.display())))
}

fn read_mm_stat(path: &std::path::Path) -> Result<(u64, u64, u64, u32)> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let parts: Vec<&str> = content.split_whitespace().collect();
    Ok((
        parts.first().and_then(|s| s.parse().ok()).unwrap_or(0),
        parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
        parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
        parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0),
    ))
}

fn detect_zram_mount(device: &str) -> String {
    let path = format!("/dev/{device}");
    let output = std::process::Command::new("swapon")
        .args(["--show=NAME,TYPE", "--noheadings"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.first() == Some(&path.as_str()) {
                    return if parts.get(1) == Some(&"partition") || parts.get(1) == Some(&"file") {
                        "[SWAP]".into()
                    } else {
                        parts.get(1).unwrap_or(&"").to_string()
                    };
                }
            }
        }
    }

    if std::fs::read_to_string(format!("/sys/block/{device}/initstate"))
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
    {
        return "[SWAP]".into();
    }

    String::new()
}

fn parse_meminfo() -> Result<MemoryInfo> {
    let content = std::fs::read_to_string("/proc/meminfo")?;
    let mut mem_total_kb = 0u64;
    let mut mem_available_kb = 0u64;
    let mut swap_total_kb = 0u64;
    let mut swap_free_kb = 0u64;

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let value_kb: u64 = value
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            match key {
                "MemTotal" => mem_total_kb = value_kb,
                "MemAvailable" => mem_available_kb = value_kb,
                "SwapTotal" => swap_total_kb = value_kb,
                "SwapFree" => swap_free_kb = value_kb,
                _ => {}
            }
        }
    }

    Ok(MemoryInfo {
        mem_total_kb,
        mem_available_kb,
        swap_total_kb,
        swap_free_kb,
    })
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_swaps_line() {
        let content = "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n/dev/zram0                              partition\t4194300\t\t0\t\t100\n";
        let mut swaps = Vec::new();
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                swaps.push(SwapEntry {
                    name: parts[0].to_string(),
                    swap_type: parts[1].to_string(),
                    size_bytes: parts[2].parse::<u64>().unwrap() * 1024,
                    used_bytes: parts[3].parse::<u64>().unwrap() * 1024,
                    priority: parts[4].parse().unwrap(),
                });
            }
        }
        assert_eq!(swaps.len(), 1);
        assert_eq!(swaps[0].name, "/dev/zram0");
        assert_eq!(swaps[0].priority, 100);
    }

    #[test]
    fn format_bytes_works() {
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(0), "0 B");
    }
}
