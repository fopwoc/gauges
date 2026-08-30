use gauges_shared::MetricSample;

#[derive(Clone, Debug, PartialEq)]
pub struct StoredHistory {
    pub retention_hours: u64,
    pub collection_interval_seconds: u64,
    pub samples: Vec<MetricSample>,
    pub next_metric_time_ms: Option<i64>,
}
