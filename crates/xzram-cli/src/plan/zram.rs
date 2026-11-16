//! Plans for compressed memory and kernel tuning.

use xzram::apply::guard::{decide, zram_restart_check};
use xzram::apply::{ApplyOptions, ZramConfig};
use xzram::status::StatusReport;

use super::Plan;
use crate::settings::Key;

/// What changing one zram setting would do. `next` is the whole configuration after the
/// change (`settings::merged_zram`); `status` is the live memory state.
pub(crate) fn zram_change(
    key: Key,
    old: Option<&ZramConfig>,
    next: &ZramConfig,
    opts: ApplyOptions,
    status: &StatusReport,
) -> Plan {
    let (before, after) = (key.show_zram(old), key.show_zram(Some(next)));
    let at_boot = opts.at_boot;
    let summary = if at_boot {
        format!("Save {} for the next restart", key.label().to_lowercase())
    } else {
        format!("Change {}", key.label().to_lowercase())
    };
    let mut plan = Plan::new(summary).change(
        "Compressed memory",
        format!("{}: {before} → {after}", key.label()),
        if at_boot {
            "Written now. The running device changes at the next restart."
        } else {
            "Applied now: the device is restarted."
        },
        None,
        Some(true),
    );
    if at_boot {
        return plan;
    }
    let check = zram_restart_check(&status.memory, &status.zram_devices);
    if check.incoming_bytes > 0 {
        plan = plan.change(
            "Memory",
            "Swap in use moves back into memory first",
            check.summary(),
            Some(check.incoming_bytes),
            Some(true),
        );
    }
    match decide(opts, &check) {
        Ok(_) => plan,
        Err(e) => Plan {
            would_refuse: true,
            reason: e.to_string(),
            ..plan
        },
    }
}

/// Turning compressed memory off, now or from the next restart.
pub(crate) fn zram_disable(opts: ApplyOptions, status: &StatusReport) -> Plan {
    let mut plan = Plan::new(if opts.at_boot {
        "Keep compressed memory off from the next restart"
    } else {
        "Turn compressed memory off"
    })
    .change(
        "Compressed memory",
        "Compressed memory is turned off",
        if opts.at_boot {
            "Written now. The running device stays until the next restart."
        } else {
            "Applied now: swap in use is moved first."
        },
        None,
        Some(true),
    );
    if opts.at_boot {
        return plan;
    }
    let check = zram_restart_check(&status.memory, &status.zram_devices);
    if check.incoming_bytes > 0 {
        plan = plan.change(
            "Memory",
            "Swap in use moves back into memory first",
            check.summary(),
            Some(check.incoming_bytes),
            Some(true),
        );
    }
    match decide(opts, &check) {
        Ok(_) => plan,
        Err(e) => Plan {
            would_refuse: true,
            reason: e.to_string(),
            ..plan
        },
    }
}

/// A kernel setting changing from `old` to `new`; nothing restarts.
pub(crate) fn tuning(key: Key, old: Option<u64>, new: u64) -> Plan {
    let before = old.map_or_else(|| "not set".to_string(), |v| v.to_string());
    Plan::new(format!("Change {}", key.label().to_lowercase())).change(
        "Kernel tuning",
        format!("{}: {before} → {new}", key.label()),
        "Applied now and kept after restarts. Nothing restarts.",
        None,
        Some(true),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use xzram::status::{MemoryInfo, ZramDevice};

    fn config(size: &str) -> ZramConfig {
        ZramConfig {
            device: "zram0".into(),
            zram_size: Some(size.into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        }
    }

    fn status(available_kb: u64, stored: u64) -> StatusReport {
        StatusReport {
            swaps: vec![],
            zram_devices: vec![ZramDevice {
                name: "zram0".into(),
                algorithm: "zstd".into(),
                disk_size_bytes: 4 << 30,
                data_bytes: stored,
                compressed_bytes: stored / 3,
                total_bytes: stored / 3,
                mem_used_max_bytes: 0,
                mount_point: "[SWAP]".into(),
            }],
            memory: MemoryInfo {
                mem_total_kb: 16 << 20,
                mem_available_kb: available_kb,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
        }
    }

    #[test]
    fn size_change_shows_before_and_after_and_the_memory_it_needs() {
        let (old, new) = (config("ram / 4"), config("ram / 2"));
        let plan = zram_change(
            Key::ZramSize,
            Some(&old),
            &new,
            ApplyOptions::default(),
            &status(8 << 20, 1 << 30),
        );
        assert!(!plan.would_refuse, "{}", plan.reason);
        assert_eq!(
            plan.changes[0].label,
            "Compressed memory size: ram / 4 → ram / 2"
        );
        assert_eq!(plan.changes[1].bytes, Some(1 << 30));
    }

    #[test]
    fn refused_for_memory_keeps_the_phrase_the_page_keys_on() {
        let (old, new) = (config("ram / 4"), config("ram / 2"));
        let plan = zram_change(
            Key::ZramSize,
            Some(&old),
            &new,
            ApplyOptions::default(),
            &status(1 << 20, 4 << 30),
        );
        assert!(plan.would_refuse);
        assert!(plan.reason.contains("not enough free memory"));
    }

    #[test]
    fn at_boot_and_force_skip_the_memory_check() {
        let (old, new) = (config("ram / 4"), config("ram / 2"));
        let low = status(1 << 20, 4 << 30);
        for opts in [
            ApplyOptions {
                at_boot: true,
                force: false,
            },
            ApplyOptions {
                at_boot: false,
                force: true,
            },
        ] {
            assert!(!zram_change(Key::ZramSize, Some(&old), &new, opts, &low).would_refuse);
        }
    }

    #[test]
    fn disable_is_refused_like_a_restart_would_be() {
        let low = status(1 << 20, 4 << 30);
        assert!(zram_disable(ApplyOptions::default(), &low).would_refuse);
        assert!(
            !zram_disable(
                ApplyOptions {
                    at_boot: true,
                    force: false
                },
                &low
            )
            .would_refuse
        );
    }

    #[test]
    fn tuning_plan_names_old_and_new() {
        let plan = tuning(Key::Swappiness, Some(60), 100);
        assert_eq!(plan.changes[0].label, "Swappiness: 60 → 100");
        assert!(!plan.would_refuse);
    }
}
