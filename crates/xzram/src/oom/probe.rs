//! Read-only probes for OOM protection, memory pressure and past OOM kills (no root).

use std::process::Command;

use super::{OOMD_SERVICE, OOMD_SESSION_DROPIN};

/// Other userspace OOM killers; any one of them active makes systemd-oomd redundant.
const OTHER_KILLERS: [&str; 3] = [
    "earlyoom.service",
    "nohang.service",
    "nohang-desktop.service",
];

/// Units whose `ManagedOOM*=kill` makes systemd-oomd act (distro or xzram drop-ins).
const MANAGED_UNITS: [&str; 4] = ["-.slice", "system.slice", "user.slice", "user@*.service"];

#[derive(Debug, Clone, Default)]
pub struct OomProbe {
    /// systemd-oomd unit file exists.
    pub oomd_installed: bool,
    pub oomd_active: bool,
    /// At least one unit has `ManagedOOMSwap=kill` or `ManagedOOMMemoryPressure=kill`.
    pub oomd_has_policies: bool,
    /// Active alternative killer (earlyoom / nohang), if any.
    pub other_killer: Option<String>,
    /// Kernel exposes PSI (`/proc/pressure/memory`), which systemd-oomd requires.
    pub psi_available: bool,
    /// A `user@*.service` has `ManagedOOMMemoryPressure=kill` (can pick session cgroups).
    pub user_pressure_kill: bool,
    /// `session.slice` is `ManagedOOMPreference=avoid|omit` (xzram drop-in or the
    /// calling user's manager reports it).
    pub session_guarded: bool,
}

impl OomProbe {
    /// A userspace killer is actually able to act before the kernel OOM killer.
    pub fn protected(&self) -> bool {
        self.other_killer.is_some() || (self.oomd_active && self.oomd_has_policies)
    }

    /// oomd may kill inside the user manager but the desktop session is not shielded.
    pub fn session_at_risk(&self) -> bool {
        self.oomd_active && self.user_pressure_kill && !self.session_guarded
    }
}

pub fn probe() -> OomProbe {
    let show = managed_show();
    OomProbe {
        oomd_installed: unit_installed(OOMD_SERVICE),
        oomd_active: unit_active(OOMD_SERVICE),
        oomd_has_policies: show_has_kill_policy(&show),
        user_pressure_kill: show_user_pressure_kill(&show),
        session_guarded: session_guarded(),
        other_killer: OTHER_KILLERS
            .iter()
            .find(|u| unit_active(u))
            .map(|u| u.trim_end_matches(".service").to_string()),
        psi_available: std::path::Path::new("/proc/pressure/memory").exists(),
    }
}

fn unit_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
        .is_ok_and(|s| s.success())
}

fn unit_installed(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["list-unit-files", "--no-legend", unit])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(unit))
}

fn session_guarded() -> bool {
    if crate::snapshot::paths::etc_path(OOMD_SESSION_DROPIN).exists() {
        return true;
    }
    Command::new("systemctl")
        .args([
            "--user",
            "show",
            "-p",
            "ManagedOOMPreference",
            "session.slice",
        ])
        .output()
        .is_ok_and(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            matches!(
                out.trim(),
                "ManagedOOMPreference=avoid" | "ManagedOOMPreference=omit"
            )
        })
}

