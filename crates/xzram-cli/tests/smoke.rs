use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_exits_success() {
    Command::new(cargo_bin("xzram"))
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn status_help_exits_success() {
    Command::new(cargo_bin("xzram"))
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"));
}

#[test]
fn defaults_help_exits_success() {
    Command::new(cargo_bin("xzram"))
        .args(["defaults", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recommend"));
}

#[test]
fn unknown_subcommand_fails() {
    Command::new(cargo_bin("xzram"))
        .arg("not-a-real-command")
        .assert()
        .failure();
}
