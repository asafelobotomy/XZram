//! Memory guard for operations that `swapoff` a full swap device (issue #28).
//!
//! Restarting `systemd-zram-setup@zramN` (resize, algorithm change, disable) swaps every
//! page stored in the device back into RAM first. On a machine that is already short of
//! memory, which is exactly when zram gets resized, that can stall the desktop or push
//! the pages to disk swap instead. The decision logic is pure ([`MemoryCheck`], [`decide`])
//! so it is testable with injected numbers; the live probes only read `/proc` and sysfs.

use serde::{Deserialize, Serialize};

use crate::backend::ZramBackendTrait;
use crate::error::{Result, XzramError};
use crate::status::{format_bytes, MemoryInfo, ZramDevice};

/// Phrase every refusal contains (the uQ page and GUI key on it).
pub const MEMORY_REFUSAL: &str = "not enough free memory";
/// Minimum headroom kept after the incoming pages, in bytes.
pub const MARGIN_MIN_BYTES: u64 = 512 << 20;

/// How a zram change is carried out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyOptions {
    /// Write the generator config only; the running device changes at the next boot.
    #[serde(default)]
    pub at_boot: bool,
    /// Restart the device even when the memory check fails.
    #[serde(default)]
    pub force: bool,
}

/// Headroom kept free after the incoming pages: 5% of RAM, at least 512 MiB.
pub fn margin_bytes(mem_total_bytes: u64) -> u64 {
    (mem_total_bytes / 20).max(MARGIN_MIN_BYTES)
}

/// Whether RAM can take the pages a swapoff brings back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCheck {
    /// What would be swapped off, e.g. "restarting zram0".
    pub what: String,
    /// RAM the swapped-off pages will need.
    pub incoming_bytes: u64,
    pub margin_bytes: u64,
    /// `MemAvailable`.
    pub available_bytes: u64,
}

impl MemoryCheck {
    pub fn ok(&self) -> bool {
        self.incoming_bytes.saturating_add(self.margin_bytes) <= self.available_bytes
    }

    pub fn summary(&self) -> String {
        format!(
            "{} brings about {} back into RAM; {} is available ({} kept free)",
            self.what,
            format_bytes(self.incoming_bytes),
            format_bytes(self.available_bytes),
            format_bytes(self.margin_bytes),
        )
    }

    /// Error text for a refused operation; `alternatives` lists what the caller can do.
    pub fn refusal(&self, alternatives: &str) -> String {
        format!(
            "{MEMORY_REFUSAL}: {}. Nothing was changed. Free some memory and retry, or \
             {alternatives}",
            self.summary()
        )
    }
}

/// Restarting zram brings every stored page (`mm_stat` orig_data_size) back into RAM.
pub fn zram_restart_check(mem: &MemoryInfo, devices: &[ZramDevice]) -> MemoryCheck {
    let active: Vec<&ZramDevice> = devices
        .iter()
        .filter(|d| d.mount_point == "[SWAP]" && d.data_bytes > 0)
        .collect();
    let names: Vec<&str> = active.iter().map(|d| d.name.as_str()).collect();
    MemoryCheck {
        what: if names.is_empty() {
            "restarting zram".into()
        } else {
            format!("restarting {}", names.join(", "))
        },
        incoming_bytes: active.iter().map(|d| d.data_bytes).sum(),
        margin_bytes: margin_bytes(mem.mem_total_kb * 1024),
        available_bytes: mem.mem_available_kb * 1024,
    }
}

const ZRAM_ALTERNATIVES: &str = "use --at-boot to write the configuration now and let it take \
     effect at the next boot, or --force to restart the device anyway";

/// `Ok(None)`: go ahead. `Ok(Some(warning))`: forced past a failed check. `Err`: refused.
pub fn decide(opts: ApplyOptions, check: &MemoryCheck) -> Result<Option<String>> {
    if opts.at_boot || check.ok() {
        return Ok(None);
    }
    if opts.force {
        return Ok(Some(format!(
            "Warning: forced past the memory check: {}",
            check.summary()
        )));
    }
    Err(XzramError::Validation(check.refusal(ZRAM_ALTERNATIVES)))
}

/// Live check before a zram restart; returns warnings to report. Call before any change.
pub fn precheck_zram_restart(opts: ApplyOptions) -> Result<Vec<String>> {
    if opts.at_boot || std::env::var_os("XZRAM_ETC_ROOT").is_some() {
        // Hermetic runs (XZRAM_ETC_ROOT) never restart devices.
        return Ok(Vec::new());
    }
    let status = crate::status::status()?;
    let check = zram_restart_check(&status.memory, &status.zram_devices);
    Ok(decide(opts, &check)?.into_iter().collect())
}

