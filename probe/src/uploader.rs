use std::{sync::Arc, time::Duration};

use anyhow::Result;
use gauges_shared::ProbeIdentity;
use tokio::sync::Notify;
use tracing::{info, warn};

use crate::{
    client::HubClient, collector::probe_identity, config::ProbeConfig, database::SampleDatabase,
};

pub struct UploadIdentity {
    cpu_model: Option<String>,
    gpu_types: Vec<String>,
}

impl From<&ProbeIdentity> for UploadIdentity {
    fn from(identity: &ProbeIdentity) -> Self {
        Self {
            cpu_model: identity.cpu_model.clone(),
            gpu_types: identity.gpu_types.clone(),
        }
    }
}

pub async fn run(
    config: ProbeConfig,
    database: SampleDatabase,
    client: HubClient,
    notify: Arc<Notify>,
    identity: UploadIdentity,
) -> Result<()> {
    let retry_duration = Duration::from_secs(config.retry_interval_seconds);
    let mut retrying = !upload_pending(&config, &database, &client, &identity).await?;

    loop {
        if retrying {
            tokio::time::sleep(retry_duration).await;
        } else {
            notify.notified().await;
        }
        retrying = !upload_pending(&config, &database, &client, &identity).await?;
    }
}

async fn upload_pending(
    config: &ProbeConfig,
    database: &SampleDatabase,
    client: &HubClient,
    identity: &UploadIdentity,
) -> Result<bool> {
    loop {
        let batch = database.oldest_batch(config.upload_batch_size)?;
        if batch.is_empty() {
            return Ok(true);
        }

        // Network identity is refreshed for every reconnect/upload so DHCP or
        // hostname changes reach the hub. Hardware names are stable startup facts.
        let current_identity = probe_identity(
            identity.cpu_model.clone(),
            identity.gpu_types.clone(),
            &config.etc_root,
        );
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
