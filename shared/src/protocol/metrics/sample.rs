use serde::{Deserialize, Serialize};

use super::{CpuMetric, GpuMetric, MemoryMetric, NetworkMetric, StorageMetric};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricSample {
    pub sample_id: String,
    pub captured_at_ms: i64,
    pub cpu: CpuMetric,
    pub memory: MemoryMetric,
    pub storage: Vec<StorageMetric>,
    pub networks: Vec<NetworkMetric>,
    pub gpus: Vec<GpuMetric>,
    pub power_watts: Option<f64>,
    pub uptime_seconds: u64,
}
