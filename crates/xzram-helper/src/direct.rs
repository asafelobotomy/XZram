//! Immediate ("apply now") helper actions that change live swap state.
//!
//! Each takes an automatic `pre_apply` snapshot after validating its payload and before
//! touching the system, exactly like `apply` does, so `rollback` (which restores the newest
//! `pre_apply` snapshot) undoes the most recent change rather than jumping back past it.

use std::path::Path;

use serde::Deserialize;
use xzram::apply::{self, guard, ApplyOptions, SwapfileConfig, ZramConfig};
use xzram::backend::{available_swapfile_backend, ensure_zram_backend, SwapfileBackendTrait};
use xzram::snapshot::{self, SnapshotTrigger};
use xzram::sysctl::{self, SysctlValues};
use xzram::validation;
use xzram::{Result, XzramError};

/// Every payload is parsed strictly: a field this helper doesn't know means the client is
/// newer, and silently dropping it could change what the client asked for.
fn parse<T: serde::de::DeserializeOwned + serde::Serialize>(payload: &str) -> Result<T> {
    validation::from_json_strict(payload)
}

/// Snapshot current state before a direct action mutates it.
fn pre_change_snapshot(action: &str) -> Result<()> {
    let label = format!("Before {action}");
    snapshot::create_snapshot(SnapshotTrigger::PreApply, Some(&label), None)?;
    Ok(())
}

fn swapfile_backend() -> Result<Box<dyn SwapfileBackendTrait>> {
    let backend = available_swapfile_backend();
    if !backend.is_available() {
        return Err(XzramError::Backend(
            "swapfile backend is not available".into(),
        ));
    }
    Ok(backend)
}

fn is_empty(payload: &str) -> bool {
    let t = payload.trim();
    t.is_empty() || t == "{}"
}

fn print_lines<'a>(lines: impl IntoIterator<Item = &'a String>) {
    for line in lines {
        println!("{line}");
    }
}

/// `{}` or `{"options": {"at_boot": bool, "force": bool}}`.
pub fn parse_options(payload: &str) -> Result<ApplyOptions> {
    if is_empty(payload) {
        return Ok(ApplyOptions::default());
    }
    #[derive(Deserialize, serde::Serialize)]
    struct Wrapper {
        options: ApplyOptions,
    }
    Ok(validation::from_json_strict::<Wrapper>(payload)?.options)
}

/// A bare `ZramConfig`, or `{"config": ZramConfig, "options": ApplyOptions}`.
fn parse_zram_configure(payload: &str) -> Result<(ZramConfig, ApplyOptions)> {
    let value: serde_json::Value = parse(payload)?;
    if value.get("config").is_some() {
        #[derive(Deserialize, serde::Serialize)]
        struct Wrapper {
            config: ZramConfig,
            #[serde(default)]
            options: ApplyOptions,
        }
        let w: Wrapper = validation::from_value_strict(&value)?;
        return Ok((w.config, w.options));
    }
    Ok((
        validation::from_value_strict(&value)?,
        ApplyOptions::default(),
    ))
}

