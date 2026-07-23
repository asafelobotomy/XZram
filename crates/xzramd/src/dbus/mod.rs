mod auth;
mod serve;
mod util;

use std::collections::HashMap;

use xzram::apply::{
    clear_pending, load_pending, pending_is_empty, stage, PendingConfig, SwapfileConfig,
    SwapfileResizeConfig, ZramConfig,
};
use xzram::backend::{available_swapfile_backend, ensure_zram_backend};
use xzram::detect;
use xzram::doctor;
use xzram::migrate::migrate_from_zram_tools;
use xzram::recommend;
use xzram::snapshot::{self, SnapshotTrigger};
use xzram::status;
use xzram::swap_partition;
use xzram::swapfile_btrfs;
use xzram::sysctl::{self, SysctlValues};
use xzram::validation;
use zbus::interface;
use zbus::message::Header;
use zbus::zvariant::OwnedValue;

use auth::authorize;
use util::{json_map, validate_staged_pending};

pub use serve::serve;

type JsonReply = HashMap<String, OwnedValue>;

pub struct Manager {
    connection: zbus::Connection,
    /// Serializes stage/clear/apply/snapshot mutations against pending.json / snapshot store.
    gate: tokio::sync::Mutex<()>,
}

impl Manager {
    pub fn new(connection: zbus::Connection) -> Self {
        Self {
            connection,
            gate: tokio::sync::Mutex::new(()),
        }
    }
}

