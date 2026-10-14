//! Kernel memory tunables staged with recommended defaults: MGLRU thrashing prevention
//! (`min_ttl_ms`) and THP `defrag=defer+madvise`.

use crate::apply::PendingConfig;
use crate::mm_tunables::{MmTunables, RECOMMENDED_MIN_TTL_MS, RECOMMENDED_THP_DEFRAG};

use super::types::RecommendationItem;

/// Live kernel state needed to decide (injected in tests).
#[derive(Debug, Clone, Default)]
pub(crate) struct MmProbe {
    pub mglru_enabled: bool,
    pub current: MmTunables,
}

pub(crate) fn probe_mm() -> MmProbe {
    MmProbe {
        mglru_enabled: crate::mm_tunables::mglru_enabled(),
        current: crate::mm_tunables::current(),
    }
}

pub(super) fn stage_mm_fixes(
    items: &mut Vec<RecommendationItem>,
    pending: &mut PendingConfig,
    probe: &MmProbe,
    staging_blocked: bool,
) {
    if staging_blocked {
        return;
    }
    let mut wanted = MmTunables::default();

    if probe.mglru_enabled && probe.current.mglru_min_ttl_ms == Some(0) {
        items.push(RecommendationItem {
            category: "memory".into(),
            summary: format!("Enable MGLRU thrashing prevention (min_ttl_ms={RECOMMENDED_MIN_TTL_MS})"),
            detail: "Protects the last second of the working set from eviction; if it cannot stay resident the kernel OOM-kills instead of thrashing. Kernel docs: 1000 ms usually eliminates intolerable jank. Persisted via /etc/tmpfiles.d/xzram-mm.conf.".into(),
            will_stage: true,
            reference: Some("mm-tunables".into()),
        });
        wanted.mglru_min_ttl_ms = Some(RECOMMENDED_MIN_TTL_MS);
    }

    // Only replace the kernel defaults; leave a deliberately chosen defer/never alone.
    if matches!(
        probe.current.thp_defrag.as_deref(),
        Some("madvise") | Some("always")
    ) {
        items.push(RecommendationItem {
            category: "memory".into(),
            summary: format!("Set THP defrag to {RECOMMENDED_THP_DEFRAG}"),
            detail: "Huge-page compaction runs in the background except for regions that asked for huge pages, avoiding allocation stalls under memory pressure. Persisted via /etc/tmpfiles.d/xzram-mm.conf.".into(),
            will_stage: true,
            reference: Some("mm-tunables".into()),
        });
        wanted.thp_defrag = Some(RECOMMENDED_THP_DEFRAG.into());
    }

    if !wanted.is_empty() {
        pending.mm = Some(wanted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_defaults() -> MmProbe {
        MmProbe {
            mglru_enabled: true,
            current: MmTunables {
                mglru_min_ttl_ms: Some(0),
                thp_defrag: Some("madvise".into()),
            },
        }
    }

    #[test]
    fn stages_both_on_kernel_defaults() {
        let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
        stage_mm_fixes(&mut items, &mut pending, &kernel_defaults(), false);
        let mm = pending.mm.expect("mm staged");
        assert_eq!(mm.mglru_min_ttl_ms, Some(RECOMMENDED_MIN_TTL_MS));
        assert_eq!(mm.thp_defrag.as_deref(), Some(RECOMMENDED_THP_DEFRAG));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn respects_existing_choices_and_missing_mglru() {
        let probe = MmProbe {
            mglru_enabled: false,
            current: MmTunables {
                mglru_min_ttl_ms: Some(0),
                thp_defrag: Some("never".into()),
            },
        };
        let (mut items, mut pending) = (Vec::new(), PendingConfig::default());
        stage_mm_fixes(&mut items, &mut pending, &probe, false);
        stage_mm_fixes(&mut items, &mut pending, &kernel_defaults(), true);
        assert!(pending.mm.is_none());
        assert!(items.is_empty());
    }
}
