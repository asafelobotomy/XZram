//! The plan a change would carry out, printed by `--dry-run`.
//!
//! One document for every command that changes the system, so the uQ Settings page and the
//! terminal say the same thing before anything happens. The shape is the one uQ shows in
//! its confirmation sheet (`docs/settings-pages.md`, "Progress and previews"):
//! `{summary, changes: [{group, label, detail, bytes, reversible}], would_refuse, reason}`.
//! A plan that would be refused still exits 0: the refusal is the answer.

mod files;
mod snapshots;
mod zram;

pub(crate) use files::{swapfile_create, swapfile_remove, swapfile_resize};
pub(crate) use snapshots::{rollback, snapshot_delete, snapshot_prune, snapshot_restore};
pub(crate) use zram::{tuning, zram_change, zram_disable};

use serde::Serialize;

pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Change {
    pub group: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// `true`: can be put back; `false`: cannot be undone; absent: unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversible: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Plan {
    pub schema_version: u32,
    pub summary: String,
    pub changes: Vec<Change>,
    pub would_refuse: bool,
    pub reason: String,
}

impl Plan {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            summary: summary.into(),
            changes: Vec::new(),
            would_refuse: false,
            reason: String::new(),
        }
    }

    pub fn refused(summary: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            would_refuse: true,
            reason: reason.into(),
            ..Self::new(summary)
        }
    }

    pub fn change(
        mut self,
        group: &str,
        label: impl Into<String>,
        detail: impl Into<String>,
        bytes: Option<u64>,
        reversible: Option<bool>,
    ) -> Self {
        let detail = detail.into();
        self.changes.push(Change {
            group: group.into(),
            label: label.into(),
            detail: (!detail.is_empty()).then_some(detail),
            bytes,
            reversible,
        });
        self
    }

    /// Print as JSON, or as the lines a person reads.
    pub fn print(&self, json: bool) -> anyhow::Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(self)?);
            return Ok(());
        }
        println!("{}", self.summary);
        for c in &self.changes {
            println!("  [{}] {}", c.group, c.label);
            if let Some(d) = &c.detail {
                println!("      {d}");
            }
        }
        if self.would_refuse {
            println!("Would refuse: {}", self.reason);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_serialises_the_shape_uq_reads() {
        let plan = Plan::new("one change").change("Group", "Label", "", Some(5), Some(true));
        let v = serde_json::to_value(&plan).unwrap();
        assert_eq!(v["summary"], "one change");
        assert_eq!(v["would_refuse"], false);
        assert_eq!(v["reason"], "");
        assert_eq!(v["changes"][0]["group"], "Group");
        assert_eq!(v["changes"][0]["bytes"], 5);
        assert_eq!(v["changes"][0]["reversible"], true);
        // An empty detail is left out rather than sent as "".
        assert!(v["changes"][0].get("detail").is_none());
    }

    #[test]
    fn refused_plan_carries_its_reason() {
        let plan = Plan::refused("nope", "because");
        assert!(plan.would_refuse);
        assert_eq!(plan.reason, "because");
    }
}
