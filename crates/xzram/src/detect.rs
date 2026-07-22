use serde::{Deserialize, Serialize};

use crate::error::{Result, XzramError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistroFamily {
    Fedora,
    Debian,
    Ubuntu,
    Arch,
    OpenSuse,
    Gentoo,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroInfo {
    pub id: String,
    pub id_like: Vec<String>,
    pub family: DistroFamily,
    pub version_id: Option<String>,
    pub pretty_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Dnf,
    Apt,
    Pacman,
    Zypper,
    Emerge,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZramBackend {
    SystemdZramGenerator,
    ZramTools,
    Manual,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionReport {
    pub distro: DistroInfo,
    pub package_manager: PackageManager,
    pub init_system: String,
    pub zram_backend: ZramBackend,
    pub zram_generator_installed: bool,
    pub zram_generator_config: Option<String>,
    pub root_filesystem: Option<String>,
    pub etc_writable: bool,
    pub immutable_os: bool,
}

pub fn detect() -> Result<DetectionReport> {
    let os_release = parse_os_release()?;
    let family = classify_distro(&os_release);
    let package_manager = detect_package_manager(&family, &os_release.id_like);
    let zram_backend = detect_zram_backend()?;
    let zram_generator_config = find_zram_generator_config();
    let etc_writable = probe_etc_writable();
    let immutable_os = detect_immutable_os(&os_release);

    Ok(DetectionReport {
        distro: os_release,
        package_manager,
        init_system: detect_init_system(),
        zram_backend,
        zram_generator_installed: which_exists("zram-generator") || zram_generator_config.is_some(),
        zram_generator_config,
        root_filesystem: detect_root_filesystem(),
        etc_writable,
        immutable_os,
    })
}

fn parse_os_release() -> Result<DistroInfo> {
    let content = std::fs::read_to_string("/etc/os-release")
        .map_err(|_| XzramError::NotFound("/etc/os-release".into()))?;

    let mut id = String::from("unknown");
    let mut id_like = Vec::new();
    let mut version_id = None;
    let mut pretty_name = None;

    for line in content.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim_matches('"').to_string();
            match key {
                "ID" => id = value,
                "ID_LIKE" => {
                    id_like = value.split_whitespace().map(str::to_string).collect();
                }
                "VERSION_ID" => version_id = Some(value),
                "PRETTY_NAME" => pretty_name = Some(value),
                _ => {}
            }
        }
    }

    let family = classify_distro_fields(&id, &id_like);

    Ok(DistroInfo {
        id,
        id_like,
        family,
        version_id,
        pretty_name,
    })
}

fn classify_distro(info: &DistroInfo) -> DistroFamily {
    classify_distro_fields(&info.id, &info.id_like)
}

fn classify_distro_fields(id: &str, id_like: &[String]) -> DistroFamily {
    match id {
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" => DistroFamily::Fedora,
        "debian" => DistroFamily::Debian,
        "ubuntu" | "pop" | "linuxmint" => DistroFamily::Ubuntu,
        "arch" | "cachyos" | "manjaro" | "endeavouros" => DistroFamily::Arch,
        "opensuse-leap" | "opensuse-tumbleweed" | "suse" => DistroFamily::OpenSuse,
        "gentoo" => DistroFamily::Gentoo,
        _ => {
            if id_like.iter().any(|l| l == "fedora" || l == "rhel") {
                DistroFamily::Fedora
            } else if id_like.iter().any(|l| l == "debian") {
                if id_like.iter().any(|l| l == "ubuntu") {
                    DistroFamily::Ubuntu
                } else {
                    DistroFamily::Debian
                }
            } else if id_like.iter().any(|l| l == "arch") {
                DistroFamily::Arch
            } else if id_like.iter().any(|l| l == "suse") {
                DistroFamily::OpenSuse
            } else {
                DistroFamily::Unknown
            }
        }
    }
}

fn detect_package_manager(family: &DistroFamily, id_like: &[String]) -> PackageManager {
    match family {
        DistroFamily::Fedora => PackageManager::Dnf,
        DistroFamily::Debian | DistroFamily::Ubuntu => PackageManager::Apt,
        DistroFamily::Arch => PackageManager::Pacman,
        DistroFamily::OpenSuse => PackageManager::Zypper,
        DistroFamily::Gentoo => PackageManager::Emerge,
        DistroFamily::Unknown => {
            if id_like.iter().any(|l| l == "debian" || l == "ubuntu") {
                PackageManager::Apt
            } else if id_like.iter().any(|l| l == "fedora" || l == "rhel") {
                PackageManager::Dnf
            } else if which_exists("pacman") {
                PackageManager::Pacman
            } else if which_exists("apt") {
                PackageManager::Apt
            } else if which_exists("dnf") {
                PackageManager::Dnf
            } else {
                PackageManager::Unknown
            }
        }
    }
}

fn detect_init_system() -> String {
    if std::path::Path::new("/run/systemd/system").exists() {
        "systemd".into()
    } else if std::path::Path::new("/sbin/openrc").exists() {
        "openrc".into()
    } else {
        "unknown".into()
    }
}

fn detect_zram_backend() -> Result<ZramBackend> {
    if find_zram_generator_config().is_some() {
        return Ok(ZramBackend::SystemdZramGenerator);
    }
    if std::path::Path::new("/etc/default/zramswap").exists() {
        return Ok(ZramBackend::ZramTools);
    }
    if std::path::Path::new("/sys/block/zram0").exists() {
        return Ok(ZramBackend::Manual);
    }
    Ok(ZramBackend::None)
}

pub fn find_zram_generator_config() -> Option<String> {
    const PATHS: &[&str] = &[
        "/run/systemd/zram-generator.conf",
        "/etc/systemd/zram-generator.conf",
        "/usr/local/lib/systemd/zram-generator.conf",
        "/usr/lib/systemd/zram-generator.conf",
    ];
    PATHS
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|s| (*s).to_string())
}

fn detect_root_filesystem() -> Option<String> {
    let output = std::process::Command::new("findmnt")
        .args(["-no", "FSTYPE", "/"])
        .output()
        .ok()?;
    if output.status.success() {
        let fstype = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if fstype.is_empty() {
            None
        } else {
            Some(fstype)
        }
    } else {
        None
    }
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn zram_generator_package_name(pm: PackageManager) -> &'static str {
    match pm {
        PackageManager::Dnf | PackageManager::Pacman | PackageManager::Zypper => "zram-generator",
        PackageManager::Apt => "systemd-zram-generator",
        PackageManager::Emerge | PackageManager::Unknown => "zram-generator",
    }
}

pub fn probe_etc_writable() -> bool {
    // Test harness with a fake etc root: keep create-new probe.
    if std::env::var_os("XZRAM_ETC_ROOT").is_some() {
        return probe_etc_writable_by_create();
    }
    // Unprivileged create under /etc always fails on normal distros; use mount options.
    !mount_options_contain_ro("/etc")
}

fn probe_etc_writable_by_create() -> bool {
    let probe = crate::snapshot::etc_root().join(".xzram-writable-probe");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

/// True when the mount covering `path` is read-only (`ro` in findmnt OPTIONS).
pub fn mount_options_contain_ro(path: &str) -> bool {
    let output = std::process::Command::new("findmnt")
        .args(["-no", "OPTIONS", "-T", path])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let opts = String::from_utf8_lossy(&output.stdout);
    findmnt_options_include_ro(&opts)
}

pub fn findmnt_options_include_ro(opts: &str) -> bool {
    opts.split(',').any(|opt| opt.trim() == "ro")
}

fn detect_immutable_os(distro: &DistroInfo) -> bool {
    if distro.id == "nixos" {
        return true;
    }
    if std::env::var("OSTREE_VERSION").is_ok() {
        return true;
    }
    if which_exists("rpm-ostree") {
        return true;
    }
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                if key == "VARIANT_ID" {
                    let value = value.trim_matches('"').to_lowercase();
                    if value.contains("silverblue")
                        || value.contains("kinoite")
                        || value.contains("coreos")
                        || value.contains("ostree")
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_fedora() {
        assert_eq!(classify_distro_fields("fedora", &[]), DistroFamily::Fedora);
    }

    #[test]
    fn classify_cachyos_as_arch() {
        assert_eq!(
            classify_distro_fields("cachyos", &["arch".into()]),
            DistroFamily::Arch
        );
    }

    #[test]
    fn findmnt_options_detect_ro() {
        assert!(findmnt_options_include_ro("ro,relatime,ssd"));
        assert!(findmnt_options_include_ro("rw,ro"));
        assert!(!findmnt_options_include_ro("rw,relatime,ssd"));
        assert!(!findmnt_options_include_ro("rw,errors=remount-ro"));
    }
}