/// Run a direct action. Returns `Ok(false)` when `action` is not a direct action.
pub fn run(action: &str, payload: &str) -> Result<bool> {
    match action {
        "zram.configure" => {
            let (config, opts) = parse_zram_configure(payload)?;
            validation::validate_zram_config(&config)?;
            let backend = ensure_zram_backend()?;
            // Memory check before the snapshot: a refusal changes nothing.
            let warnings = guard::precheck_zram_restart(opts)?;
            pre_change_snapshot(action)?;
            let messages = guard::configure_zram(backend.as_ref(), &config, opts)?;
            print_lines(warnings.iter().chain(&messages));
            println!("ZRAM configured");
        }
        "zram.disable" => {
            let opts = parse_options(payload)?;
            let backend = ensure_zram_backend()?;
            let warnings = guard::precheck_zram_restart(opts)?;
            pre_change_snapshot(action)?;
            let messages = guard::disable_zram(backend.as_ref(), opts)?;
            print_lines(warnings.iter().chain(&messages));
            println!("ZRAM disabled");
        }
        "swap.drain-overflow" => {
            #[derive(Deserialize, serde::Serialize)]
            struct DrainPayload {
                #[serde(default)]
                force: bool,
            }
            let p: DrainPayload = if is_empty(payload) {
                DrainPayload { force: false }
            } else {
                validation::from_json_strict(payload)?
            };
            // Only swapoff/swapon; no configuration changes, so no snapshot.
            print_lines(&apply::drain_overflow(p.force)?);
        }
        "swapfile.create" => {
            /// `SwapfileConfig` fields plus the optional prepare flags, all at the top level.
            #[derive(Deserialize, serde::Serialize)]
            struct CreatePayload {
                #[serde(flatten)]
                config: SwapfileConfig,
                #[serde(default)]
                prepare: bool,
                #[serde(default)]
                mkdir_parents: bool,
            }
            let p: CreatePayload = parse(payload)?;
            let config = validation::validate_swapfile_config(&p.config)?;
            if p.prepare {
                validation::validate_swapfile_prepare_path(&config.path)?;
            }
            let backend = swapfile_backend()?;
            pre_change_snapshot(action)?;
            if p.prepare {
                xzram::swapfile_btrfs::prepare_nodatacow(Path::new(&config.path), p.mkdir_parents)?;
            }
            backend.create(&config)?;
            println!("Swapfile created: {}", config.path);
        }
        "swapfile.resize" => {
            #[derive(Deserialize, serde::Serialize)]
            struct ResizePayload {
                path: String,
                size_mb: u64,
            }
            let p: ResizePayload = parse(payload)?;
            validation::validate_swapfile_resize_path(&p.path, p.size_mb)?;
            let backend = swapfile_backend()?;
            pre_change_snapshot(action)?;
            backend.resize(&p.path, p.size_mb)?;
            println!("Swapfile resized: {}", p.path);
        }
        "swapfile.prepare" => {
            // `xzram swapfile prepare`, xzramd PrepareSwapfileBtrfs and the doctor/GUI fix
            // for a copy-on-write swap directory. Creates the directory if asked and sets
            // nodatacow (+C) on it; no swap state changes, so no snapshot.
            #[derive(Deserialize, serde::Serialize)]
            struct PreparePayload {
                path: String,
                #[serde(default)]
                mkdir_parents: bool,
            }
            let p: PreparePayload = parse(payload)?;
            validation::validate_swapfile_prepare_path(&p.path)?;
            let status =
                xzram::swapfile_btrfs::prepare_nodatacow(Path::new(&p.path), p.mkdir_parents)?;
            println!(
                "Prepared {} for a swapfile ({})",
                status.parent_directory, status.message
            );
        }
        "swapfile.remove" => {
            #[derive(Deserialize, serde::Serialize)]
            struct RemovePayload {
                path: String,
            }
            let p: RemovePayload = parse(payload)?;
            validation::validate_swapfile_remove_path(&p.path)?;
            let backend = swapfile_backend()?;
            pre_change_snapshot(action)?;
            backend.remove(&p.path)?;
            println!("Swapfile removed: {}", p.path);
        }
        "swap.activate" => {
            #[derive(Deserialize, serde::Serialize)]
            struct SwapPayload {
                action: String,
                device: String,
            }
            let p: SwapPayload = parse(payload)?;
            validation::validate_swap_device(&p.device)?;
            let cmd = match p.action.as_str() {
                "on" => "swapon",
                "off" => "swapoff",
                _ => return Err(XzramError::Validation("unknown swap action".into())),
            };
            pre_change_snapshot(action)?;
            apply::run_command(cmd, &["--", &p.device])?;
            println!("Swap {}: {}", p.action, p.device);
        }
        "sysctl.set" => {
            let values: SysctlValues = validation::from_json_strict(payload)?;
            sysctl::validate_sysctl_values(&values)?;
            pre_change_snapshot(action)?;
            sysctl::set(&values)?;
            println!("Sysctl values applied");
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zram_configure_accepts_bare_and_wrapped_payloads() {
        let bare = r#"{"device":"zram0","zram_size":"ram","zram_resident_limit":null,
            "compression_algorithm":"zstd","swap_priority":100,"fs_type":null,"mount_point":null}"#;
        let (config, opts) = parse_zram_configure(bare).unwrap();
        assert_eq!(config.zram_size.as_deref(), Some("ram"));
        assert_eq!(opts, ApplyOptions::default());

        let wrapped = format!(r#"{{"config":{bare},"options":{{"at_boot":true}}}}"#);
        let (_, opts) = parse_zram_configure(&wrapped).unwrap();
        assert!(opts.at_boot && !opts.force);

        let unknown = format!(r#"{{"config":{bare},"options":{{"later":true}}}}"#);
        assert!(parse_zram_configure(&unknown).is_err());
    }

    #[test]
    fn options_payloads() {
        assert_eq!(parse_options("{}").unwrap(), ApplyOptions::default());
        assert!(
            parse_options(r#"{"options":{"force":true}}"#)
                .unwrap()
                .force
        );
        assert!(parse_options(r#"{"force":true}"#).is_err());
    }
}