/// `systemctl show` output for [`MANAGED_UNITS`], one block per unit (with `Id=`).
fn managed_show() -> String {
    let mut args = vec![
        "show",
        "-p",
        "Id,ManagedOOMSwap,ManagedOOMMemoryPressure",
        "--",
    ];
    args.extend(MANAGED_UNITS);
    Command::new("systemctl")
        .args(&args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn show_user_pressure_kill(show: &str) -> bool {
    show.split("\n\n").any(|block| {
        block.lines().any(|l| l.starts_with("Id=user@"))
            && block
                .lines()
                .any(|l| l.trim() == "ManagedOOMMemoryPressure=kill")
    })
}

fn show_has_kill_policy(show: &str) -> bool {
    show.lines().any(|l| {
        matches!(
            l.trim(),
            "ManagedOOMSwap=kill" | "ManagedOOMMemoryPressure=kill"
        )
    })
}

/// Kernel OOM kills since boot (`oom_kill` in `/proc/vmstat`).
pub fn oom_kills_this_boot() -> Option<u64> {
    let vmstat = std::fs::read_to_string("/proc/vmstat").ok()?;
    parse_vmstat_oom_kill(&vmstat)
}

fn parse_vmstat_oom_kill(vmstat: &str) -> Option<u64> {
    vmstat
        .lines()
        .find_map(|l| l.strip_prefix("oom_kill "))
        .and_then(|v| v.trim().parse().ok())
}

/// Kernel OOM kills logged during the previous boot. `None` when the journal is unreadable
/// (no persistent journal, or the user lacks journal access).
pub fn oom_kills_previous_boot() -> Option<usize> {
    let output = Command::new("journalctl")
        .args(["-k", "-b", "-1", "-q", "--no-pager", "-o", "cat"])
        .args(["-g", "Out of memory: Killed process"])
        .output()
        .ok()?;
    // journalctl exits 1 when the grep has no matches; a missing boot also prints to stderr.
    if !output.status.success() && !output.stderr.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).lines().count())
}

/// Memory PSI averages (percent of wall time stalled).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MemoryPressure {
    pub some_avg300: f64,
    pub full_avg60: f64,
}

pub fn parse_psi_memory(text: &str) -> Option<MemoryPressure> {
    let field = |kind: &str, key: &str| -> Option<f64> {
        let line = text.lines().find(|l| l.starts_with(kind))?;
        line.split_whitespace()
            .find_map(|kv| kv.strip_prefix(key))
            .and_then(|v| v.parse().ok())
    };
    Some(MemoryPressure {
        some_avg300: field("some", "avg300=")?,
        full_avg60: field("full", "avg60=")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vmstat_oom_kill() {
        assert_eq!(
            parse_vmstat_oom_kill("pgfault 10\noom_kill 3\nfoo 1\n"),
            Some(3)
        );
        assert_eq!(parse_vmstat_oom_kill("pgfault 10\n"), None);
    }

    #[test]
    fn parses_psi_memory() {
        let psi = "some avg10=0.00 avg60=1.50 avg300=12.25 total=7\n\
                   full avg10=0.00 avg60=6.10 avg300=0.80 total=7\n";
        assert_eq!(
            parse_psi_memory(psi),
            Some(MemoryPressure {
                some_avg300: 12.25,
                full_avg60: 6.10,
            })
        );
        assert_eq!(parse_psi_memory("garbage"), None);
    }

    #[test]
    fn detects_kill_policy_in_systemctl_show() {
        let none = "ManagedOOMSwap=auto\nManagedOOMMemoryPressure=auto\n\n\
                    ManagedOOMSwap=auto\nManagedOOMMemoryPressure=auto\n";
        assert!(!show_has_kill_policy(none));
        assert!(show_has_kill_policy(
            "ManagedOOMSwap=kill\nManagedOOMMemoryPressure=auto\n"
        ));
    }

    #[test]
    fn detects_user_pressure_kill_block() {
        let show = "Id=-.slice\nManagedOOMSwap=kill\nManagedOOMMemoryPressure=auto\n\n\
                    Id=user@1000.service\nManagedOOMSwap=auto\nManagedOOMMemoryPressure=kill\n";
        assert!(show_user_pressure_kill(show));
        assert!(!show_user_pressure_kill(
            "Id=-.slice\nManagedOOMMemoryPressure=kill\n"
        ));
        let at_risk = OomProbe {
            oomd_active: true,
            user_pressure_kill: true,
            ..Default::default()
        };
        assert!(at_risk.session_at_risk());
        assert!(!OomProbe {
            session_guarded: true,
            ..at_risk
        }
        .session_at_risk());
    }

    #[test]
    fn protected_needs_policies_or_alternative() {
        let mut p = OomProbe {
            oomd_active: true,
            ..Default::default()
        };
        assert!(!p.protected(), "oomd without policies never acts");
        p.oomd_has_policies = true;
        assert!(p.protected());
        let alt = OomProbe {
            other_killer: Some("earlyoom".into()),
            ..Default::default()
        };
        assert!(alt.protected());
    }
}
