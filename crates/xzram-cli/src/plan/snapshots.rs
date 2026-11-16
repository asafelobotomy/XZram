//! Plans for the stored snapshots of the configuration.

use xzram::snapshot::{SnapshotMeta, SnapshotTrigger};
use xzram::status::{format_bytes, StatusReport, MIB};

use super::files::{refuse_if_short, swapoff_check};
use super::Plan;

fn mib(size_mb: u64) -> u64 {
    size_mb.saturating_mul(MIB)
}

fn artifacts(meta: &SnapshotMeta) -> Vec<(&'static str, bool)> {
    let a = &meta.artifacts;
    vec![
        (
            "Compressed memory settings (zram-generator)",
            a.zram_generator_conf.present,
        ),
        ("Swap lines in /etc/fstab", a.fstab.present),
        ("Kernel tuning (sysctl)", a.sysctl.present),
        ("zram-tools settings", a.zramswap.present),
        ("zswap setting", a.zswap.present),
        (
            "Memory-pressure killer (oomd)",
            a.oomd_root.present || a.oomd_user.present || a.oomd_session.present,
        ),
        ("Memory tunables (THP, MGLRU)", a.mm.present),
    ]
}

pub(crate) fn snapshot_restore(
    meta: Option<&SnapshotMeta>,
    id: &str,
    status: &StatusReport,
) -> Plan {
    let Some(meta) = meta else {
        return Plan::refused(
            format!("Restore snapshot {id}"),
            format!("There is no snapshot {id}. Nothing was changed."),
        );
    };
    let mut plan = Plan::new(format!("Restore the snapshot “{}”", meta.label));
    for (label, present) in artifacts(meta) {
        if present {
            plan = plan.change(
                "Settings put back",
                label,
                "Replaced by the stored copy.",
                None,
                Some(true),
            );
        }
    }
    for s in &meta.swapfiles {
        plan = plan.change(
            "Swap files",
            format!("{} ({})", s.path, format_bytes(mib(s.size_mb))),
            if s.present_on_disk {
                "Kept as it is."
            } else {
                "Made again."
            },
            None,
            Some(true),
        );
    }
    plan = plan.change(
        "Memory",
        "Swap is switched off and on again while this runs",
        "Everything in swap moves back into memory first.",
        None,
        Some(true),
    );
    let used: u64 = status.swaps.iter().map(|s| s.used_bytes).sum();
    let check = swapoff_check("restoring the snapshot".into(), used, status);
    refuse_if_short(plan, &check)
}

pub(crate) fn rollback(latest_pre_apply: Option<&SnapshotMeta>, status: &StatusReport) -> Plan {
    match latest_pre_apply {
        None => Plan::refused(
            "Go back to before the last change",
            "No change has been made through XZram yet, so there is nothing to go back to.",
        ),
        Some(meta) => {
            let mut plan = snapshot_restore(Some(meta), &meta.id, status);
            plan.summary = format!("Go back to before the last change (“{}”)", meta.label);
            plan
        }
    }
}

pub(crate) fn snapshot_delete(meta: Option<&SnapshotMeta>, id: &str) -> Plan {
    match meta {
        None => Plan::refused(
            format!("Delete snapshot {id}"),
            format!("There is no snapshot {id}. Nothing was changed."),
        ),
        Some(m) => Plan::new(format!("Delete the snapshot “{}”", m.label)).change(
            "Snapshots",
            format!("{} ({})", m.label, m.created_at),
            "The stored copy of the settings is deleted.",
            None,
            Some(false),
        ),
    }
}

/// The snapshots `prune --keep N` removes: all but the newest N, and never the newest
/// `pre_apply` one (`rollback` needs it). `list` is newest first, as XZram stores it.
pub(crate) fn pruned(list: &[SnapshotMeta], keep: usize) -> Vec<&SnapshotMeta> {
    let newest_pre_apply = list
        .iter()
        .find(|s| s.trigger == SnapshotTrigger::PreApply)
        .map(|s| s.id.as_str());
    list.iter()
        .skip(keep)
        .filter(|s| Some(s.id.as_str()) != newest_pre_apply)
        .collect()
}

pub(crate) fn snapshot_prune(list: &[SnapshotMeta], keep: usize) -> Plan {
    let doomed = pruned(list, keep);
    if doomed.is_empty() {
        return Plan::new(format!("Keep the newest {keep} snapshots"));
    }
    let mut plan = Plan::new(format!(
        "Delete {} old snapshot{}, keeping the newest {keep}",
        doomed.len(),
        if doomed.len() == 1 { "" } else { "s" }
    ));
    for m in doomed {
        plan = plan.change(
            "Old snapshots",
            m.label.clone(),
            m.created_at.clone(),
            None,
            Some(false),
        );
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use xzram::snapshot::{SnapshotArtifact, SnapshotArtifacts};
    use xzram::status::{MemoryInfo, SwapEntry};

    fn snap(id: &str, trigger: SnapshotTrigger) -> SnapshotMeta {
        SnapshotMeta {
            id: id.into(),
            created_at: "2026-11-16T10:00:00Z".into(),
            label: format!("label {id}"),
            trigger,
            state_hash: String::new(),
            xzram_version: "0.6.0".into(),
            pending_summary: None,
            artifacts: SnapshotArtifacts {
                sysctl: SnapshotArtifact {
                    present: true,
                    filename: None,
                },
                ..Default::default()
            },
            swapfiles: vec![],
            zram_devices: vec![],
            zswap_enabled: None,
            oomd_enabled: None,
            mm_runtime: None,
        }
    }

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
    fn prune_never_removes_the_newest_pre_apply_snapshot() {
        let list = vec![
            snap("d", SnapshotTrigger::Manual),
            snap("c", SnapshotTrigger::PreApply),
            snap("b", SnapshotTrigger::Manual),
            snap("a", SnapshotTrigger::PreApply),
        ];
        let ids: Vec<_> = pruned(&list, 1).iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a"]);
        assert!(pruned(&list, 4).is_empty());
    }

    #[test]
    fn prune_plan_lists_each_snapshot_as_not_reversible() {
        let list = vec![
            snap("b", SnapshotTrigger::Manual),
            snap("a", SnapshotTrigger::Manual),
        ];
        let plan = snapshot_prune(&list, 1);
        assert_eq!(plan.changes.len(), 1);
        assert_eq!(plan.changes[0].reversible, Some(false));
        assert!(snapshot_prune(&list, 5).changes.is_empty());
    }

    #[test]
    fn restore_of_an_unknown_snapshot_is_refused() {
        assert!(snapshot_restore(None, "x", &status(8 << 20, 0)).would_refuse);
        assert!(snapshot_delete(None, "x").would_refuse);
        assert!(rollback(None, &status(8 << 20, 0)).would_refuse);
    }

    #[test]
    fn restore_is_refused_when_swap_cannot_come_back_into_memory() {
        let m = snap("a", SnapshotTrigger::Manual);
        let plan = snapshot_restore(Some(&m), "a", &status(1 << 20, 4 << 30));
        assert!(plan.would_refuse);
        assert!(plan.reason.contains("not enough free memory"));
        assert!(!snapshot_restore(Some(&m), "a", &status(8 << 20, 1 << 20)).would_refuse);
    }
}
