use xzram::status::format_bytes;
use xzram::swap_partition;

use crate::args::SwapCommands;
use crate::privileged::run_privileged;

pub(crate) fn run(command: SwapCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        SwapCommands::List => {
            let entries = swap_partition::list_swaps_merged()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                for s in &entries {
                    let state = if s.active { "active" } else { "inactive" };
                    println!(
                        "{} [{}] {}  {} used / {}  priority {}  ({})",
                        s.name,
                        s.swap_type,
                        state,
                        format_bytes(s.used_bytes),
                        format_bytes(s.size_bytes),
                        s.priority,
                        format!("{:?}", s.source).to_lowercase()
                    );
                }
            }
        }
        SwapCommands::On { device } => {
            xzram::validation::validate_swap_device(&device)?;
            let payload = serde_json::json!({ "action": "on", "device": device });
            run_privileged(dbus, "swap.activate", &payload.to_string())?;
        }
        SwapCommands::Off { device } => {
            xzram::validation::validate_swap_device(&device)?;
            let payload = serde_json::json!({ "action": "off", "device": device });
            run_privileged(dbus, "swap.activate", &payload.to_string())?;
        }
        SwapCommands::DrainOverflow { force, dry_run } => drain(force, dry_run, json, dbus)?,
    }
    Ok(())
}

/// Show the drain plan and its memory check, then (unless `dry_run`) run it privileged.
/// The helper re-checks with fresh numbers before touching anything.
fn drain(force: bool, dry_run: bool, json: bool, dbus: bool) -> anyhow::Result<()> {
    let status = xzram::status::status()?;
    let plan = xzram::apply::plan_drain(&status).map_err(anyhow::Error::msg)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else if plan.targets.is_empty() {
        println!("No lower-priority disk swap holds data; nothing to drain");
    } else {
        for t in &plan.targets {
            println!(
                "{}: {} used, priority {}",
                t.name,
                format_bytes(t.used_bytes),
                t.priority
            );
        }
        println!("zram free capacity: {}", format_bytes(plan.zram_free_bytes));
        let verdict = if plan.check.ok() {
            "ok"
        } else {
            "too little memory"
        };
        println!("Memory check: {verdict}: {}", plan.check.summary());
    }
    if dry_run || plan.targets.is_empty() {
        return Ok(());
    }
    if let Some(warning) = xzram::apply::drain::decide_drain(&plan, force)? {
        eprintln!("{warning}");
    }
    let payload = serde_json::json!({ "force": force }).to_string();
    run_privileged(dbus, "swap.drain-overflow", &payload)
}
