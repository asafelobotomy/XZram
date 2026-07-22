use xzram::apply::PendingConfig;
use xzram::sysctl::{self, SysctlValues};

use crate::args::SysctlCommands;
use crate::print::print_sysctl;
use crate::privileged::run_privileged;

pub(crate) fn run(command: SysctlCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        SysctlCommands::Show => {
            let values = sysctl::show()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&values)?);
            } else {
                print_sysctl(&values);
            }
        }
        SysctlCommands::Set {
            swappiness,
            watermark_boost_factor,
            watermark_scale_factor,
            page_cluster,
            zram_defaults,
            now,
        } => {
            let values = if zram_defaults {
                sysctl::zram_tuning_defaults()
            } else {
                SysctlValues {
                    swappiness,
                    watermark_boost_factor,
                    watermark_scale_factor,
                    page_cluster,
                }
            };
            let pending = PendingConfig {
                sysctl: Some(values.clone()),
                ..Default::default()
            };
            if now {
                run_privileged(dbus, "sysctl.set", &serde_json::to_string(&values)?)?;
            } else {
                run_privileged(dbus, "stage", &serde_json::to_string(&pending)?)?;
                println!("Staged sysctl changes; run 'xzram apply' to apply");
            }
        }
    }
    Ok(())
}
