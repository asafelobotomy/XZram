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
