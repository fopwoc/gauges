mod model;

use std::{io::Read, path::Path, sync::Arc};

use anyhow::{Context, Result};
use gauges_shared::{MILLIS_PER_HOUR, MetricSample, ProbeIdentity};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

pub use model::{StoredDevice, StoredDeviceStatus, StoredHistory};

const SCHEMA_VERSION: u64 = 2;
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const DEVICES: TableDefinition<&str, &[u8]> = TableDefinition::new("devices");
const SAMPLES_BY_TIME: TableDefinition<&[u8], &[u8]> = TableDefinition::new("samples_by_time");
const SAMPLE_LOCATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("sample_locations");
const SAMPLES_BY_RECEIPT: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("samples_by_receipt");

#[derive(Clone)]
pub struct HubDatabase {
    database: Arc<Database>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StoredDeviceDocument {
    display_name: String,
    identity: ProbeIdentity,
    retention_hours: u64,
    collection_interval_seconds: u64,
    last_metric_time_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSampleDocument {
    received_at_ms: i64,
    sample: MetricSample,
}

impl HubDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        remove_legacy_sqlite(path)?;
        let database =
            Database::create(path).with_context(|| format!("failed to open {}", path.display()))?;
        initialize_schema(&database)?;
        Ok(Self {
            database: Arc::new(database),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ingest(
        &self,
        device_id: &str,
        display_name: &str,
        identity: &ProbeIdentity,
        retention_hours: u64,
        collection_interval_seconds: u64,
        samples: &[MetricSample],
        received_at_ms: i64,
    ) -> Result<Vec<String>> {
        let batch_last_metric_time_ms = samples
            .iter()
            .map(|sample| sample.captured_at_ms)
            .max()
            .context("ingest requires at least one sample")?;
        let transaction = self.database.begin_write()?;
        let stored_last_metric_time_ms = {
            let devices = transaction.open_table(DEVICES)?;
            devices
                .get(device_id)?
                .map(|value| serde_json::from_slice::<StoredDeviceDocument>(value.value()))
                .transpose()?
                .map_or(batch_last_metric_time_ms, |device| {
                    device.last_metric_time_ms.max(batch_last_metric_time_ms)
                })
        };

        for sample in samples {
            let id_key = id_key(device_id, &sample.sample_id);
            let exists = {
                let locations = transaction.open_table(SAMPLE_LOCATIONS)?;
                locations.get(id_key.as_slice())?.is_some()
            };
            if exists {
                continue;
            }

            let sample_time_key = sample_time_key(
                device_id,
                sample.captured_at_ms,
                received_at_ms,
                &sample.sample_id,
            );
            let receipt_key = time_key(device_id, received_at_ms, &sample.sample_id);
            let payload = serde_json::to_vec(&StoredSampleDocument {
                received_at_ms,
                sample: sample.clone(),
            })?;
            {
                let mut by_time = transaction.open_table(SAMPLES_BY_TIME)?;
                by_time.insert(sample_time_key.as_slice(), payload.as_slice())?;
            }
            {
                let mut locations = transaction.open_table(SAMPLE_LOCATIONS)?;
                locations.insert(id_key.as_slice(), sample_time_key.as_slice())?;
            }
            {
                let mut by_receipt = transaction.open_table(SAMPLES_BY_RECEIPT)?;
                by_receipt.insert(receipt_key.as_slice(), id_key.as_slice())?;
            }
        }

        let device = StoredDeviceDocument {
            display_name: display_name.into(),
            identity: identity.clone(),
            retention_hours,
            collection_interval_seconds,
            last_metric_time_ms: stored_last_metric_time_ms,
        };
        let payload = serde_json::to_vec(&device)?;
        {
            let mut devices = transaction.open_table(DEVICES)?;
            devices.insert(device_id, payload.as_slice())?;
        }
        transaction.commit()?;
        Ok(samples
            .iter()
            .map(|sample| sample.sample_id.clone())
            .collect())
    }

    pub fn devices(&self, device_ids: &[String], now_ms: i64) -> Result<Vec<StoredDevice>> {
        let transaction = self.database.begin_read()?;
        let devices = transaction.open_table(DEVICES)?;
        let samples = transaction.open_table(SAMPLES_BY_TIME)?;
        let mut result = Vec::new();
        for device_id in device_ids {
            let Some(document) = devices.get(device_id.as_str())? else {
                continue;
            };
            let document: StoredDeviceDocument = serde_json::from_slice(document.value())?;
            let cutoff = retention_cutoff_ms(now_ms, document.retention_hours);
            let prefix = device_prefix(device_id);
            let upper = prefix_end(&prefix);
            let mut latest_sample = None;
            for entry in samples.range(prefix.as_slice()..upper.as_slice())?.rev() {
                let (_, payload) = entry?;
                let stored: StoredSampleDocument = serde_json::from_slice(payload.value())?;
                if stored.received_at_ms >= cutoff {
                    latest_sample = Some(stored.sample);
                    break;
                }
            }
            result.push(StoredDevice {
                device_id: device_id.clone(),
                display_name: document.display_name,
                identity: document.identity,
                retention_hours: document.retention_hours,
                collection_interval_seconds: document.collection_interval_seconds,
                last_metric_time_ms: document.last_metric_time_ms,
                latest_sample,
            });
        }
        Ok(result)
    }

    pub fn device_statuses(&self, device_ids: &[String]) -> Result<Vec<StoredDeviceStatus>> {
        let transaction = self.database.begin_read()?;
        let devices = transaction.open_table(DEVICES)?;
        let mut result = Vec::new();
        for device_id in device_ids {
            let Some(document) = devices.get(device_id.as_str())? else {
                continue;
            };
            let document: StoredDeviceDocument = serde_json::from_slice(document.value())?;
            result.push(StoredDeviceStatus {
                collection_interval_seconds: document.collection_interval_seconds,
                last_metric_time_ms: document.last_metric_time_ms,
            });
        }
        Ok(result)
    }

    pub fn history(
        &self,
        device_id: &str,
        last_metric_time_ms: Option<i64>,
        now_ms: i64,
    ) -> Result<Option<StoredHistory>> {
        let transaction = self.database.begin_read()?;
        let devices = transaction.open_table(DEVICES)?;
        let Some(document) = devices.get(device_id)? else {
            return Ok(None);
        };
        let document: StoredDeviceDocument = serde_json::from_slice(document.value())?;
        let cutoff = retention_cutoff_ms(now_ms, document.retention_hours);
        let start_timestamp = match last_metric_time_ms {
            Some(i64::MAX) => {
                return Ok(Some(StoredHistory {
                    retention_hours: document.retention_hours,
                    collection_interval_seconds: document.collection_interval_seconds,
                    samples: Vec::new(),
                    next_metric_time_ms: last_metric_time_ms,
                }));
            }
            Some(timestamp) => timestamp + 1,
            None => 0,
        };
        let start = time_prefix(device_id, start_timestamp);
        let upper = prefix_end(&device_prefix(device_id));
        let samples_table = transaction.open_table(SAMPLES_BY_TIME)?;
        let mut samples = Vec::new();
        for entry in samples_table.range(start.as_slice()..upper.as_slice())? {
            let (_, payload) = entry?;
            let stored: StoredSampleDocument = serde_json::from_slice(payload.value())?;
            if stored.received_at_ms >= cutoff {
                samples.push(stored.sample);
            }
        }
        samples.sort_by(|left, right| {
            left.captured_at_ms
                .cmp(&right.captured_at_ms)
                .then_with(|| left.sample_id.cmp(&right.sample_id))
        });
        let next_metric_time_ms = samples
            .iter()
            .map(|sample| sample.captured_at_ms)
            .max()
            .or(last_metric_time_ms);
        Ok(Some(StoredHistory {
            retention_hours: document.retention_hours,
            collection_interval_seconds: document.collection_interval_seconds,
            samples,
            next_metric_time_ms,
        }))
    }

    pub fn delete_expired_samples(&self, now_ms: i64) -> Result<usize> {
        let transaction = self.database.begin_write()?;
        let device_documents = {
            let devices = transaction.open_table(DEVICES)?;
            devices
                .iter()?
                .map(|entry| {
                    let (device_id, document) = entry?;
                    Ok::<_, anyhow::Error>((
                        device_id.value().to_owned(),
                        serde_json::from_slice::<StoredDeviceDocument>(document.value())?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?
        };

        let mut expired = Vec::new();
        for (device_id, document) in device_documents {
            let cutoff = retention_cutoff_ms(now_ms, document.retention_hours);
            if cutoff <= 0 {
                continue;
            }
            let start = device_prefix(&device_id);
            let end = time_prefix(&device_id, cutoff);
            let by_receipt = transaction.open_table(SAMPLES_BY_RECEIPT)?;
            for entry in by_receipt.range(start.as_slice()..end.as_slice())? {
                let (receipt_key, id_key) = entry?;
                expired.push((receipt_key.value().to_vec(), id_key.value().to_vec()));
            }
        }

        let mut deleted = 0;
        for (receipt_key, id_key) in expired {
            let location = {
                let locations = transaction.open_table(SAMPLE_LOCATIONS)?;
                locations
                    .get(id_key.as_slice())?
                    .map(|value| value.value().to_vec())
            };
            if let Some(location) = location {
                {
                    let mut by_time = transaction.open_table(SAMPLES_BY_TIME)?;
                    by_time.remove(location.as_slice())?;
                }
                {
                    let mut locations = transaction.open_table(SAMPLE_LOCATIONS)?;
                    locations.remove(id_key.as_slice())?;
                }
                deleted += 1;
            }
            {
                let mut by_receipt = transaction.open_table(SAMPLES_BY_RECEIPT)?;
                by_receipt.remove(receipt_key.as_slice())?;
            }
        }
        transaction.commit()?;
        Ok(deleted)
    }

    pub fn is_healthy(&self) -> bool {
        (|| -> Result<bool> {
            let transaction = self.database.begin_read()?;
            let meta = transaction.open_table(META)?;
            Ok(meta
                .get("schema_version")?
                .is_some_and(|version| version.value() == SCHEMA_VERSION))
        })()
        .unwrap_or(false)
    }
}

fn initialize_schema(database: &Database) -> Result<()> {
    let current_version = {
        let transaction = database.begin_read()?;
        match transaction.open_table(META) {
            Ok(meta) => meta.get("schema_version")?.map(|value| value.value()),
            Err(redb::TableError::TableDoesNotExist(_)) => None,
            Err(error) => return Err(error.into()),
        }
    };
    if current_version == Some(SCHEMA_VERSION) {
        return Ok(());
    }

    let transaction = database.begin_write()?;
    transaction.delete_table(SAMPLES_BY_RECEIPT)?;
    transaction.delete_table(SAMPLE_LOCATIONS)?;
    transaction.delete_table(SAMPLES_BY_TIME)?;
    transaction.delete_table(DEVICES)?;
    transaction.delete_table(META)?;
    {
        let mut meta = transaction.open_table(META)?;
        meta.insert("schema_version", SCHEMA_VERSION)?;
    }
    {
        transaction.open_table(DEVICES)?;
        transaction.open_table(SAMPLES_BY_TIME)?;
        transaction.open_table(SAMPLE_LOCATIONS)?;
        transaction.open_table(SAMPLES_BY_RECEIPT)?;
    }
    transaction.commit()?;
    Ok(())
}

fn remove_legacy_sqlite(path: &Path) -> Result<()> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Ok(());
    };
    let mut header = [0_u8; 16];
    if file.read_exact(&mut header).is_ok() && &header == b"SQLite format 3\0" {
        drop(file);
        std::fs::remove_file(path)
            .with_context(|| format!("failed to replace legacy SQLite file {}", path.display()))?;
    }
    Ok(())
}

fn retention_cutoff_ms(now_ms: i64, retention_hours: u64) -> i64 {
    let retention_ms = retention_hours.saturating_mul(MILLIS_PER_HOUR);
    now_ms.saturating_sub(i64::try_from(retention_ms).unwrap_or(i64::MAX))
}

fn device_prefix(device_id: &str) -> Vec<u8> {
    let bytes = device_id.as_bytes();
    let mut key = Vec::with_capacity(2 + bytes.len());
    key.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    key.extend_from_slice(bytes);
    key
}

fn id_key(device_id: &str, sample_id: &str) -> Vec<u8> {
    let mut key = device_prefix(device_id);
    key.extend_from_slice(sample_id.as_bytes());
    key
}

fn time_prefix(device_id: &str, timestamp_ms: i64) -> Vec<u8> {
    let mut key = device_prefix(device_id);
    key.extend_from_slice(
        &u64::try_from(timestamp_ms)
            .unwrap_or_default()
            .to_be_bytes(),
    );
    key
}

fn time_key(device_id: &str, timestamp_ms: i64, sample_id: &str) -> Vec<u8> {
    let mut key = time_prefix(device_id, timestamp_ms);
    key.extend_from_slice(sample_id.as_bytes());
    key
}

fn sample_time_key(
    device_id: &str,
    captured_at_ms: i64,
    received_at_ms: i64,
    sample_id: &str,
) -> Vec<u8> {
    let mut key = time_prefix(device_id, captured_at_ms);
    key.extend_from_slice(
        &u64::try_from(received_at_ms)
            .unwrap_or_default()
            .to_be_bytes(),
    );
    key.extend_from_slice(sample_id.as_bytes());
    key
}

fn prefix_end(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    for index in (0..end.len()).rev() {
        if end[index] != u8::MAX {
            end[index] += 1;
            end.truncate(index + 1);
            return end;
        }
    }
    vec![u8::MAX]
}