const AT_BOOT_NOTE: &str = "the running device is unchanged until the next boot";

/// Write `config`, then restart the device (or leave that to the next boot).
pub fn configure_zram(
    backend: &dyn ZramBackendTrait,
    config: &crate::apply::ZramConfig,
    opts: ApplyOptions,
) -> Result<Vec<String>> {
    backend.configure(config)?;
    if opts.at_boot {
        return Ok(vec![format!(
            "Wrote zram config for {}; {AT_BOOT_NOTE}",
            config.device
        )]);
    }
    backend.apply()?;
    Ok(vec![format!("Applied zram config for {}", config.device)])
}

/// Disable zram now, or only write the override that keeps it off from the next boot.
pub fn disable_zram(backend: &dyn ZramBackendTrait, opts: ApplyOptions) -> Result<Vec<String>> {
    if opts.at_boot {
        backend.disable_at_boot()?;
        return Ok(vec![format!("Disabled zram configuration; {AT_BOOT_NOTE}")]);
    }
    backend.disable()?;
    Ok(vec!["Disabled zram configuration".into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn mem(total_gib: u64, available: u64) -> MemoryInfo {
        MemoryInfo {
            mem_total_kb: total_gib * 1024 * 1024,
            mem_available_kb: available / 1024,
            swap_total_kb: 0,
            swap_free_kb: 0,
        }
    }

    fn zram0(data: u64) -> ZramDevice {
        ZramDevice {
            name: "zram0".into(),
            algorithm: "zstd".into(),
            disk_size_bytes: 4 * GIB,
            data_bytes: data,
            compressed_bytes: data / 3,
            total_bytes: data / 3,
            mem_used_max_bytes: data / 3,
            mount_point: "[SWAP]".into(),
        }
    }

    #[test]
    fn margin_is_five_percent_with_floor() {
        assert_eq!(margin_bytes(4 * GIB), MARGIN_MIN_BYTES);
        assert_eq!(margin_bytes(32 * GIB), 32 * GIB / 20);
    }

    #[test]
    fn issue_28_numbers_pass_and_short_memory_refuses() {
        // 3.79 GB in zram, 7.2 GiB available on 15.5 GiB: fine.
        let ok = zram_restart_check(&mem(16, 7 * GIB), &[zram0(3_790_000_000)]);
        assert!(ok.ok(), "{}", ok.summary());
        assert_eq!(decide(ApplyOptions::default(), &ok).unwrap(), None);

        // Same data with 3 GiB available: refused, nothing changed.
        let short = zram_restart_check(&mem(16, 3 * GIB), &[zram0(3_790_000_000)]);
        assert!(!short.ok());
        let err = decide(ApplyOptions::default(), &short)
            .unwrap_err()
            .to_string();
        assert!(err.contains(MEMORY_REFUSAL), "{err}");
        assert!(
            err.contains("--at-boot") && err.contains("--force"),
            "{err}"
        );
        assert!(err.contains("restarting zram0"), "{err}");
    }

    #[test]
    fn at_boot_and_force_bypass_the_refusal() {
        let short = zram_restart_check(&mem(16, GIB), &[zram0(3 * GIB)]);
        let at_boot = ApplyOptions {
            at_boot: true,
            force: false,
        };
        assert_eq!(decide(at_boot, &short).unwrap(), None);
        let forced = ApplyOptions {
            at_boot: false,
            force: true,
        };
        let warning = decide(forced, &short).unwrap().unwrap();
        assert!(warning.contains("forced"), "{warning}");
    }

    #[test]
    fn idle_or_inactive_devices_need_nothing() {
        let mut ramdisk = zram0(2 * GIB);
        ramdisk.mount_point = "/tmp".into();
        let check = zram_restart_check(&mem(8, GIB), &[zram0(0), ramdisk]);
        assert_eq!(check.incoming_bytes, 0);
        assert!(check.ok());
    }

    #[test]
    fn options_parse_with_defaults() {
        let o: ApplyOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(o, ApplyOptions::default());
        let o: ApplyOptions = serde_json::from_str(r#"{"at_boot":true}"#).unwrap();
        assert!(o.at_boot && !o.force);
    }
}
