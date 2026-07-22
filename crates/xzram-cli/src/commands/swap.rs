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
            let payload = serde_json::json!({ "action": "on", "device": device });
            run_privileged(dbus, "swap.activate", &payload.to_string())?;
        }
        SwapCommands::Off { device } => {
            let payload = serde_json::json!({ "action": "off", "device": device });
            run_privileged(dbus, "swap.activate", &payload.to_string())?;
        }
    }
    Ok(())
}
