//! `xzram config show|set`: the settings a person changes, one key at a time.
//!
//! The same contract as `crumbs config`: `show` prints every setting in effect (no root),
//! `set <key> <value>` checks the value, then changes it now. `--dry-run` prints the plan
//! instead (see `plan`), `--ndjson` streams progress, `--json` prints one result document.
//! A failure with `--json` or `--ndjson` still prints a document, and exits 1.

use serde_json::{json, Value};
use xzram::apply::{ApplyOptions, ZramConfig};
use xzram::backend::available_zram_backend;
use xzram::status;
use xzram::sysctl;

use crate::args::ConfigCommands;
use crate::exit_codes;
use crate::plan::{self, SCHEMA_VERSION};
use crate::privileged::run_privileged;
use crate::progress::{self, Watch};
use crate::settings::{self, Key};

pub(crate) struct Mode {
    pub json: bool,
    pub ndjson: bool,
    pub dbus: bool,
}

fn current_zram() -> anyhow::Result<Option<ZramConfig>> {
    Ok(available_zram_backend()?.show()?)
}

pub(crate) fn run(command: ConfigCommands, mode: Mode) -> anyhow::Result<u8> {
    match command {
        ConfigCommands::Show => show(&mode),
        ConfigCommands::Set {
            key,
            value,
            restart,
            dry_run,
        } => set(&key, &value, restart.options(), dry_run, &mode),
    }
}

fn show(mode: &Mode) -> anyhow::Result<u8> {
    let zram = current_zram()?;
    let tuning = sysctl::show()?;
    let pending = crate::store_read::load_pending_readable()
        .ok()
        .flatten()
        .is_some_and(|p| !xzram::apply::pending_is_empty(&p));
    let snapshots = crate::store_read::list_snapshots().map_or(0, |l| l.len());
    let doc = settings::show_document(zram.as_ref(), &tuning, pending, snapshots);
    if mode.json {
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(exit_codes::OK);
    }
    println!("Compressed memory");
    match &zram {
        None => println!("  not set up through XZram"),
        Some(c) => {
            println!(
                "  size:        {}",
                c.zram_size.as_deref().unwrap_or("default")
            );
            println!(
                "  compression: {}",
                c.compression_algorithm.as_deref().unwrap_or("default")
            );
            println!(
                "  priority:    {}",
                c.swap_priority.map_or("default".into(), |p| p.to_string())
            );
        }
    }
    println!("Kernel tuning");
    for key in [
        Key::Swappiness,
        Key::PageCluster,
        Key::VfsCachePressure,
        Key::WatermarkScaleFactor,
        Key::WatermarkBoostFactor,
    ] {
        let v = key
            .tuning_value(&tuning)
            .map_or("not set".into(), |v| v.to_string());
        println!("  {:<24} {v}", key.name());
    }
    Ok(exit_codes::OK)
}

/// A failure printed as a document when the caller asked for one, else left to `main`.
fn fail(err: anyhow::Error, mode: &Mode) -> anyhow::Result<u8> {
    if !mode.json && !mode.ndjson {
        return Err(err);
    }
    let message = format!("{err:#}");
    if mode.json {
        let doc = json!({ "schema_version": SCHEMA_VERSION, "ok": false,
            "kind": progress::error_kind(&message), "error": message });
        println!("{}", serde_json::to_string_pretty(&doc)?);
    }
    eprintln!("xzram: {message}");
    Ok(exit_codes::ERROR)
}

fn set(
    key: &str,
    value: &str,
    opts: ApplyOptions,
    dry_run: bool,
    mode: &Mode,
) -> anyhow::Result<u8> {
    let outcome = (|| {
        let key = Key::parse(key)?;
        if !key.is_zram() && (opts.at_boot || opts.force) {
            anyhow::bail!("--at-boot and --force only apply to compressed memory settings");
        }
        if key.is_zram() {
            set_zram(key, value, opts, dry_run, mode)
        } else {
            set_tuning(key, value, dry_run, mode)
        }
    })();
    match outcome {
        Ok(()) => Ok(exit_codes::OK),
        Err(e) => fail(e, mode),
    }
}

fn finish(plan_or_done: Value, mode: &Mode, human: &str) -> anyhow::Result<()> {
    if mode.json {
        println!("{}", serde_json::to_string_pretty(&plan_or_done)?);
    } else if !mode.ndjson {
        println!("{human}");
    }
    Ok(())
}

fn set_zram(
    key: Key,
    value: &str,
    opts: ApplyOptions,
    dry_run: bool,
    mode: &Mode,
) -> anyhow::Result<()> {
    let old = current_zram()?;
    let next = settings::merged_zram(old.as_ref(), key, value)?;
    if dry_run {
        let report = status::status()?;
        return plan::zram_change(key, old.as_ref(), &next, opts, &report).print(mode.json);
    }
    // Same payload `xzram zram set --now` sends: the whole configuration, plus options
    // only when they are not the defaults.
    let payload = if opts == ApplyOptions::default() {
        serde_json::to_string(&next)?
    } else {
        serde_json::to_string(&json!({ "config": next, "options": opts }))?
    };
    let watch = if opts.at_boot {
        Watch::Nothing
    } else {
        Watch::Zram
    };
    let applied = if opts.at_boot { "at_boot" } else { "now" };
    let extra = json!({ "key": key.name(), "value": value.trim(), "applied": applied });
    progress::run(
        mode.ndjson,
        "Changing compressed memory",
        watch,
        extra.clone(),
        || run_privileged(mode.dbus, "zram.configure", &payload),
    )?;
    let mut done = extra;
    done["schema_version"] = json!(SCHEMA_VERSION);
    done["ok"] = json!(true);
    finish(
        done,
        mode,
        &format!("{} set to {}", key.label(), value.trim()),
    )
}

fn set_tuning(key: Key, value: &str, dry_run: bool, mode: &Mode) -> anyhow::Result<()> {
    let values = settings::tuning_values(key, value)?;
    if dry_run {
        let now = sysctl::show()?;
        let new = key.tuning_value(&values).unwrap_or_default();
        return plan::tuning(key, key.tuning_value(&now), new).print(mode.json);
    }
    let extra = json!({ "key": key.name(), "value": value.trim(), "applied": "now" });
    progress::run(
        mode.ndjson,
        "Changing kernel tuning",
        Watch::Nothing,
        extra.clone(),
        || run_privileged(mode.dbus, "sysctl.set", &serde_json::to_string(&values)?),
    )?;
    let mut done = extra;
    done["schema_version"] = json!(SCHEMA_VERSION);
    done["ok"] = json!(true);
    finish(
        done,
        mode,
        &format!("{} set to {}", key.label(), value.trim()),
    )
}
