use std::{process::ExitCode, sync::Arc};

use anyhow::{Context, Result};
use gauges_hub::{HubConfig, HubDatabase, router, system_clock};
use gauges_shared::BUILD_VERSION;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new("error")),
        )
        .compact()
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(error = ?error, "Gauges hub stopped unexpectedly");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    info!(version = BUILD_VERSION, "starting Gauges hub");

    let config = HubConfig::from_environment().context("failed to load hub configuration")?;
    info!(
        devices = config.devices.len(),
        database = %config.database_path.display(),
        console = %config.console_path.display(),
        "configuration loaded"
    );
    let database =
        HubDatabase::open(&config.database_path).context("failed to open hub database")?;
    info!("redb storage initialized");
    let deleted = database.delete_expired_samples(system_clock())?;
    if deleted > 0 {
        info!(deleted, "expired samples removed during startup");
    }

    let cleanup_database = database.clone();
    let cleanup_interval = config.cleanup_interval;
    let cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(cleanup_interval);
        interval.tick().await;
        loop {
            interval.tick().await;
            let database = cleanup_database.clone();
            match tokio::task::spawn_blocking(move || {
                database.delete_expired_samples(system_clock())
            })
            .await
            {
                Ok(Ok(deleted)) if deleted > 0 => info!(deleted, "expired samples removed"),
                Ok(Ok(_)) => {}
                Ok(Err(error)) => error!(error = ?error, "sample retention cleanup failed"),
                Err(error) => error!(error = ?error, "sample retention cleanup task failed"),
            }
        }
    });

    let address = format!("{}:{}", config.host, config.port);
    let app = router(config, database, Arc::new(system_clock));
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("failed to bind {address}"))?;
    info!(%address, "Gauges hub ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    cleanup.abort();
    let _ = cleanup.await;
    info!("Gauges hub stopped");
    Ok(())
}

async fn shutdown_signal() {
    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(signal) => signal,
        Err(error) => {
            error!(%error, "failed to install SIGTERM handler");
            if let Err(error) = tokio::signal::ctrl_c().await {
                error!(%error, "failed to wait for Ctrl-C");
            }
            return;
        }
    };
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            if let Err(error) = result {
                error!(%error, "failed to wait for Ctrl-C");
            }
        }
        _ = terminate.recv() => {}
    }
}
