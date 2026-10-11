mod apply_now;
mod auth;
mod reads;
mod serve;
mod store_reads;
mod util;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedMutexGuard, Semaphore};
use xzram::apply::{
    clear_pending, pending_is_empty, stage, PendingConfig, SwapfileConfig, SwapfileResizeConfig,
    ZramConfig,
};
use xzram::migrate::migrate_from_zram_tools;
use xzram::recommend;
use xzram::snapshot;
use xzram::sysctl::SysctlValues;
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
    /// Held (owned) for the full lifetime of every privileged helper run.
    gate: Arc<Mutex<()>>,
    /// Bounds concurrent unauthenticated read methods (see `reads`).
    read_slots: Arc<Semaphore>,
}

impl Manager {
    pub fn new(connection: zbus::Connection) -> Self {
        Self {
            connection,
            gate: Arc::new(Mutex::new(())),
            read_slots: reads::read_slots(),
        }
    }

    async fn lock_gate(&self) -> OwnedMutexGuard<()> {
        self.gate.clone().lock_owned().await
    }
}

#[interface(name = "io.github.XZram.Manager")]
impl Manager {
    async fn get_status(&self) -> zbus::fdo::Result<JsonReply> {
        reads::get_status(self).await
    }

    async fn get_detection(&self) -> zbus::fdo::Result<JsonReply> {
        reads::get_detection(self).await
    }

    async fn run_doctor(&self) -> zbus::fdo::Result<JsonReply> {
        reads::run_doctor(self).await
    }

    async fn get_zram_config(&self) -> zbus::fdo::Result<JsonReply> {
        reads::get_zram_config(self).await
    }

    async fn list_swapfiles(&self) -> zbus::fdo::Result<JsonReply> {
        reads::list_swapfiles(self).await
    }

    async fn list_swaps(&self) -> zbus::fdo::Result<JsonReply> {
        reads::list_swaps(self).await
    }

    async fn get_sysctl(&self) -> zbus::fdo::Result<JsonReply> {
        reads::get_sysctl(self).await
    }

    async fn get_pending(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<JsonReply> {
        store_reads::get_pending(self, hdr).await
    }

    async fn check_swapfile_btrfs(&self, path: &str) -> zbus::fdo::Result<JsonReply> {
        reads::check_swapfile_btrfs(self, path).await
    }

    async fn prepare_swapfile_btrfs(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
        mkdir_parents: bool,
    ) -> zbus::fdo::Result<JsonReply> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.prepare").await?;
        util::prepare_swapfile_btrfs(self.lock_gate().await, path, mkdir_parents).await
    }

    async fn get_recommended_defaults(&self) -> zbus::fdo::Result<JsonReply> {
        reads::get_recommended_defaults(self).await
    }

    async fn stage_recommended_defaults(
        &self,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<JsonReply> {
        let report = reads::bounded(self, recommend::recommend).await?;
        if pending_is_empty(&report.pending) {
            return Ok(json_map(&report));
        }
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        let _guard = self.gate.lock().await;
        validate_staged_pending(&report.pending)?;
        stage(&report.pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn configure_zram(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        config_json: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        let config: ZramConfig = serde_json::from_str(config_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        validation::validate_zram_config(&config)
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
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
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
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
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
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        validation::validate_swapfile_remove_path(path)
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
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        validation::validate_swapfile_resize_path(path, size_mb)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
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
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
        let values: SysctlValues = serde_json::from_str(values_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let pending = PendingConfig {
            sysctl: Some(values),
            ..Default::default()
        };
        validate_staged_pending(&pending)?;
        let _guard = self.gate.lock().await;
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn apply(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.apply").await?;
        crate::privileged::run_helper(self.lock_gate().await, "apply", "{}").await
    }

    async fn rollback(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.rollback").await?;
        crate::privileged::run_helper(self.lock_gate().await, "rollback", "{}").await
    }

    async fn list_snapshots(
        &self,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<JsonReply> {
        store_reads::list_snapshots(self, hdr).await
    }

    async fn get_snapshot(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        id: &str,
    ) -> zbus::fdo::Result<JsonReply> {
        store_reads::get_snapshot(self, hdr, id).await
    }

    async fn create_snapshot(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        trigger: &str,
        label: &str,
    ) -> zbus::fdo::Result<JsonReply> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.create").await?;
        let trigger = util::caller_snapshot_trigger(trigger)?;
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
        crate::privileged::run_helper(self.lock_gate().await, "snapshot.restore", &payload).await
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
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.prune").await?;
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
        validate_staged_pending(&pending)?;
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
        apply_now::apply_now_zram(self, hdr, config_json).await
    }

    async fn apply_now_zram_disable(
        &self,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        apply_now::apply_now_zram_disable(self, hdr).await
    }

    async fn apply_now_swapfile_create(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        config_json: &str,
    ) -> zbus::fdo::Result<()> {
        apply_now::apply_now_swapfile_create(self, hdr, config_json).await
    }

    async fn apply_now_swapfile_resize(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
        size_mb: u64,
    ) -> zbus::fdo::Result<()> {
        apply_now::apply_now_swapfile_resize(self, hdr, path, size_mb).await
    }

    async fn apply_now_swapfile_remove(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        path: &str,
    ) -> zbus::fdo::Result<()> {
        apply_now::apply_now_swapfile_remove(self, hdr, path).await
    }

    async fn apply_now_sysctl(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        values_json: &str,
    ) -> zbus::fdo::Result<()> {
        apply_now::apply_now_sysctl(self, hdr, values_json).await
    }
}
