use gauges_shared::{MetricSample, ProbeIdentity};

#[derive(Clone, Debug, PartialEq)]
pub struct StoredDevice {
    pub device_id: String,
    pub display_name: String,
    pub identity: ProbeIdentity,
    pub retention_hours: u64,
    pub collection_interval_seconds: u64,
    pub last_metric_time_ms: i64,
    pub latest_sample: Option<MetricSample>,
}
