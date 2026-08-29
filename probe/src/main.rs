use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use gauges_probe::{
    client::HubClient,
    collector::{MetricsCollector, probe_identity},
    config::ProbeConfig,
    database::SampleDatabase,
};
use tokio::signal::unix::{SignalKind, signal};
use tokio::{sync::Notify, time::MissedTickBehavior};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "gauges_probe=error".into()),
        )
        .compact()
        .init();

    let config = ProbeConfig::load()?;
    let database = SampleDatabase::open(&config.database_path)?;
    let upload_database = database.clone();
    let client = HubClient::new(&config)?;
    let mut collector = MetricsCollector::new(&config);
    let identity = collector.identity();
    let gpu_types = identity.gpu_types.clone();
    info!(
        device_id = %config.device_id,
        hostname = %identity.hostname,
        hub = %config.hub_url,
        retention_hours = config.retention_hours,
        collection_interval_seconds = config.collection_interval_seconds,
        "starting Gauges probe"
    );

    let collection_duration = Duration::from_secs(config.collection_interval_seconds);
    let retention_ms = i64::try_from(config.retention_hours)
        .unwrap_or(i64::MAX)
        .saturating_mul(60 * 60 * 1_000);
    let mut interval = tokio::time::interval(collection_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let upload_notify = Arc::new(Notify::new());
    let mut uploader = tokio::spawn(upload_loop(
        config.clone(),
        upload_database,
        client,
        upload_notify.clone(),
        gpu_types,
    ));
    let mut terminate = signal(SignalKind::terminate())?;

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
    Ok(())
}

async fn upload_loop(
    config: ProbeConfig,
    database: SampleDatabase,
    client: HubClient,
    notify: Arc<Notify>,
    gpu_types: Vec<String>,
) -> Result<()> {
    let retry_duration = Duration::from_secs(config.retry_interval_seconds);
    let mut retrying = !upload_pending(&config, &database, &client, &gpu_types).await?;

    loop {
        if retrying {
            tokio::time::sleep(retry_duration).await;
        } else {
            notify.notified().await;
        }
        retrying = !upload_pending(&config, &database, &client, &gpu_types).await?;
    }
}

async fn upload_pending(
    config: &ProbeConfig,
    database: &SampleDatabase,
    client: &HubClient,
    gpu_types: &[String],
) -> Result<bool> {
    loop {
        let batch = database.oldest_batch(config.upload_batch_size)?;
        if batch.is_empty() {
            return Ok(true);
        }

        // Network identity is refreshed for every reconnect/upload so DHCP or
        // hostname changes reach the hub. GPU types are stable startup facts.
        let current_identity = probe_identity(gpu_types.to_vec(), &config.etc_root);
        match client
            .ingest(
                &current_identity,
                config.retention_hours,
                config.collection_interval_seconds,
                batch,
            )
            .await
        {
            Ok(response) => {
                let acknowledged = response.accepted_sample_ids.len();
                let deleted = database.delete_accepted(&response.accepted_sample_ids)?;
                let pending = database.pending_count()?;
                info!(
                    acknowledged,
                    deleted,
                    pending,
                    server_time_ms = response.server_time_ms,
                    "uploaded metrics"
                );
                if acknowledged == 0 {
                    warn!(
                        pending,
                        retry_seconds = config.retry_interval_seconds,
                        "hub acknowledged no samples; pausing backlog flush"
                    );
                    return Ok(false);
                }
            }
            Err(error) => {
                let pending = database.pending_count()?;
                warn!(
                    %error,
                    pending,
                    retry_seconds = config.retry_interval_seconds,
                    "hub unavailable; samples remain buffered"
                );
                return Ok(false);
            }
        }
    }
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
