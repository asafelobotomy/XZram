use tracing::info;

use super::Manager;

pub async fn serve() -> anyhow::Result<()> {
    let connection = zbus::connection::Builder::system()?
        .name("io.github.XZram1")?
        .build()
        .await?;

    connection
        .object_server()
        .at("/io/github/XZram", Manager::new(connection.clone()))
        .await?;

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
