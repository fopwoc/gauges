mod support;

use gauges_hub::HubDatabase;
use gauges_shared::MILLIS_PER_HOUR;
use redb::{Database, ReadableDatabase, TableDefinition};
use support::{test_identity, test_sample};

#[test]
fn duplicate_sample_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let database = HubDatabase::open(&directory.path().join("hub.redb")).unwrap();
    let sample = test_sample("sample-1", 1_000);

    assert_eq!(
        database
            .ingest(
                "server-a",
                "Server A",
                &test_identity(),
                24,
                60,
                std::slice::from_ref(&sample),
                2_000,
            )
            .unwrap(),
        ["sample-1"]
    );
    database
        .ingest(
            "server-a",
            "Server A",
            &test_identity(),
            24,
            60,
            std::slice::from_ref(&sample),
            3_000,
        )
        .unwrap();

    let history = database.history("server-a", None, 3_000).unwrap().unwrap();
    assert_eq!(history.samples, [sample]);
    assert_eq!(history.collection_interval_seconds, 60);
    assert_eq!(
        database.devices(&["server-a".into()], 0).unwrap()[0].last_metric_time_ms,
        1_000
    );
    assert!(database.history("missing", None, 3_000).unwrap().is_none());
}

#[test]
fn retention_uses_receipt_time_while_history_uses_probe_time() {
    let directory = tempfile::tempdir().unwrap();
    let database = HubDatabase::open(&directory.path().join("hub.redb")).unwrap();
    database
        .ingest(
            "server-a",
            "Server A",
            &test_identity(),
            1,
            10,
            &[test_sample("old", 9_000_000)],
            1_000,
        )
        .unwrap();
    database
        .ingest(
            "server-a",
            "Server A",
            &test_identity(),
            1,
            10,
            &[test_sample("new", 2_000)],
            3_000,
        )
        .unwrap();

    let history = database
        .history("server-a", Some(1_500), 3_000)
        .unwrap()
        .unwrap();
    assert_eq!(
        history
            .samples
            .iter()
            .map(|sample| sample.sample_id.as_str())
            .collect::<Vec<_>>(),
        ["new", "old"]
    );
    assert_eq!(
        database
            .delete_expired_samples(MILLIS_PER_HOUR as i64 + 1_500)
            .unwrap(),
        1
    );
    assert_eq!(
        database.devices(&["server-a".into()], 0).unwrap()[0]
            .latest_sample
            .as_ref()
            .unwrap()
            .sample_id,
        "new"
    );
}

#[test]
fn cleanup_applies_each_probe_retention() {
    let directory = tempfile::tempdir().unwrap();
    let database = HubDatabase::open(&directory.path().join("hub.redb")).unwrap();
    let received_at_ms = MILLIS_PER_HOUR as i64;
    let now_ms = 8 * MILLIS_PER_HOUR as i64;
    database
        .ingest(
            "short",
            "Short history",
            &test_identity(),
            1,
            10,
            &[test_sample("short-sample", 1_000)],
            received_at_ms,
        )
        .unwrap();
    database
        .ingest(
            "long",
            "Long history",
            &test_identity(),
            24,
            60,
            &[test_sample("long-sample", 1_000)],
            received_at_ms,
        )
        .unwrap();

    assert_eq!(database.delete_expired_samples(now_ms).unwrap(), 1);
    let short = database.history("short", None, now_ms).unwrap().unwrap();
    let long = database.history("long", None, now_ms).unwrap().unwrap();
    assert!(short.samples.is_empty());
    assert_eq!(long.samples[0].sample_id, "long-sample");
    assert_eq!(short.retention_hours, 1);
    assert_eq!(long.retention_hours, 24);
}

#[test]
fn probe_time_cursor_is_strictly_exclusive() {
    let directory = tempfile::tempdir().unwrap();
    let database = HubDatabase::open(&directory.path().join("hub.redb")).unwrap();
    database
        .ingest(
            "server-a",
            "Server A",
            &test_identity(),
            24,
            10,
            &[
                test_sample("first", 1_000),
                test_sample("boundary-a", 2_000),
            ],
            3_000,
        )
        .unwrap();
    let first = database.history("server-a", None, 3_000).unwrap().unwrap();
    assert_eq!(first.next_metric_time_ms, Some(2_000));
    database
        .ingest(
            "server-a",
            "Server A",
            &test_identity(),
            24,
            10,
            &[
                test_sample("boundary-b", 2_000),
                test_sample("newer", 2_500),
            ],
            4_000,
        )
        .unwrap();

    let incremental = database
        .history("server-a", first.next_metric_time_ms, 4_000)
        .unwrap()
        .unwrap();
    assert_eq!(
        incremental
            .samples
            .iter()
            .map(|sample| sample.sample_id.as_str())
            .collect::<Vec<_>>(),
        ["newer"]
    );
    assert_eq!(incremental.next_metric_time_ms, Some(2_500));
}

#[test]
fn legacy_sqlite_file_is_replaced_by_redb() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hub.db");
    std::fs::write(&path, b"SQLite format 3\0legacy").unwrap();

    let database = HubDatabase::open(&path).unwrap();
    assert!(database.is_healthy());
    assert!(
        !std::fs::read(path)
            .unwrap()
            .starts_with(b"SQLite format 3\0")
    );
}

#[test]
fn schema_mismatch_resets_redb_tables() {
    const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
    const DEVICES: TableDefinition<&str, &[u8]> = TableDefinition::new("devices");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hub.redb");
    let database = Database::create(&path).unwrap();
    let transaction = database.begin_write().unwrap();
    {
        let mut meta = transaction.open_table(META).unwrap();
        meta.insert("schema_version", 999).unwrap();
        let mut devices = transaction.open_table(DEVICES).unwrap();
        devices.insert("old", b"data".as_slice()).unwrap();
    }
    transaction.commit().unwrap();
    drop(database);

    let database = HubDatabase::open(&path).unwrap();
    assert!(database.is_healthy());
    drop(database);
    let database = Database::create(path).unwrap();
    let transaction = database.begin_read().unwrap();
    let devices = transaction.open_table(DEVICES).unwrap();
    assert!(devices.get("old").unwrap().is_none());
}
