use gauges_probe::database::SampleDatabase;
use gauges_shared::{CpuMetric, MemoryMetric, MetricSample};

fn sample(id: &str, captured_at_ms: i64) -> MetricSample {
    MetricSample {
        sample_id: id.into(),
        captured_at_ms,
        cpu: CpuMetric {
            usage_percent: 12.0,
            temperature_celsius: Some(48.0),
        },
        memory: MemoryMetric {
            total_bytes: 100,
            used_bytes: 50,
            available_bytes: 50,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
        },
        storage: vec![],
        networks: vec![],
        gpus: vec![],
        power_watts: None,
        uptime_seconds: 10,
    }
}

#[test]
fn buffers_orders_acknowledges_and_prunes_samples() {
    let directory = tempfile::tempdir().unwrap();
    let database = SampleDatabase::open(&directory.path().join("probe.redb")).unwrap();
    database.insert(&sample("new", 20)).unwrap();
    database.insert(&sample("old", 10)).unwrap();

    let batch = database.oldest_batch(10).unwrap();
    assert_eq!(
        batch
            .iter()
            .map(|item| item.sample_id.as_str())
            .collect::<Vec<_>>(),
        ["old", "new"]
    );

    assert_eq!(database.delete_accepted(&["old".into()]).unwrap(), 1);
    assert_eq!(database.pending_count().unwrap(), 1);
    assert_eq!(database.prune_before(21).unwrap(), 1);
    assert_eq!(database.pending_count().unwrap(), 0);
}
