use xzram::apply::PendingConfig;
use xzram::recommend::{self, RecommendScales, RecommendSizeScale};

use crate::args::DefaultsCommands;
use crate::print::{confirm_apply_defaults, print_recommended_defaults};
use crate::privileged::run_privileged;

fn parse_scales(zram_scale: &str, swap_scale: &str) -> anyhow::Result<RecommendScales> {
    let zram = RecommendSizeScale::parse(zram_scale)
        .ok_or_else(|| anyhow::anyhow!("invalid --zram-scale (use low|default|high)"))?;
    let swapfile = RecommendSizeScale::parse(swap_scale)
        .ok_or_else(|| anyhow::anyhow!("invalid --swap-scale (use low|default|high)"))?;
    Ok(RecommendScales { zram, swapfile })
}

pub(crate) fn run(command: DefaultsCommands, json: bool, dbus: bool) -> anyhow::Result<()> {
    match command {
        DefaultsCommands::Recommend {
            zram_scale,
            swap_scale,
        } => {
            let scales = parse_scales(&zram_scale, &swap_scale)?;
            let report = recommend::recommend_with_scales(scales)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_recommended_defaults(&report);
            }
        }
        DefaultsCommands::Stage {
            zram_scale,
            swap_scale,
        } => {
            let scales = parse_scales(&zram_scale, &swap_scale)?;
            let report = recommend::recommend_with_scales(scales)?;
            if !pending_has_changes(&report.pending) {
                println!("System already matches recommended defaults; nothing to stage");
                return Ok(());
            }
            run_privileged(dbus, "stage", &serde_json::to_string(&report.pending)?)?;
            println!("Recommended defaults staged; run 'xzram apply' or review tabs in the GUI");
        }
        DefaultsCommands::Apply {
            yes,
            zram_scale,
            swap_scale,
        } => {
            let scales = parse_scales(&zram_scale, &swap_scale)?;
            let report = recommend::recommend_with_scales(scales)?;
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
        DefaultsCommands::OptimizeLinked { anchor, seed_file } => {
            let anchor = recommend::LinkedAnchor::parse(&anchor).ok_or_else(|| {
                anyhow::anyhow!(
                    "invalid --anchor (use zram_size|zram_algo|zram_priority|sysctl|swapfile_create|swapfile_resize|swapfile_remove)"
                )
            })?;
            let seed_raw = match seed_file {
                Some(path) => std::fs::read_to_string(path)?,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };
            let seed: PendingConfig = serde_json::from_str(&seed_raw)
                .map_err(|e| anyhow::anyhow!("invalid seed JSON: {e}"))?;
            let ctx = recommend::linked_optimize_context()?;
            let result = recommend::optimize_linked(anchor, seed, &ctx);
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for line in &result.adjustments {
                    println!("Adjusted: {line}");
                }
                if result.adjustments.is_empty() {
                    println!("No linked adjustments");
                }
                println!("{}", serde_json::to_string_pretty(&result.pending)?);
            }
        }
    }
    Ok(())
}

fn pending_has_changes(pending: &PendingConfig) -> bool {
    !xzram::apply::pending_is_empty(pending)
}
