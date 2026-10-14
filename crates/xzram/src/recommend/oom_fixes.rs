//! OOM protection staged with recommended defaults: enable systemd-oomd (with ManagedOOM
//! drop-ins) when no userspace OOM killer can act, and surface recent kernel OOM kills.

use crate::apply::PendingConfig;
use crate::oom::OomProbe;

use super::engine_notes::note_item;
use super::types::RecommendationItem;

pub(super) fn stage_oom_fixes(
    items: &mut Vec<RecommendationItem>,
    pending: &mut PendingConfig,
    probe: &OomProbe,
    recent_oom_kills: u64,
    staging_blocked: bool,
) {
    if recent_oom_kills > 0 {
        items.push(note_item(
            "Kernel OOM killer fired recently",
            format!(
                "{recent_oom_kills} process(es) were killed after RAM and swap ran out (this or the previous boot). Before that, the desktop usually freezes while the kernel thrashes. A larger overflow swapfile and an early userspace OOM killer address this."
            ),
            Some("oom-protection"),
        ));
    }

    if probe.protected() {
        if probe.session_at_risk() && !staging_blocked {
            items.push(RecommendationItem {
                category: "oom".into(),
                summary: "Protect the desktop session from systemd-oomd".into(),
                detail: "systemd-oomd kills inside user@.service under memory pressure but session.slice (compositor, shell) is not shielded, so it can end the whole session. Apply adds ManagedOOMPreference=avoid on session.slice so apps are killed first.".into(),
                will_stage: true,
                reference: Some("oom-protection".into()),
            });
            pending.enable_oomd = true;
        }
        return;
    }
    if !probe.psi_available {
        items.push(note_item(
            "No userspace OOM killer (kernel lacks PSI)",
            "systemd-oomd needs pressure stall information (CONFIG_PSI, not disabled with psi=0). Install and enable earlyoom instead.",
            Some("oom-protection"),
        ));
        return;
    }
    if !probe.oomd_installed {
        items.push(note_item(
            "No userspace OOM killer installed",
            "Install systemd-oomd (Debian/Ubuntu: systemd-oomd; Fedora: systemd-oomd-defaults) or earlyoom, then re-run recommendations.",
            Some("oom-protection"),
        ));
        return;
    }
    if staging_blocked {
        items.push(note_item(
            "Enable systemd-oomd",
            "No userspace OOM killer is active. Staging is blocked here; enable systemd-oomd.service and add ManagedOOM drop-ins manually.",
            Some("oom-protection"),
        ));
        return;
    }

    let why = if probe.oomd_active {
        "systemd-oomd is running but no unit has a ManagedOOM kill policy, so it never acts."
    } else {
        "No userspace OOM killer is active, so a memory runaway freezes the desktop until the kernel OOM killer fires."
    };
    items.push(RecommendationItem {
        category: "oom".into(),
        summary: "Enable systemd-oomd OOM protection".into(),
        detail: format!(
            "{why} Apply adds ManagedOOMSwap=kill on -.slice and ManagedOOMMemoryPressure=kill (50%) on user@.service, shields session.slice (ManagedOOMPreference=avoid) so apps die before the compositor, then enables systemd-oomd."
        ),
        will_stage: true,
        reference: Some("oom-protection".into()),
    });
    pending.enable_oomd = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unprotected() -> OomProbe {
        OomProbe {
            oomd_installed: true,
            psi_available: true,
            ..Default::default()
        }
    }

    #[test]
    fn stages_oomd_when_unprotected() {
        let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
        stage_oom_fixes(&mut items, &mut pending, &unprotected(), 0, false);
        assert!(pending.enable_oomd);
        assert!(items.iter().any(|i| i.will_stage && i.category == "oom"));
    }

    #[test]
    fn stages_policies_when_oomd_runs_without_them() {
        let probe = OomProbe {
            oomd_active: true,
            ..unprotected()
        };
        let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
        stage_oom_fixes(&mut items, &mut pending, &probe, 0, false);
        assert!(pending.enable_oomd);
        assert!(items[0].detail.contains("never acts"));
    }

    #[test]
    fn skips_when_protected_or_alternative_active() {
        for probe in [
            OomProbe {
                oomd_active: true,
                oomd_has_policies: true,
                ..unprotected()
            },
            OomProbe {
                other_killer: Some("earlyoom".into()),
                ..unprotected()
            },
        ] {
            let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
            stage_oom_fixes(&mut items, &mut pending, &probe, 0, false);
            assert!(!pending.enable_oomd);
            assert!(items.is_empty());
        }
    }

    #[test]
    fn stages_session_guard_when_protected_but_session_exposed() {
        let probe = OomProbe {
            oomd_active: true,
            oomd_has_policies: true,
            user_pressure_kill: true,
            ..unprotected()
        };
        let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
        stage_oom_fixes(&mut items, &mut pending, &probe, 0, false);
        assert!(pending.enable_oomd);
        assert!(items[0].summary.contains("session"));
    }

    #[test]
    fn advice_only_when_missing_or_blocked() {
        let missing = OomProbe {
            oomd_installed: false,
            ..unprotected()
        };
        let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
        stage_oom_fixes(&mut items, &mut pending, &missing, 2, false);
        stage_oom_fixes(&mut items, &mut pending, &unprotected(), 0, true);
        assert!(!pending.enable_oomd);
        assert!(items.iter().all(|i| !i.will_stage));
        assert!(items.iter().any(|i| i.summary.contains("fired recently")));
    }
}
