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
fn unknown_action_with_payload_fails() {
    Command::new(cargo_bin("xzram-helper"))
        .args(["not.a.real.action", "{}"])
        .assert()
        .failure();
}
