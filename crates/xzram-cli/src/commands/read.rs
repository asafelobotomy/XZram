use tracing::debug;
use xzram::apply::load_pending;
use xzram::detect;
use xzram::doctor;
use xzram::status;

use crate::print::{print_detect, print_doctor, print_status};

pub(crate) fn status(json: bool) -> anyhow::Result<()> {
    debug!("collecting status report");
    let report = status::status()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_status(&report);
    }
    Ok(())
}

pub(crate) fn detect(json: bool) -> anyhow::Result<()> {
    debug!("running detection");
    let report = detect::detect()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_detect(&report);
    }
    Ok(())
}

pub(crate) fn doctor(json: bool) -> anyhow::Result<()> {
    debug!("running doctor checks");
    let report = doctor::doctor()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_doctor(&report);
    }
    if !report.healthy {
        std::process::exit(1);
    }
    Ok(())
}

pub(crate) fn pending_show(json: bool) -> anyhow::Result<()> {
    let pending = load_pending()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&pending)?);
    } else if let Some(p) = pending {
        println!("{}", serde_json::to_string_pretty(&p)?);
    } else {
        println!("No pending configuration");
    }
    Ok(())
}
