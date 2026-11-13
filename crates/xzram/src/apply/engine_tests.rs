//! Tests for the apply engine (kept apart to stay under the per-file line limit).

use super::engine::*;
use super::pending::{load_pending, write_pending};
use super::types::PendingConfig;
use crate::apply::test_lock;
use crate::sysctl::SysctlValues;
use std::fs;

#[test]
fn apply_pending_empty_errors() {
    let _guard = test_lock();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", dir.path());
    write_pending(&PendingConfig::default()).unwrap();
    let err = apply_pending().unwrap_err().to_string();
    assert!(err.contains("empty"));
    std::env::remove_var("XZRAM_DATA_DIR");
}

#[test]
fn failed_apply_rolls_back_and_keeps_pending() {
    let _guard = test_lock();
    let data = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", data.path());
    std::env::set_var("XZRAM_ETC_ROOT", etc.path());
    fs::create_dir_all(etc.path().join("sysctl.d")).unwrap();
    let drop_in = etc.path().join("sysctl.d/99-xzram.conf");
    fs::write(&drop_in, "vm.swappiness = 60\n").unwrap();

    // zram step writes the generator config; the out-of-range sysctl step then fails.
    let pending = PendingConfig {
        zram: Some(crate::apply::types::ZramConfig {
            device: "zram0".into(),
            zram_size: Some("4096".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        }),
        sysctl: Some(SysctlValues {
            swappiness: Some(999),
            watermark_boost_factor: None,
            watermark_scale_factor: None,
            page_cluster: None,
            ..Default::default()
        }),
        ..Default::default()
    };
    write_pending(&pending).unwrap();
    let err = apply_pending().unwrap_err().to_string();

    let zram_conf_exists = etc.path().join("systemd/zram-generator.conf").exists();
    let drop_in_content = fs::read_to_string(&drop_in).unwrap();
    let pending_kept = load_pending().unwrap().is_some();
    std::env::remove_var("XZRAM_DATA_DIR");
    std::env::remove_var("XZRAM_ETC_ROOT");

    assert!(err.contains("rolled back to pre-apply snapshot"), "{err}");
    assert!(err.contains("pending configuration kept"), "{err}");
    assert!(!zram_conf_exists, "zram config should be rolled back");
    assert_eq!(drop_in_content, "vm.swappiness = 60\n");
    assert!(pending_kept);
}

#[test]
fn apply_from_pending_sysctl_under_etc_root() {
    let _guard = test_lock();
    let data = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", data.path());
    std::env::set_var("XZRAM_ETC_ROOT", etc.path());

    let pending = PendingConfig {
        sysctl: Some(SysctlValues {
            swappiness: Some(180),
            watermark_boost_factor: Some(0),
            watermark_scale_factor: Some(125),
            page_cluster: Some(0),
            ..Default::default()
        }),
        ..Default::default()
    };
    let result = apply_from_pending(&pending, super::guard::ApplyOptions::default()).unwrap();
    assert!(result.success);
    assert!(result.messages.iter().any(|m| m.contains("sysctl")));

    let drop_in = etc.path().join("sysctl.d/99-xzram.conf");
    let content = fs::read_to_string(&drop_in).unwrap();
    assert!(content.contains("vm.swappiness = 180"));
    assert!(content.contains("vm.page-cluster = 0"));

    std::env::remove_var("XZRAM_DATA_DIR");
    std::env::remove_var("XZRAM_ETC_ROOT");
}

#[test]
fn stage_then_apply_sysctl_bundle() {
    let _guard = test_lock();
    let data = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", data.path());
    std::env::set_var("XZRAM_ETC_ROOT", etc.path());

    let pending = PendingConfig {
        sysctl: Some(SysctlValues {
            swappiness: Some(150),
            watermark_boost_factor: Some(0),
            watermark_scale_factor: Some(125),
            page_cluster: Some(0),
            ..Default::default()
        }),
        ..Default::default()
    };
    let result = stage_then_apply(&pending).unwrap();
    assert!(result.success);
    assert!(load_pending().unwrap().is_none());
    let drop_in = etc.path().join("sysctl.d/99-xzram.conf");
    assert!(fs::read_to_string(drop_in)
        .unwrap()
        .contains("vm.swappiness = 150"));

    std::env::remove_var("XZRAM_DATA_DIR");
    std::env::remove_var("XZRAM_ETC_ROOT");
}

#[test]
fn disable_zswap_persists_drop_in_and_rollback_removes_it() {
    let _guard = test_lock();
    let data = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", data.path());
    std::env::set_var("XZRAM_ETC_ROOT", etc.path());

    let pending = PendingConfig {
        disable_zswap: true,
        ..Default::default()
    };
    let result = stage_then_apply(&pending).unwrap();
    let drop_in = etc.path().join(crate::zswap::ZSWAP_TMPFILES);
    let written = drop_in.exists();
    rollback().unwrap();
    let after_rollback = drop_in.exists();
    std::env::remove_var("XZRAM_DATA_DIR");
    std::env::remove_var("XZRAM_ETC_ROOT");

    assert!(result.messages.iter().any(|m| m.contains("zswap")));
    assert!(written);
    assert!(!after_rollback, "rollback should remove the zswap drop-in");
}

#[test]
fn oomd_and_mm_drop_ins_written_and_rolled_back() {
    let _guard = test_lock();
    let data = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", data.path());
    std::env::set_var("XZRAM_ETC_ROOT", etc.path());

    let pending = PendingConfig {
        enable_oomd: true,
        mm: Some(crate::mm_tunables::MmTunables {
            mglru_min_ttl_ms: Some(1000),
            thp_defrag: None,
        }),
        ..Default::default()
    };
    let result = stage_then_apply(&pending).unwrap();
    let drop_ins = [
        crate::oom::OOMD_ROOT_DROPIN,
        crate::oom::OOMD_USER_DROPIN,
        crate::oom::OOMD_SESSION_DROPIN,
        crate::mm_tunables::MM_TMPFILES,
    ]
    .map(|r| etc.path().join(r));
    let written = drop_ins.iter().all(|p| p.exists());
    rollback().unwrap();
    let after_rollback = drop_ins.iter().any(|p| p.exists());
    std::env::remove_var("XZRAM_DATA_DIR");
    std::env::remove_var("XZRAM_ETC_ROOT");

    assert!(result.messages.iter().any(|m| m.contains("systemd-oomd")));
    assert!(written);
    assert!(
        !after_rollback,
        "rollback should remove the oomd and mm drop-ins"
    );
}

#[test]
fn apply_at_boot_writes_generator_config_only() {
    let _guard = test_lock();
    let data = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    std::env::set_var("XZRAM_DATA_DIR", data.path());
    std::env::set_var("XZRAM_ETC_ROOT", etc.path());

    let pending = PendingConfig {
        zram: Some(crate::apply::types::ZramConfig {
            device: "zram0".into(),
            zram_size: Some("min(ram * 1.5, 65536)".into()),
            zram_resident_limit: None,
            compression_algorithm: Some("zstd".into()),
            swap_priority: Some(100),
            fs_type: None,
            mount_point: None,
        }),
        ..Default::default()
    };
    let opts = super::guard::ApplyOptions {
        at_boot: true,
        force: false,
    };
    let result = stage_then_apply_with(&pending, opts);
    let conf = fs::read_to_string(etc.path().join("systemd/zram-generator.conf"));
    std::env::remove_var("XZRAM_DATA_DIR");
    std::env::remove_var("XZRAM_ETC_ROOT");

    let result = result.unwrap();
    assert!(
        result.messages.iter().any(|m| m.contains("next boot")),
        "{:?}",
        result.messages
    );
    assert!(conf.unwrap().contains("zram-size = min(ram * 1.5, 65536)"));
}
