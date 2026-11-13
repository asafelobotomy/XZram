use tracing::debug;
use xzram::detect;
use xzram::doctor;
use xzram::status;

use crate::print::{print_detect, print_doctor, print_pressure, print_status};

pub(crate) fn status(json: bool) -> anyhow::Result<()> {
    debug!("collecting status report");
    let report = status::status()?;
    let pressure = status::pressure();
    if json {
        // `pressure` is added next to the library report's fields (xzramd's GetStatus
        // does not carry it).
        let mut value = serde_json::to_value(&report)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("pressure".into(), serde_json::to_value(pressure)?);
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print_status(&report);
        print_pressure(&pressure);
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

/// Returns the exit code: `OK` when healthy, `UNHEALTHY` (3) when checks report problems.
pub(crate) fn doctor(json: bool) -> anyhow::Result<u8> {
    debug!("running doctor checks");
    let report = doctor::doctor()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_doctor(&report);
    }
    Ok(if report.healthy {
        crate::exit_codes::OK
    } else {
        crate::exit_codes::UNHEALTHY
    })
}

pub(crate) fn pending_show(json: bool) -> anyhow::Result<()> {
    let pending = crate::store_read::load_pending_readable()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&pending)?);
    } else if let Some(p) = pending {
        println!("{}", serde_json::to_string_pretty(&p)?);
    } else {
        println!("No pending configuration");
    }
    Ok(())
}
