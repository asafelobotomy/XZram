use std::collections::HashMap;

use tracing::info;
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
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

pub struct Manager {
    connection: zbus::Connection,
}

impl Manager {
    pub fn new(connection: zbus::Connection) -> Self {
        Self { connection }
    }
}

fn json_map<T: serde::Serialize>(value: &T) -> HashMap<String, zbus::zvariant::OwnedValue> {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    let mut map = HashMap::new();
    let owned: zbus::zvariant::OwnedValue = zbus::zvariant::Value::from(json)
        .try_into()
        .expect("json string is a valid D-Bus value");
    map.insert("json".into(), owned);
    map
}

#[interface(name = "io.github.XZram.Manager")]
impl Manager {
    async fn get_status(&self) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let report = status::status().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn get_detection(
        &self,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let report = detect::detect().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn run_doctor(&self) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let report = doctor::doctor().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn get_zram_config(
        &self,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let backend = ensure_zram_backend().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let config = backend
            .show()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&config))
    }

    async fn list_swapfiles(
        &self,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let backend = available_swapfile_backend();
        let files = backend
            .list()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&files))
    }

    async fn list_swaps(&self) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let swaps = swap_partition::list_swaps_merged()
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&swaps))
    }

    async fn get_sysctl(&self) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let values = sysctl::show().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&values))
    }

    async fn get_pending(&self) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let pending = load_pending().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&pending))
    }

    async fn check_swapfile_btrfs(
        &self,
        path: &str,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
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
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        authorize(&self.connection, &hdr, "io.github.xzram.swapfile.prepare").await?;
        validation::validate_swapfile_path(path)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let status = swapfile_btrfs::prepare_nodatacow(std::path::Path::new(path), mkdir_parents)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&status))
    }

    async fn get_recommended_defaults(
        &self,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let report = recommend::recommend().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&report))
    }

    async fn stage_recommended_defaults(
        &self,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        authorize(&self.connection, &hdr, "io.github.xzram.stage").await?;
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
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn disable_zram(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.zram.disable").await?;
        let pending = PendingConfig {
            disable_zram: true,
            ..Default::default()
        };
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
        stage(&pending).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn apply(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.apply").await?;
        crate::privileged::run_helper("apply", "{}")
    }

    async fn rollback(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<Vec<String>> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.restore").await?;
        crate::privileged::run_helper("rollback", "{}")
    }

    async fn list_snapshots(
        &self,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let list =
            snapshot::list_snapshots().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&list))
    }

    async fn get_snapshot(
        &self,
        id: &str,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let meta =
            snapshot::get_snapshot(id).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(json_map(&meta))
    }

    async fn create_snapshot(
        &self,
        trigger: &str,
        label: &str,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let trigger = SnapshotTrigger::from_str(trigger)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let label_opt = if label.is_empty() { None } else { Some(label) };
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
        crate::privileged::run_helper("snapshot.restore", &payload)
    }

    async fn delete_snapshot(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        id: &str,
    ) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.delete").await?;
        snapshot::delete_snapshot(id).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn prune_snapshots(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        keep: u32,
    ) -> zbus::fdo::Result<u32> {
        authorize(&self.connection, &hdr, "io.github.xzram.snapshot.delete").await?;
        let removed = snapshot::prune_snapshots(keep as usize)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(removed)
    }

    async fn clear_pending(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.pending.clear").await?;
        clear_pending().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn migrate_zram(&self, #[zbus(header)] hdr: Header<'_>) -> zbus::fdo::Result<()> {
        authorize(&self.connection, &hdr, "io.github.xzram.zram.migrate").await?;
        let pending =
            migrate_from_zram_tools().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
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
        if let Some(ref swapfile) = pending.swapfile {
            validation::validate_swapfile_config(swapfile)
                .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        }
        if let Some(ref path) = pending.remove_swapfile {
            validation::validate_swapfile_path(path)
                .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        }
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
        crate::privileged::run_helper("zram.configure", config_json)?;
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
        crate::privileged::run_helper("swapfile.create", config_json)?;
        Ok(())
    }
}

async fn authorize(
    connection: &zbus::Connection,
    header: &Header<'_>,
    action_id: &str,
) -> zbus::fdo::Result<()> {
    let proxy = AuthorityProxy::new(connection)
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

    let subject = subject_from_header(connection, header).await?;
    let result = proxy
        .check_authorization(
            &subject,
            action_id,
            &HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

    if result.is_authorized {
        Ok(())
    } else {
        Err(zbus::fdo::Error::AccessDenied(format!(
            "polkit denied action {action_id}"
        )))
    }
}

async fn subject_from_header(
    connection: &zbus::Connection,
    _header: &Header<'_>,
) -> zbus::fdo::Result<Subject> {
    let creds = connection
        .peer_creds()
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

    let pid = creds
        .process_id()
        .ok_or_else(|| zbus::fdo::Error::Failed("could not determine caller PID".into()))?;
    let uid = creds.unix_user_id();

    Subject::new_for_owner(pid, None, uid).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
}

pub async fn serve() -> anyhow::Result<()> {
    let connection = zbus::connection::Builder::system()?
        .name("io.github.XZram1")?
        .build()
        .await?;

    connection
        .object_server()
        .at("/io/github/XZram", Manager::new(connection.clone()))
        .await?;

    info!("xzramd listening on system bus as io.github.XZram1");
    tokio::signal::ctrl_c().await?;
    connection.release_name("io.github.XZram1").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_map_contains_json_key() {
        let map = json_map(&serde_json::json!({"ok": true}));
        assert!(map.contains_key("json"));
    }
}
