use std::{
    process::ExitCode,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use gauges_probe::{
    client::HubClient,
    collector::MetricsCollector,
    config::ProbeConfig,
    database::SampleDatabase,
    uploader::{self, UploadIdentity},
};
use gauges_shared::BUILD_VERSION;
use tokio::signal::unix::{SignalKind, signal};
use tokio::{sync::Notify, time::MissedTickBehavior};
use tracing::{error, info, warn};
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
            error!(error = ?error, "Gauges probe stopped unexpectedly");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    info!(version = BUILD_VERSION, "starting Gauges probe");
    let config = ProbeConfig::load()?;
    info!(
        device_id = %config.device_id,
        hub = %config.hub_url,
        retention_hours = config.retention_hours,
        collection_interval_seconds = config.collection_interval_seconds,
        "configuration loaded"
    );
    let database = SampleDatabase::open(&config.database_path)?;
    info!(database = %config.database_path.display(), "redb outbox initialized");
    let upload_database = database.clone();
    let client = HubClient::new(&config)?;
    info!("hub client initialized");
    let mut collector = MetricsCollector::new(&config);
    let identity = collector.identity();
    let upload_identity = UploadIdentity::from(&identity);
    info!(
        hostname = %identity.hostname,
        cpu_model = identity.cpu_model.as_deref().unwrap_or("unknown"),
        gpu_count = identity.gpu_types.len(),
        "metrics collector initialized"
    );

    let collection_duration = Duration::from_secs(config.collection_interval_seconds);
    let retention_ms = i64::try_from(config.retention_hours)
        .unwrap_or(i64::MAX)
        .saturating_mul(60 * 60 * 1_000);
    let mut interval = tokio::time::interval(collection_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let upload_notify = Arc::new(Notify::new());
    let mut uploader = tokio::spawn(uploader::run(
        config.clone(),
        upload_database,
        client,
        upload_notify.clone(),
        upload_identity,
    ));
    let mut terminate = signal(SignalKind::terminate())?;
    info!("Gauges probe ready");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutting down");
                break;
            }
            _ = terminate.recv() => {
                info!("shutting down after SIGTERM");
                break;
            }
            _ = interval.tick() => {
                match collector.collect() {
                    Ok(sample) => {
                        database.insert(&sample)?;
                        let cutoff = unix_time_ms().saturating_sub(retention_ms);
                        let pruned = database.prune_before(cutoff)?;
                        if pruned > 0 {
                            warn!(pruned, "discarded local samples beyond retention window");
                        }
                        upload_notify.notify_one();
                    }
                    Err(error) => warn!(%error, "metric collection failed"),
                }
            }
            result = &mut uploader => {
                return match result {
                    Ok(result) => result,
                    Err(error) => Err(error.into()),
                };
            }
        }
    }

    uploader.abort();
    let _ = uploader.await;
    info!("Gauges probe stopped");
    Ok(())
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
