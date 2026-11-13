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

#[test]
fn help_documents_exit_codes() {
    Command::new(cargo_bin("xzram"))
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Exit codes"));
}

#[test]
fn usage_error_exits_2() {
    Command::new(cargo_bin("xzram"))
        .arg("not-a-real-command")
        .assert()
        .code(2);
}

#[test]
fn prune_keep_out_of_range_is_usage_error() {
    Command::new(cargo_bin("xzram"))
        .args(["snapshot", "prune", "--keep", "4294967297", "--yes"])
        .assert()
        .code(2);
    Command::new(cargo_bin("xzram"))
        .args(["snapshot", "prune", "--keep", "0", "--yes"])
        .assert()
        .code(2);
}

#[test]
fn doctor_exit_code_is_0_or_3() {
    let output = std::process::Command::new(cargo_bin("xzram"))
        .args(["doctor", "--json"])
        .output()
        .expect("run xzram doctor");
    let code = output.status.code().expect("exit code");
    assert!(code == 0 || code == 3, "unexpected doctor exit code {code}");
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor prints JSON report");
    assert_eq!(report["healthy"].as_bool(), Some(code == 0));
}

#[test]
fn apply_offers_at_boot_and_force() {
    Command::new(cargo_bin("xzram"))
        .args(["apply", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--at-boot").and(predicate::str::contains("--force")));
}

#[test]
fn at_boot_and_force_conflict() {
    Command::new(cargo_bin("xzram"))
        .args(["apply", "--at-boot", "--force"])
        .assert()
        .code(2);
}

#[test]
fn zram_set_force_needs_now() {
    // Rejected before any privileged call.
    Command::new(cargo_bin("xzram"))
        .args(["zram", "set", "--size", "ram", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--now"));
}

#[test]
fn drain_overflow_is_listed() {
    Command::new(cargo_bin("xzram"))
        .args(["swap", "drain-overflow", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--dry-run"));
}
