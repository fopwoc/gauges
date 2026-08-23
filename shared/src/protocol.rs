use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEVICE_ID_HEADER: &str = "X-Gauges-Device-Id";
pub const INGEST_PATH: &str = "/api/v1/ingest";
pub const MAX_SYNC_MACHINES: usize = 1_000;
pub const MILLIS_PER_HOUR: u64 = 3_600_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeIdentity {
    pub hostname: String,
    pub distro: String,
    pub distro_version: String,
    pub kernel_version: String,
    pub ip_address: Option<String>,
    #[serde(default)]
    pub gpu_types: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricSample {
    pub sample_id: String,
    pub captured_at_ms: i64,
    pub cpu: CpuMetric,
    pub memory: MemoryMetric,
    pub disks: Vec<DiskMetric>,
    pub networks: Vec<NetworkMetric>,
    pub gpus: Vec<GpuMetric>,
    pub power_watts: Option<f64>,
    pub uptime_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpuMetric {
    pub usage_percent: f32,
    pub temperature_celsius: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetric {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskMetric {
    pub device: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMetric {
    pub interface: String,
    pub received_bytes_per_second: f64,
    pub transmitted_bytes_per_second: f64,
    pub total_received_bytes: u64,
    pub total_transmitted_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetric {
    pub device: String,
    pub driver: Option<String>,
    pub usage_percent: Option<f32>,
    pub temperature_celsius: Option<f32>,
    pub vram_total_bytes: Option<u64>,
    pub vram_used_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestRequest {
    pub identity: ProbeIdentity,
    pub retention_hours: u64,
    pub collection_interval_seconds: u64,
    pub samples: Vec<MetricSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestResponse {
    pub accepted_sample_ids: Vec<String>,
    pub server_time_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub device_id: String,
    pub display_name: String,
    pub identity: ProbeIdentity,
    pub retention_hours: u64,
    pub collection_interval_seconds: u64,
    pub last_metric_time_ms: i64,
    pub online: bool,
    pub latest_sample: Option<MetricSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIndexResponse {
    pub revision: String,
    pub total: usize,
    pub online: usize,
    pub offline: usize,
    pub last_metric_time_ms: Option<i64>,
    pub device_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummarySyncRequest {
    pub device_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummarySyncResponse {
    pub server_time_ms: i64,
    pub devices: BTreeMap<String, Option<DeviceSummary>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistorySyncRequest {
    pub machines: BTreeMap<String, Option<i64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MachineHistory {
    pub retention_hours: u64,
    pub collection_interval_seconds: u64,
    pub next_metric_time_ms: Option<i64>,
    pub samples: Vec<MetricSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistorySyncResponse {
    pub server_time_ms: i64,
    pub machines: BTreeMap<String, MachineHistory>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct ValidationError(pub String);

impl IngestRequest {
    pub fn validate(&self, max_batch_size: usize) -> Result<(), ValidationError> {
        if self.samples.len() > max_batch_size {
            return Err(ValidationError(format!(
                "batch exceeds GAUGES_MAX_INGEST_BATCH_SIZE ({max_batch_size})"
            )));
        }
        if self.retention_hours == 0 || self.retention_hours > i64::MAX as u64 / MILLIS_PER_HOUR {
            return Err(ValidationError(
                "retentionHours must be a positive whole number of hours".into(),
            ));
        }
        if self.collection_interval_seconds == 0
            || self.collection_interval_seconds > i64::MAX as u64 / 2_000
        {
            return Err(ValidationError(
                "collectionIntervalSeconds must be a positive whole number of seconds".into(),
            ));
        }
        if self.samples.is_empty() {
            return Err(ValidationError(
                "ingest requires at least one sample".into(),
            ));
        }
        self.identity.validate()?;
        for sample in &self.samples {
            if sample.sample_id.trim().is_empty() || sample.sample_id.len() > 128 {
                return Err(ValidationError(
                    "sampleId must contain 1 to 128 characters".into(),
                ));
            }
            if sample.captured_at_ms < 0 {
                return Err(ValidationError(
                    "sample timestamps and uptime must not be negative".into(),
                ));
            }
        }
        Ok(())
    }
}

impl DeviceSummarySyncRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.device_ids.len() > MAX_SYNC_MACHINES {
            return Err(ValidationError(format!(
                "summary sync supports at most {MAX_SYNC_MACHINES} machines"
            )));
        }
        if self
            .device_ids
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 128)
        {
            return Err(ValidationError(
                "device ids must contain 1 to 128 characters".into(),
            ));
        }
        Ok(())
    }
}

impl HistorySyncRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.machines.len() > MAX_SYNC_MACHINES {
            return Err(ValidationError(format!(
                "history sync supports at most {MAX_SYNC_MACHINES} machines"
            )));
        }
        if self.machines.iter().any(|(id, timestamp)| {
            id.trim().is_empty()
                || id.len() > 128
                || timestamp.is_some_and(|timestamp| timestamp < 0)
        }) {
            return Err(ValidationError(
                "history cursors require valid device ids and non-negative times".into(),
            ));
        }
        Ok(())
    }
}

impl ProbeIdentity {
    fn validate(&self) -> Result<(), ValidationError> {
        let values = [
            &self.hostname,
            &self.distro,
            &self.distro_version,
            &self.kernel_version,
        ];
        if values
            .into_iter()
            .any(|value| value.trim().is_empty() || value.len() > 512)
            || self
                .ip_address
                .as_ref()
                .is_some_and(|value| value.len() > 512)
            || self.gpu_types.len() > 32
            || self
                .gpu_types
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err(ValidationError(
                "identity fields must contain 1 to 512 characters and gpuTypes at most 32 entries"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_remain_camel_case() {
        let response = IngestResponse {
            accepted_sample_ids: vec!["sample".into()],
            server_time_ms: 42,
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["acceptedSampleIds"][0], "sample");
        assert_eq!(json["serverTimeMs"], 42);
    }
}
