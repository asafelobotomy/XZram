mod dbus;
mod privileged;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // uQ's log format and a crash record on a panic (uQ's native/uq-diag).
    xzram::uq_diag::init("xzramd", "warn");

    dbus::serve().await
}
