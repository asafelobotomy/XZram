//! Drop development overrides before a release helper does anything as root.
//!
//! `XZRAM_DATA_DIR` and `XZRAM_ETC_ROOT` redirect the state store and `/etc`, and
//! `XZRAM_ETC_ROOT` also turns off the root-ownership checks on swapfile paths
//! (`validation::secure::ownership_checks_enforced`). pkexec already clears the
//! environment, but a root launch that keeps it (`sudo -E`, a wrapper script) must not get
//! those test semantics. Debug builds keep them for the hermetic tests.

/// The one `XZRAM_*` variable a root helper needs: where to leave its error message.
/// `ErrorSink` validates the path itself.
const KEPT: &[&str] = &["XZRAM_ERROR_FILE"];

/// Whether `key` is an override the root helper must ignore.
fn is_dev_override(key: &str) -> bool {
    key.starts_with("XZRAM_") && !KEPT.contains(&key)
}

/// Remove every dev override when running as root in a release build, and say so on
/// stderr. Call first thing in `main`, before any thread starts.
pub fn scrub_dev_overrides() {
    // SAFETY: geteuid has no preconditions and cannot fail.
    if cfg!(debug_assertions) || unsafe { libc::geteuid() } != 0 {
        return;
    }
    let keys: Vec<_> = std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .filter(|k| is_dev_override(k))
        .collect();
    for key in keys {
        eprintln!("xzram-helper: ignoring {key} (development override) when running as root");
        std::env::remove_var(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::is_dev_override;

    #[test]
    fn only_the_error_file_survives() {
        assert!(is_dev_override("XZRAM_ETC_ROOT"));
        assert!(is_dev_override("XZRAM_DATA_DIR"));
        assert!(is_dev_override("XZRAM_SOMETHING_NEW"));
        assert!(!is_dev_override("XZRAM_ERROR_FILE"));
        assert!(!is_dev_override("RUST_LOG"));
        assert!(!is_dev_override("PATH"));
    }
}
