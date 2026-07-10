use std::process;

use clap::Parser;
use serde::Deserialize;
use xzram::apply::{self, ApplyRequest, SwapfileConfig, ZramConfig};
use xzram::backend::{available_swapfile_backend, available_zram_backend};
use xzram::sysctl::{self, SysctlValues};

/// Privileged helper for xzram — invoked via pkexec with polkit authorization.
#[derive(Parser)]
#[command(name = "xzram-helper", about = "XZram privileged helper")]
struct Args {
    /// Action to perform
    action: String,
    /// JSON payload
    payload: String,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args.action, &args.payload) {
        eprintln!("xzram-helper: {e}");
        process::exit(1);
    }
}

fn run(action: &str, payload: &str) -> xzram::Result<()> {
    match action {
        "zram.configure" => {
            let config: ZramConfig = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_zram_backend();
            backend.configure(&config)?;
            backend.apply()?;
            println!("ZRAM configured");
        }
        "zram.disable" => {
            let backend = available_zram_backend();
            backend.disable()?;
            println!("ZRAM disabled");
        }
        "swapfile.create" => {
            let config: SwapfileConfig = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_swapfile_backend();
            backend.create(&config)?;
            println!("Swapfile created: {}", config.path);
        }
        "swapfile.resize" => {
            #[derive(Deserialize)]
            struct ResizePayload {
                path: String,
                size_mb: u64,
            }
            let p: ResizePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_swapfile_backend();
            backend.resize(&p.path, p.size_mb)?;
            println!("Swapfile resized: {}", p.path);
        }
        "swapfile.remove" => {
            #[derive(Deserialize)]
            struct RemovePayload {
                path: String,
            }
            let p: RemovePayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            let backend = available_swapfile_backend();
            backend.remove(&p.path)?;
            println!("Swapfile removed: {}", p.path);
        }
        "swap.activate" => {
            #[derive(Deserialize)]
            struct SwapPayload {
                action: String,
                device: String,
            }
            let p: SwapPayload = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            match p.action.as_str() {
                "on" => {
                    apply::run_command("swapon", &[&p.device])?;
                }
                "off" => {
                    apply::run_command("swapoff", &[&p.device])?;
                }
                _ => {
                    return Err(xzram::XzramError::Validation("unknown swap action".into()));
                }
            }
            println!("Swap {}: {}", p.action, p.device);
        }
        "sysctl.set" => {
            let values: SysctlValues = serde_json::from_str(payload)
                .map_err(|e| xzram::XzramError::Parse(e.to_string()))?;
            sysctl::set(&values)?;
            println!("Sysctl values applied");
        }
        "apply" => {
            let request: ApplyRequest = if payload == "{}" {
                ApplyRequest {
                    zram: None,
                    swapfile: None,
                    disable_zram: false,
                    remove_swapfile: None,
                }
            } else {
                serde_json::from_str(payload)
                    .map_err(|e| xzram::XzramError::Parse(e.to_string()))?
            };
            let result = apply::apply(&request)?;
            for msg in &result.messages {
                println!("{msg}");
            }
        }
        "rollback" => {
            let result = apply::rollback()?;
            for msg in &result.messages {
                println!("{msg}");
            }
        }
        _ => {
            return Err(xzram::XzramError::Validation(format!(
                "unknown action: {action}"
            )));
        }
    }
    Ok(())
}
