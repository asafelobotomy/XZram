//! Staging methods of the Manager D-Bus interface (bodies kept out of `mod.rs`).

use xzram::apply::{pending_is_empty, stage, PendingConfig, ZramConfig};
use xzram::recommend;
use xzram::validation;
use zbus::message::Header;

use super::auth::authorize;
use super::util::{json_map, validate_staged_pending};
use super::{reads, JsonReply, Manager};

pub(super) async fn stage_recommended_defaults(
    mgr: &Manager,
    hdr: Header<'_>,
) -> zbus::fdo::Result<JsonReply> {
    let report = reads::bounded(mgr, recommend::recommend).await?;
    if pending_is_empty(&report.pending) {
        return Ok(json_map(&report));
    }
    authorize(&mgr.connection, &hdr, "io.github.xzram.stage").await?;
    let _guard = mgr.gate.lock().await;
    validate_staged_pending(&report.pending)?;
    stage(&report.pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(json_map(&report))
}

pub(super) async fn configure_zram(
    mgr: &Manager,
    hdr: Header<'_>,
    config_json: &str,
) -> zbus::fdo::Result<()> {
    authorize(&mgr.connection, &hdr, "io.github.xzram.stage").await?;
    let config: ZramConfig = validation::from_json_strict(config_json)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    validation::validate_zram_config(&config)
        .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
    let pending = PendingConfig {
        zram: Some(config),
        ..Default::default()
    };
    mgr.stage_locked(&pending).await
}
