use xzram::apply::PendingConfig;
use xzram::recommend;

use crate::args::DefaultsCommands;
use crate::print::{confirm_apply_defaults, print_recommended_defaults};
use crate::privileged::run_privileged;

pub(crate) fn run(command: DefaultsCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        DefaultsCommands::Recommend => {
            let report = recommend::recommend()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_recommended_defaults(&report);
            }
        }
        DefaultsCommands::Stage => {
            let report = recommend::recommend()?;
            if !pending_has_changes(&report.pending) {
                println!("System already matches recommended defaults; nothing to stage");
                return Ok(());
            }
            run_privileged(dbus, "stage", &serde_json::to_string(&report.pending)?)?;
            println!("Recommended defaults staged; run 'xzram apply' or review tabs in the GUI");
        }
        DefaultsCommands::Apply { yes } => {
            let report = recommend::recommend()?;
            if !pending_has_changes(&report.pending) {
                println!("System already matches recommended defaults; nothing to apply");
                return Ok(());
            }
            if !yes {
                print_recommended_defaults(&report);
                if !confirm_apply_defaults()? {
                    println!("Cancelled");
                    return Ok(());
                }
            }
            run_privileged(dbus, "stage", &serde_json::to_string(&report.pending)?)?;
            run_privileged(dbus, "apply", "{}")?;
            println!("Recommended defaults applied");
        }
    }
    Ok(())
}

fn pending_has_changes(pending: &PendingConfig) -> bool {
    !xzram::apply::pending_is_empty(pending)
}
