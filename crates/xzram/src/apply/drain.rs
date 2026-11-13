//! Move overflow pages from lower-priority disk swap back into zram (issue #28, opt-in).
//!
//! After zram grows, what already spilled to the disk swapfile stays there and only comes
//! back as pages are touched. `swapoff` + `swapon` of each lower-priority disk swap pulls
//! those pages in at once; under reclaim they then land in zram, the highest-priority swap.
//! The same memory guard as a zram restart applies: estimated RAM cost (pages that fit in
//! zram's free capacity cost their compressed size, the rest their full size) must fit in
//! `MemAvailable` minus a margin.

use serde::{Deserialize, Serialize};

use super::commands::run_command;
use super::guard::{margin_bytes, MemoryCheck};
use crate::error::{Result, XzramError};
use crate::status::{format_bytes, StatusReport};
use crate::validation::fstab::decode_fstab_escapes;

/// Compression ratio assumed when zram reports none yet.
const DEFAULT_RATIO: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrainTarget {
    /// Path as listed in `/proc/swaps` (escapes decoded).
    pub name: String,
    pub priority: i32,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrainPlan {
    /// Disk swaps below zram priority that hold data, in the order they are drained.
    pub targets: Vec<DrainTarget>,
    pub zram_free_bytes: u64,
    pub check: MemoryCheck,
}

/// Plan the drain from a status snapshot. `Err` when there is no active zram to drain into.
pub fn plan_drain(status: &StatusReport) -> std::result::Result<DrainPlan, String> {
    let zram: Vec<_> = status
        .swaps
        .iter()
        .filter(|s| s.name.contains("zram"))
        .collect();
    let Some(zram_priority) = zram.iter().map(|s| s.priority).max() else {
        return Err("no active zram swap to move overflow into".into());
    };
    let zram_free_bytes: u64 = zram
        .iter()
        .map(|s| s.size_bytes.saturating_sub(s.used_bytes))
        .sum();
    let targets: Vec<DrainTarget> = status
        .swaps
        .iter()
        .filter(|s| !s.name.contains("zram") && s.priority < zram_priority && s.used_bytes > 0)
        .map(|s| DrainTarget {
            name: decode_fstab_escapes(&s.name),
            priority: s.priority,
            used_bytes: s.used_bytes,
        })
        .collect();

    let (data, compr) = status.zram_devices.iter().fold((0u64, 0u64), |(d, c), z| {
        (d + z.data_bytes, c + z.compressed_bytes)
    });
    let ratio = if data > 0 && compr > 0 {
        (data as f64 / compr as f64).clamp(1.0, 5.0)
    } else {
        DEFAULT_RATIO
    };
    let disk_used: u64 = targets.iter().map(|t| t.used_bytes).sum();
    let into_zram = disk_used.min(zram_free_bytes);
    let incoming = (disk_used - into_zram) + (into_zram as f64 / ratio) as u64;
    let what = if targets.is_empty() {
        "draining disk swap".to_string()
    } else {
        let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
        format!(
            "draining {} of overflow from {}",
            format_bytes(disk_used),
            names.join(", ")
        )
    };
    Ok(DrainPlan {
        targets,
        zram_free_bytes,
        check: MemoryCheck {
            what,
            incoming_bytes: incoming,
            margin_bytes: margin_bytes(status.memory.mem_total_kb * 1024),
            available_bytes: status.memory.mem_available_kb * 1024,
        },
    })
}

const DRAIN_ALTERNATIVES: &str = "wait for a quieter moment, or pass --force to drain anyway";

/// Refuse a failed check unless `force`. `Ok(Some(warning))` when forced.
pub fn decide_drain(plan: &DrainPlan, force: bool) -> Result<Option<String>> {
    if plan.check.ok() {
        return Ok(None);
    }
    if force {
        return Ok(Some(format!(
            "Warning: forced past the memory check: {}",
            plan.check.summary()
        )));
    }
    Err(XzramError::Validation(
        plan.check.refusal(DRAIN_ALTERNATIVES),
    ))
}

/// Live, privileged: swapoff + swapon (same priority) each planned disk swap.
pub fn drain_overflow(force: bool) -> Result<Vec<String>> {
    let status = crate::status::status()?;
    let plan = plan_drain(&status).map_err(XzramError::Validation)?;
    if plan.targets.is_empty() {
        return Ok(vec![
            "No lower-priority disk swap holds data; nothing to drain".into(),
        ]);
    }
    let mut messages: Vec<String> = decide_drain(&plan, force)?.into_iter().collect();
    for target in &plan.targets {
        let priority = target.priority.to_string();
        run_command("swapoff", &["--", &target.name]).map_err(|e| {
            XzramError::Command(format!(
                "swapoff {} failed ({e}); it is still active swap",
                target.name
            ))
        })?;
        run_command("swapon", &["-p", &priority, "--", &target.name]).map_err(|e| {
            XzramError::Command(format!(
                "{} was drained but re-enabling it failed ({e}); turn it back on with \
                 'xzram swap on {}'",
                target.name, target.name
            ))
        })?;
        messages.push(format!(
            "Drained {} from {} (re-enabled at priority {})",
            format_bytes(target.used_bytes),
            target.name,
            target.priority
        ));
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saturation::tests::status_28;

    const GIB: u64 = 1 << 30;

    #[test]
    fn plans_lower_priority_disk_swap_only() {
        // zram0 grown to 23 GiB holding 4 GiB, 4.1 GiB still on the swapfile.
        let mut status = status_28(4 * GIB, 4 * GIB + (100 << 20));
        status.swaps[0].size_bytes = 23 * GIB;
        let plan = plan_drain(&status).unwrap();
        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].name, "/swap/swapfile");
        assert_eq!(plan.zram_free_bytes, 19 * GIB);
        // All of it fits in zram: cost is the compressed size (~3.2x).
        assert!(
            plan.check.incoming_bytes < 2 * GIB,
            "{}",
            plan.check.summary()
        );
        assert!(plan.check.ok(), "{}", plan.check.summary());
        assert_eq!(decide_drain(&plan, false).unwrap(), None);
    }

    #[test]
    fn refuses_when_ram_and_zram_cannot_take_it() {
        // zram full: everything comes back at full size; only 1 GiB available.
        let mut status = status_28(4 * GIB, 4 * GIB);
        status.memory.mem_available_kb = 1024 * 1024;
        let plan = plan_drain(&status).unwrap();
        assert_eq!(plan.check.incoming_bytes, 4 * GIB);
        let err = decide_drain(&plan, false).unwrap_err().to_string();
        assert!(err.contains(super::super::guard::MEMORY_REFUSAL), "{err}");
        assert!(err.contains("--force"), "{err}");
        assert!(decide_drain(&plan, true).unwrap().is_some());
    }

    #[test]
    fn nothing_to_drain_or_no_zram() {
        let calm = status_28(GIB, 0);
        assert!(plan_drain(&calm).unwrap().targets.is_empty());
        let mut higher = status_28(4 * GIB, GIB);
        higher.swaps[1].priority = 150;
        assert!(plan_drain(&higher).unwrap().targets.is_empty());
        let mut no_zram = status_28(0, GIB);
        no_zram.swaps.remove(0);
        assert!(plan_drain(&no_zram).is_err());
    }
}
