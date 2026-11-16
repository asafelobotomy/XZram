//! `--ndjson`: a command that takes a while prints one JSON object per line while it works,
//! so a caller (the uQ Settings page) can show a real progress bar.
//!
//! ```text
//! {"event":"start","label":"Waiting for permission"}
//! {"event":"progress","label":"Moving what is in swap back into memory","percent":40,"detail":"1.2 GB left"}
//! {"event":"done","ok":true,"percent":100, ...}            or
//! {"event":"error","ok":false,"kind":"memory","error":"…"}
//! ```
//!
//! Progress is measured, not guessed: while the privileged step runs, the bytes still
//! stored in swap (zram data, or one swap file) are sampled, and the percent is how much of
//! the starting amount has moved. With nothing in swap there is no percent, only the two
//! ends. Events are rate-limited to a few per second.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{json, Value};
use xzram::apply::MEMORY_REFUSAL;
use xzram::status::{self, format_bytes};

const POLL: Duration = Duration::from_millis(400);

/// What to measure while the privileged step runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Watch {
    /// Nothing to measure: only the start and the end are reported.
    Nothing,
    /// Data stored in active zram devices.
    Zram,
    /// Data stored in one swap file.
    SwapFile(String),
    /// Data in every swap area.
    AllSwap,
}

fn emit(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// Bytes still in the watched swap, if it can be read.
pub(crate) fn sample(watch: &Watch) -> Option<u64> {
    let report = status::status().ok()?;
    match watch {
        Watch::Nothing => None,
        Watch::Zram => Some(
            report
                .zram_devices
                .iter()
                .filter(|d| d.mount_point == "[SWAP]")
                .map(|d| d.data_bytes)
                .sum(),
        ),
        Watch::SwapFile(path) => Some(
            report
                .swaps
                .iter()
                .filter(|s| &s.name == path)
                .map(|s| s.used_bytes)
                .sum(),
        ),
        Watch::AllSwap => Some(report.swaps.iter().map(|s| s.used_bytes).sum()),
    }
}

/// How far along a move is, 0 to 99: what share of `start` has left. `None` with nothing to
/// move. Never 100: only the end of the step says that.
pub(crate) fn percent(start: u64, now: u64) -> Option<u8> {
    if start == 0 {
        return None;
    }
    let moved = start.saturating_sub(now) as f64 / start as f64;
    Some((moved * 100.0).floor().clamp(0.0, 99.0) as u8)
}

/// The sort of failure, for the page to word: `memory`, `denied` or `other`.
pub(crate) fn error_kind(message: &str) -> &'static str {
    let low = message.to_lowercase();
    if low.contains(MEMORY_REFUSAL) {
        "memory"
    } else if low.contains("authorization") || low.contains("not authorized") {
        "denied"
    } else {
        "other"
    }
}

/// Runs `step`. With `ndjson`, reports around it and while it runs; the final event carries
/// `extra` (what was done) on success. The step's own error is returned unchanged.
pub(crate) fn run<F>(
    ndjson: bool,
    label: &str,
    watch: Watch,
    extra: Value,
    step: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<()>,
{
    if !ndjson {
        return step();
    }
    emit(&json!({ "event": "start", "label": label }));
    let start = sample(&watch).filter(|b| *b > 0);
    let stop = AtomicBool::new(false);
    let (stop_ref, watch_ref) = (&stop, &watch);
    let result = std::thread::scope(|scope| {
        if let Some(start) = start {
            scope.spawn(move || {
                let mut last = None;
                while !stop_ref.load(Ordering::Relaxed) {
                    std::thread::sleep(POLL);
                    let Some(now) = sample(watch_ref) else {
                        continue;
                    };
                    let p = percent(start, now);
                    if p != last && !stop_ref.load(Ordering::Relaxed) {
                        last = p;
                        emit(&json!({
                            "event": "progress",
                            "label": "Moving what is in swap back into memory",
                            "percent": p,
                            "detail": format!("{} left to move", format_bytes(now)),
                        }));
                    }
                }
            });
        }
        let result = step();
        stop.store(true, Ordering::Relaxed);
        result
    });
    match &result {
        Ok(()) => {
            let mut done = json!({ "event": "done", "ok": true, "percent": 100 });
            if let (Some(d), Some(e)) = (done.as_object_mut(), extra.as_object()) {
                d.extend(e.clone());
            }
            emit(&done);
        }
        Err(e) => {
            let message = format!("{e:#}");
            emit(
                &json!({ "event": "error", "ok": false, "kind": error_kind(&message), "error": message }),
            );
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_the_share_that_has_moved_and_never_100() {
        assert_eq!(percent(1000, 1000), Some(0));
        assert_eq!(percent(1000, 600), Some(40));
        assert_eq!(percent(1000, 0), Some(99));
        // More data than at the start (it grew meanwhile) is still 0, not negative.
        assert_eq!(percent(1000, 1500), Some(0));
    }

    #[test]
    fn nothing_to_move_has_no_percent() {
        assert_eq!(percent(0, 0), None);
    }

    #[test]
    fn failures_are_sorted_for_the_page() {
        assert_eq!(
            error_kind("not enough free memory: restarting zram0 brings"),
            "memory"
        );
        assert_eq!(
            error_kind("Authorization cancelled (authentication dialog dismissed)"),
            "denied"
        );
        assert_eq!(
            error_kind("Authorization denied (not authorized for this action)"),
            "denied"
        );
        assert_eq!(error_kind("disk full"), "other");
    }

    #[test]
    fn without_ndjson_the_step_just_runs() {
        let mut ran = false;
        run(false, "x", Watch::Nothing, json!({}), || {
            ran = true;
            Ok(())
        })
        .unwrap();
        assert!(ran);
        let err = run(false, "x", Watch::Nothing, json!({}), || {
            anyhow::bail!("boom")
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "boom");
    }
}
