use tracing::info;

use super::Manager;

pub async fn serve() -> anyhow::Result<()> {
    // Register the interface before requesting the bus name: `Manager` needs an owned
    // `Connection` (for polkit authorize() calls), so the interface can't be handed to
    // `Builder::serve_at()` ahead of `build()`. Requesting the name via `Builder::name()`
    // instead would race method calls arriving before the object server is set up.
    let connection = zbus::connection::Builder::system()?.build().await?;

    connection
        .object_server()
        .at("/io/github/XZram", Manager::new(connection.clone()))
        .await?;

    connection.request_name("io.github.XZram1").await?;

    info!("xzramd listening on system bus as io.github.XZram1");
    wait_shutdown().await?;
    connection.release_name("io.github.XZram1").await?;
    Ok(())
}

async fn wait_shutdown() -> std::io::Result<()> {
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        r = tokio::signal::ctrl_c() => r,
        _ = sigterm.recv() => Ok(()),
    }
}
