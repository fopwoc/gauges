use std::time::Duration;

use anyhow::{Context, Result, bail};
use gauges_shared::{
    BUILD_VERSION, DEVICE_ID_HEADER, INGEST_PATH, IngestRequest, IngestResponse, MetricSample,
    ProbeIdentity,
};
use reqwest::{Client, StatusCode};

use crate::config::ProbeConfig;

pub struct HubClient {
    client: Client,
    endpoint: String,
    device_id: String,
    token: String,
}

impl HubClient {
    pub fn new(config: &ProbeConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .user_agent(format!("gauges-probe/{BUILD_VERSION}"))
            .build()?;
        Ok(Self {
            client,
            endpoint: format!("{}{INGEST_PATH}", config.hub_url.trim_end_matches('/')),
            device_id: config.device_id.clone(),
            token: config.token.clone(),
        })
    }

    pub async fn ingest(
        &self,
        identity: &ProbeIdentity,
        retention_hours: u64,
        collection_interval_seconds: u64,
        samples: Vec<MetricSample>,
    ) -> Result<IngestResponse> {
        let response = self
            .client
            .post(&self.endpoint)
            .header(DEVICE_ID_HEADER, &self.device_id)
            .bearer_auth(&self.token)
            .json(&IngestRequest {
                identity: identity.clone(),
                retention_hours,
                collection_interval_seconds,
                samples,
            })
            .send()
            .await
            .context("hub request failed")?;

        if response.status() != StatusCode::OK {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("hub rejected upload with {status}: {body}");
        }
        response
            .json()
            .await
            .context("hub returned an invalid response")
    }
}
