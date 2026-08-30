use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{MAX_SYNC_MACHINES, MetricSample, ProbeIdentity, ValidationError};

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