#[interface(name = "io.github.XZram.Manager")]
impl Manager {
    async fn get_status(&self) -> zbus::fdo::Result<JsonReply> {
        let report = status::status().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn get_detection(&self) -> zbus::fdo::Result<JsonReply> {
        let report = detect::detect().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn run_doctor(&self) -> zbus::fdo::Result<JsonReply> {
        let report = doctor::doctor().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn get_zram_config(&self) -> zbus::fdo::Result<JsonReply> {
        let backend = ensure_zram_backend().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let config = backend
            .show()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&config))
    }

    async fn list_swapfiles(&self) -> zbus::fdo::Result<JsonReply> {
        let backend = available_swapfile_backend();
        let files = backend
            .list()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&files))
    }

    async fn list_swaps(&self) -> zbus::fdo::Result<JsonReply> {
        let swaps = swap_partition::list_swaps_merged()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&swaps))
    }

    async fn get_sysctl(&self) -> zbus::fdo::Result<JsonReply> {
        let values = sysctl::show().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&values))
    }

    async fn get_pending(&self) -> zbus::fdo::Result<JsonReply> {
        let pending = load_pending().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&pending))
    }

    async fn check_swapfile_btrfs(&self, path: &str) -> zbus::fdo::Result<JsonReply> {
        validation::validate_swapfile_path(path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let status = swapfile_btrfs::check_nodatacow(std::path::Path::new(path))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&status))
    }

    async fn prepare_swapfile_btrfs(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
        mkdir_parents: bool,
    ) -> zbus::fdo::Result<JsonReply> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.prepare").await?;
        validation::validate_swapfile_path(path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let payload = serde_json::json!({
            "path": path,
            "mkdir_parents": mkdir_parents,
        })
        .to_string();
        let lines = crate::privileged::run_helper("swapfile.prepare", &payload).await?;
        let raw = lines
            .iter()
            .rev()
            .find(|l| l.starts_with('{'))
            .ok_or_else(|| {
                zbus::fdo::Error::Failed("swapfile.prepare returned no status JSON".into())
            })?;
        let status: swapfile_btrfs::NodatacowStatus = serde_json::from_str(raw)
            .map_err(|e| zbus::fdo::Error::Failed(format!("invalid prepare status: {e}")))?;
        Ok(json_map(&status))
    }

    async fn get_recommended_defaults(&self) -> zbus::fdo::Result<JsonReply> {
        let report = recommend::recommend().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn stage_recommended_defaults(
        &self,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<JsonReply> {
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        let _guard = self.gate.lock().await;
        let report = recommend::recommend().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if !pending_is_empty(&report.pending) {
            stage(&report.pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        }
        Ok(json_map(&report))
    }

    async fn configure_zram(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        config_json: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.zram.configure").await?;
        let config: ZramConfig = serde_json::from_str(config_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let pending = PendingConfig {
            zram: Some(config),
            ..Default::default()
        };
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn disable_zram(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.zram.disable").await?;
        let pending = PendingConfig {
            disable_zram: true,
            ..Default::default()
        };
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn create_swapfile(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
        size_mb: u64,
        priority: i32,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.create").await?;
        let config = SwapfileConfig {
            path: path.into(),
            size_mb,
            priority,
        };
        validation::validate_swapfile_config(&config)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let pending = PendingConfig {
            swapfile: Some(config),
            ..Default::default()
        };
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn remove_swapfile(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.remove").await?;
        validation::validate_swapfile_path(path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let pending = PendingConfig {
            remove_swapfile: Some(path.into()),
            ..Default::default()
        };
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn resize_swapfile(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
        size_mb: u64,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.resize").await?;
        validation::validate_swapfile_path(path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        if size_mb == 0 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "size_mb must be greater than 0".into(),
            ));
        }
        let pending = PendingConfig {
            swapfile_resize: Some(SwapfileResizeConfig {
                path: path.into(),
                size_mb,
            }),
            ..Default::default()
        };
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn set_sysctl(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        values_json: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.sysctl.set").await?;
        let values: SysctlValues = serde_json::from_str(values_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let pending = PendingConfig {
            sysctl: Some(values),
            ..Default::default()
        };
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn apply(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.apply").await?;
        let _guard = self.gate.lock().await;
        crate::privileged::run_helper("apply", "{}").await
    }

    async fn rollback(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.rollback").await?;
        crate::privileged::run_helper("rollback", "{}").await
    }

    async fn list_snapshots(&self) -> zbus::fdo::Result<JsonReply> {
        let list =
            snapshot::list_snapshots().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&list))
    }

    async fn get_snapshot(&self, id: &str) -> zbus::fdo::Result<JsonReply> {
        let meta =
            snapshot::get_snapshot(id).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&meta))
    }

    async fn create_snapshot(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        trigger: &str,
        label: &str,
    ) -> zbus::fdo::Result<JsonReply> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.create").await?;
        let trigger = SnapshotTrigger::parse(trigger)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let label_opt = if label.is_empty() { None } else { Some(label) };
        let _guard = self.gate.lock().await;
        let meta = snapshot::create_snapshot(trigger, label_opt, None)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&meta))
    }

    async fn restore_snapshot(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        id: &str,
    ) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.restore").await?;
        let payload = serde_json::json!({ "id": id }).to_string();
        let _guard = self.gate.lock().await;
        crate::privileged::run_helper("snapshot.restore", &payload).await
    }

    async fn delete_snapshot(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        id: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.delete").await?;
        let _guard = self.gate.lock().await;
        snapshot::delete_snapshot(id).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn prune_snapshots(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        keep: u32,
    ) -> zbus::fdo::Result<u32> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.delete").await?;
        let _guard = self.gate.lock().await;
        let removed = snapshot::prune_snapshots(keep as usize)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(removed)
    }

    async fn clear_pending(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.pending.clear").await?;
        let _guard = self.gate.lock().await;
        clear_pending().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn migrate_zram(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.zram.migrate").await?;
        let pending =
            migrate_from_zram_tools().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn stage_action(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        pending_json: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        let pending: PendingConfig = serde_json::from_str(pending_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        validate_staged_pending(&pending)?;
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn apply_now_zram(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        config_json: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.zram.configure").await?;
        let _: ZramConfig = serde_json::from_str(config_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        crate::privileged::run_helper("zram.configure", config_json).await?;
        Ok(())
    }

    async fn apply_now_swapfile_create(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        config_json: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.create").await?;
        let config: SwapfileConfig = serde_json::from_str(config_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        validation::validate_swapfile_config(&config)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        crate::privileged::run_helper("swapfile.create", config_json).await?;
        Ok(())
    }
}
