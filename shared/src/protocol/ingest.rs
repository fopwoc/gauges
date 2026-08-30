use serde::{Deserialize, Serialize};

use super::{MILLIS_PER_HOUR, MetricSample, ProbeIdentity, ValidationError};

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

impl IngestRequest {
    pub fn validate(&self, max_batch_size: usize) -> Result<(), ValidationError> {
        if self.samples.len() > max_batch_size {
            return Err(ValidationError(format!(
                "batch exceeds MAX_INGEST_BATCH_SIZE ({max_batch_size})"
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
