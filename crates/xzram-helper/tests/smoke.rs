use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_exits_success() {
    Command::new(cargo_bin("xzram-helper"))
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("xzram-helper"));
}

#[test]
fn missing_args_fails() {
    Command::new(cargo_bin("xzram-helper"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage").or(predicate::str::contains("required")));
}

#[test]
fn snapshot_create_rejects_pre_apply_and_ignores_bad_error_file() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("evil-error-file");
    Command::new(cargo_bin("xzram-helper"))
        .env("XZRAM_DATA_DIR", dir.path())
        .env("XZRAM_ERROR_FILE", &outside)
        .args(["snapshot.create", r#"{"trigger":"pre_apply"}"#])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
    // A per-run error file outside /run/xzram or /var/lib/xzram is refused; the error
    // falls back to the global last_error instead.
    assert!(!outside.exists());
    let last = std::fs::read_to_string(dir.path().join("last_error")).unwrap();
    assert!(last.contains("reserved"));
}

#[test]
fn unknown_action_with_payload_fails() {
    Command::new(cargo_bin("xzram-helper"))
        .args(["not.a.real.action", "{}"])
        .assert()
        .failure();
}

/// A field this helper doesn't know means the client is newer; refuse instead of
/// silently dropping it (checked before anything touches the system).
#[test]
fn unknown_payload_fields_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    let stage = serde_json::json!({
        "pending": xzram::apply::PendingConfig::default(),
        "prepare_swapfile": {"path": "/swap/swapfile"},
        "extra": 1,
    })
    .to_string();
    let cases = [
        ("snapshot.delete", r#"{"id":"x","bogus":1}"#),
        ("snapshot.prune", r#"{"keep":5,"force":true}"#),
        ("snapshot.restore", r#"{"id":"x","dry_run":true}"#),
        ("snapshot.create", r#"{"trigger":"manual","pin":true}"#),
        ("zram.migrate", r#"{"apply_now":false,"extra":1}"#),
        (
            "swap.activate",
            r#"{"action":"on","device":"/dev/zram0","pri":5}"#,
        ),
        ("swapfile.remove", r#"{"path":"/swap/swapfile","now":true}"#),
        (
            "swapfile.resize",
            r#"{"path":"/swap/swapfile","size_mb":512,"x":1}"#,
        ),
        (
            "swapfile.create",
            r#"{"path":"/swap/swapfile","size_mb":512,"priority":10,"prepare":false,"x":1}"#,
        ),
        (
            "swapfile.prepare",
            r#"{"path":"/swap/swapfile","mkdir":true}"#,
        ),
        ("stage", stage.as_str()),
    ];
    for (action, payload) in cases {
        Command::new(cargo_bin("xzram-helper"))
            .env("XZRAM_DATA_DIR", dir.path())
            .env("XZRAM_ETC_ROOT", etc.path())
            .args([action, payload])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unsupported field"));
    }
}

#[test]
fn known_swapfile_create_fields_still_parse() {
    // Gets past parsing (prepare flags flattened alongside the config) and fails later,
    // on validation or the backend, not on "unsupported field".
    let dir = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    Command::new(cargo_bin("xzram-helper"))
        .env("XZRAM_DATA_DIR", dir.path())
        .env("XZRAM_ETC_ROOT", etc.path())
        .args([
            "swapfile.create",
            r#"{"path":"/nonexistent-xzram/swapfile","size_mb":512,"priority":10,"prepare":true,"mkdir_parents":false}"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported field").not());
}

/// Regression: `swapfile.prepare` was dropped from the helper in 321173e, so the command
/// doctor recommends ("xzram swapfile prepare") always failed with "unknown action".
#[test]
fn swapfile_prepare_is_a_known_action() {
    let dir = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    Command::new(cargo_bin("xzram-helper"))
        .env("XZRAM_DATA_DIR", dir.path())
        .env("XZRAM_ETC_ROOT", etc.path())
        .args([
            "swapfile.prepare",
            r#"{"path":"/nonexistent-xzram/swapfile","mkdir_parents":false}"#,
        ])
        .assert()
        .stderr(predicate::str::contains("unknown action").not());
}
