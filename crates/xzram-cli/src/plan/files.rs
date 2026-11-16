//! Plans for swap files and for the stored snapshots of the configuration.

use std::path::Path;

use xzram::apply::guard::{decide, margin_bytes};
use xzram::apply::{ApplyOptions, MemoryCheck};
use xzram::status::{format_bytes, StatusReport, MIB};

use super::Plan;

fn mib(size_mb: u64) -> u64 {
    size_mb.saturating_mul(MIB)
}

/// A swap file switched off brings what is in it back into memory.
pub(super) fn swapoff_check(what: String, incoming: u64, status: &StatusReport) -> MemoryCheck {
    MemoryCheck {
        what,
        incoming_bytes: incoming,
        margin_bytes: margin_bytes(status.memory.mem_total_kb * 1024),
        available_bytes: status.memory.mem_available_kb * 1024,
    }
}

pub(super) fn refuse_if_short(plan: Plan, check: &MemoryCheck) -> Plan {
    match decide(ApplyOptions::default(), check) {
        Ok(_) => plan,
        Err(e) => Plan {
            would_refuse: true,
            reason: e.to_string(),
            ..plan
        },
    }
}

pub(crate) fn swapfile_create(
    path: &str,
    size_mb: u64,
    priority: i32,
    free_bytes: Option<u64>,
    validation: Result<(), String>,
) -> Plan {
    let mut plan = Plan::new(format!("Create a swap file at {path}"))
        .change(
            "Swap file",
            format!("{path}, {}", format_bytes(mib(size_mb))),
            format!("Priority {priority}. Switched on now and at every start."),
            Some(mib(size_mb)),
            Some(true),
        )
        .change(
            "Files",
            "A line is added to /etc/fstab",
            "So the swap file comes back after a restart.",
            None,
            Some(true),
        );
    if let Err(reason) = validation {
        plan.would_refuse = true;
        plan.reason = reason;
    } else if Path::new(path).exists() {
        plan.would_refuse = true;
        plan.reason = format!("{path} already exists. Nothing was changed.");
    } else if let Some(free) = free_bytes.filter(|f| *f < mib(size_mb)) {
        plan.would_refuse = true;
        plan.reason = format!(
            "There is not enough disk space for {} ({} is free). Nothing was changed.",
            format_bytes(mib(size_mb)),
            format_bytes(free)
        );
    }
    plan
}

pub(crate) fn swapfile_resize(
    path: &str,
    size_mb: u64,
    used_bytes: u64,
    free_bytes: Option<u64>,
    status: &StatusReport,
    validation: Result<(), String>,
) -> Plan {
    let mut plan = Plan::new(format!("Resize the swap file {path}")).change(
        "Swap file",
        format!("{path} becomes {}", format_bytes(mib(size_mb))),
        "A new file is made first, so the old one stays if that fails.",
        Some(mib(size_mb)),
        Some(true),
    );
    if used_bytes > 0 {
        plan = plan.change(
            "Memory",
            "What is stored in it moves back into memory",
            format!("{} is in use", format_bytes(used_bytes)),
            Some(used_bytes),
            Some(true),
        );
    }
    if let Err(reason) = validation {
        plan.would_refuse = true;
        plan.reason = reason;
        return plan;
    }
    if let Some(free) = free_bytes.filter(|f| *f < mib(size_mb)) {
        plan.would_refuse = true;
        plan.reason = format!(
            "There is not enough disk space for {} ({} is free). Nothing was changed.",
            format_bytes(mib(size_mb)),
            format_bytes(free)
        );
        return plan;
    }
    refuse_if_short(
        plan,
        &swapoff_check(format!("resizing {path}"), used_bytes, status),
    )
}

pub(crate) fn swapfile_remove(
    path: &str,
    size_mb: Option<u64>,
    used_bytes: u64,
    status: &StatusReport,
    validation: Result<(), String>,
) -> Plan {
    let mut plan = Plan::new(format!("Remove the swap file {path}"))
        .change(
            "Swap file",
            format!("{path} is switched off and deleted"),
            "Its size is given back to the disk.",
            size_mb.map(mib),
            Some(false),
        )
        .change(
            "Files",
            "Its line is taken out of /etc/fstab",
            "",
            None,
            Some(true),
        );
    if used_bytes > 0 {
        plan = plan.change(
            "Memory",
            "What is stored in it moves back into memory",
            format!("{} is in use", format_bytes(used_bytes)),
            Some(used_bytes),
            Some(true),
        );
    }
    if let Err(reason) = validation {
        plan.would_refuse = true;
        plan.reason = reason;
        return plan;
    }
    refuse_if_short(
        plan,
        &swapoff_check(format!("removing {path}"), used_bytes, status),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use xzram::status::{MemoryInfo, SwapEntry};

    fn status(available_kb: u64, used: u64) -> StatusReport {
        StatusReport {
            swaps: vec![SwapEntry {
                name: "/swap/swapfile".into(),
                swap_type: "file".into(),
                size_bytes: 8 << 30,
                used_bytes: used,
                priority: 10,
            }],
            zram_devices: vec![],
            memory: MemoryInfo {
                mem_total_kb: 16 << 20,
                mem_available_kb: available_kb,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        }
    }

    #[test]
    fn removing_a_swap_file_in_use_needs_the_memory_back() {
        let st = status(1 << 20, 4 << 30);
        let plan = swapfile_remove("/swap/swapfile", Some(8192), 4 << 30, &st, Ok(()));
        assert!(plan.would_refuse);
        let plan = swapfile_remove("/swap/swapfile", Some(8192), 0, &st, Ok(()));
        assert!(!plan.would_refuse);
    }

    #[test]
    fn create_is_refused_for_a_bad_path_or_too_little_disk() {
        let bad = swapfile_create("/etc/x", 1024, 10, None, Err("not allowed".into()));
        assert!(bad.would_refuse && bad.reason == "not allowed");
        let full = swapfile_create(
            "/swap/new-swapfile-for-test",
            1024,
            10,
            Some(1 << 20),
            Ok(()),
        );
        assert!(full.would_refuse && full.reason.contains("disk space"));
        let ok = swapfile_create(
            "/swap/new-swapfile-for-test",
            1024,
            10,
            Some(8 << 30),
            Ok(()),
        );
        assert!(!ok.would_refuse);
    }
}
