use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{MAX_SYNC_MACHINES, MetricSample, ValidationError};

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
