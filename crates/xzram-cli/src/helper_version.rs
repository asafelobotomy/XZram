//! Refuse to send privileged requests to a helper that would silently drop settings.
//!
//! Helpers before 0.4.2 parsed payloads leniently: fields they did not know (added by a
//! newer CLI) were ignored and apply still reported success. 0.4.2+ helpers reject unknown
//! fields themselves, so the CLI only needs to block the older ones. Those also predate
//! `xzram-helper --version`, so a failed version query means "too old".

use std::process::Command;
use std::sync::OnceLock;

/// First helper release that rejects unknown payload fields.
const STRICT_HELPER_MIN: (u32, u32, u32) = (0, 4, 2);

pub(crate) fn ensure_helper_supports_client(helper: &str) -> anyhow::Result<()> {
    static CHECKED: OnceLock<Result<(), String>> = OnceLock::new();
    CHECKED
        .get_or_init(|| check(helper, query_version(helper).as_deref()))
        .clone()
        .map_err(anyhow::Error::msg)
}

fn query_version(helper: &str) -> Option<String> {
    let output = Command::new(helper).arg("--version").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn check(helper: &str, reported: Option<&str>) -> Result<(), String> {
    let version = reported
        .and_then(|r| r.split_whitespace().last())
        .and_then(parse_version);
    match version {
        Some(v) if v >= STRICT_HELPER_MIN => Ok(()),
        _ => {
            let found = reported
                .and_then(|r| r.split_whitespace().last())
                .unwrap_or("older than 0.4.2");
            Err(format!(
                "{helper} is version {found}, but this xzram CLI is {}. That helper would \
                 silently ignore newer settings, so nothing was changed. Upgrade the installed \
                 xzram package (for a checkout: makepkg -si, or make install-cli) and retry.",
                env!("CARGO_PKG_VERSION")
            ))
        }
    }
}

/// `0.4.2` / `0.4.2-rc1` → (0, 4, 2).
fn parse_version(raw: &str) -> Option<(u32, u32, u32)> {
    let core = raw.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_strict_helpers_only() {
        assert!(check("/h", Some("xzram-helper 0.4.2")).is_ok());
        assert!(check("/h", Some("xzram-helper 1.0.0-rc1")).is_ok());
        let old = check("/h", Some("xzram-helper 0.4.1")).unwrap_err();
        assert!(old.contains("version 0.4.1"), "{old}");
        // Pre-0.4.2 helpers have no --version at all.
        let none = check("/h", None).unwrap_err();
        assert!(none.contains("older than 0.4.2"), "{none}");
    }

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("0.4.2"), Some((0, 4, 2)));
        assert_eq!(parse_version("0.10.0+git"), Some((0, 10, 0)));
        assert_eq!(parse_version("garbage"), None);
    }
}
