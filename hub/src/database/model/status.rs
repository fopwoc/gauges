#[derive(Clone, Debug, PartialEq)]
pub struct StoredDeviceStatus {
    pub collection_interval_seconds: u64,
    pub last_metric_time_ms: i64,
}
