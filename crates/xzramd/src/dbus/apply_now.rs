//! Immediate apply helpers for the Manager D-Bus interface.

use xzram::apply::{SwapfileConfig, SwapfileResizeConfig, ZramConfig};
use xzram::sysctl::SysctlValues;
use xzram::validation;
use zbus::message::Header;

use super::auth::authorize;
use super::Manager;

pub(super) async fn apply_now_zram(
    mgr: &Manager,
    hdr: Header<'_>,
    config_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.zram.configure").await?;
    let config: ZramConfig = serde_json::from_str(config_json)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    validation::validate_zram_config(&config)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let _guard = mgr.gate.lock().await;
    crate::privileged::run_helper("zram.configure", config_json).await?;
    Ok(())
}

pub(super) async fn apply_now_zram_disable(
    mgr: &Manager,
    hdr: Header<'_>,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.zram.disable").await?;
    let _guard = mgr.gate.lock().await;
    crate::privileged::run_helper("zram.disable", "{}").await?;
    Ok(())
}

pub(super) async fn apply_now_swapfile_create(
    mgr: &Manager,
    hdr: Header<'_>,
    config_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.swapfile.create").await?;
    let config: SwapfileConfig = serde_json::from_str(config_json)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    validation::validate_swapfile_config(&config)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let _guard = mgr.gate.lock().await;
    crate::privileged::run_helper("swapfile.create", config_json).await?;
    Ok(())
}

pub(super) async fn apply_now_swapfile_resize(
    mgr: &Manager,
    hdr: Header<'_>,
    path: &str,
    size_mb: u64,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.swapfile.resize").await?;
    validation::validate_swapfile_resize_path(path, size_mb)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let payload = serde_json::to_string(&SwapfileResizeConfig {
        path: path.into(),
        size_mb,
    })
    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    let _guard = mgr.gate.lock().await;
    crate::privileged::run_helper("swapfile.resize", &payload).await?;
    Ok(())
}

pub(super) async fn apply_now_swapfile_remove(
    mgr: &Manager,
    hdr: Header<'_>,
    path: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.swapfile.remove").await?;
    validation::validate_swapfile_remove_path(path)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let payload = serde_json::json!({ "path": path }).to_string();
    let _guard = mgr.gate.lock().await;
    crate::privileged::run_helper("swapfile.remove", &payload).await?;
    Ok(())
}

pub(super) async fn apply_now_sysctl(
    mgr: &Manager,
    hdr: Header<'_>,
    values_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.sysctl.set").await?;
    let _: SysctlValues = serde_json::from_str(values_json)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let _guard = mgr.gate.lock().await;
    crate::privileged::run_helper("sysctl.set", values_json).await?;
    Ok(())
}
