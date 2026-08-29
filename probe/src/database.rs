use std::{io::Read, path::Path, sync::Arc};

use anyhow::{Context, Result};
use gauges_shared::MetricSample;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};

const SCHEMA_VERSION: u64 = 2;
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const SAMPLES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("samples");
const LOCATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sample_locations");

#[derive(Clone)]
pub struct SampleDatabase {
    database: Arc<Database>,
}

impl SampleDatabase {
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

    pub fn insert(&self, sample: &MetricSample) -> Result<()> {
        let key = sample_key(sample.captured_at_ms, &sample.sample_id);
        let payload = serde_json::to_vec(sample)?;
        let transaction = self.database.begin_write()?;
        let exists = {
            let locations = transaction.open_table(LOCATIONS)?;
            locations.get(sample.sample_id.as_str())?.is_some()
        };
        if !exists {
            {
                let mut samples = transaction.open_table(SAMPLES)?;
                samples.insert(key.as_slice(), payload.as_slice())?;
            }
            {
                let mut locations = transaction.open_table(LOCATIONS)?;
                locations.insert(sample.sample_id.as_str(), key.as_slice())?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn oldest_batch(&self, limit: usize) -> Result<Vec<MetricSample>> {
        let transaction = self.database.begin_read()?;
        let samples = transaction.open_table(SAMPLES)?;
        samples
            .iter()?
            .take(limit)
            .map(|entry| {
                let (_, payload) = entry?;
                serde_json::from_slice(payload.value()).context("invalid locally buffered sample")
            })
            .collect()
    }

    pub fn delete_accepted(&self, sample_ids: &[String]) -> Result<usize> {
        let transaction = self.database.begin_write()?;
        let mut deleted = 0;
        for sample_id in sample_ids {
            let location = {
                let locations = transaction.open_table(LOCATIONS)?;
                locations
                    .get(sample_id.as_str())?
                    .map(|value| value.value().to_vec())
            };
            let Some(location) = location else {
                continue;
            };
            {
                let mut samples = transaction.open_table(SAMPLES)?;
                samples.remove(location.as_slice())?;
            }
            {
                let mut locations = transaction.open_table(LOCATIONS)?;
                locations.remove(sample_id.as_str())?;
            }
            deleted += 1;
        }
        transaction.commit()?;
        Ok(deleted)
    }

    pub fn prune_before(&self, cutoff_ms: i64) -> Result<usize> {
        let upper = u64::try_from(cutoff_ms).unwrap_or_default().to_be_bytes();
        let transaction = self.database.begin_write()?;
        let expired = {
            let samples = transaction.open_table(SAMPLES)?;
            samples
                .range::<&[u8]>(..upper.as_slice())?
                .map(|entry| {
                    let (key, _) = entry?;
                    let key = key.value().to_vec();
                    let sample_id = String::from_utf8(key[8..].to_vec())?;
                    Ok::<_, anyhow::Error>((key, sample_id))
                })
                .collect::<Result<Vec<_>>>()?
        };
        for (key, sample_id) in &expired {
            {
                let mut samples = transaction.open_table(SAMPLES)?;
                samples.remove(key.as_slice())?;
            }
            {
                let mut locations = transaction.open_table(LOCATIONS)?;
                locations.remove(sample_id.as_str())?;
            }
        }
        transaction.commit()?;
        Ok(expired.len())
    }

    pub fn pending_count(&self) -> Result<u64> {
        let transaction = self.database.begin_read()?;
        let samples = transaction.open_table(SAMPLES)?;
        Ok(samples.len()?)
    }
}

fn sample_key(captured_at_ms: i64, sample_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(8 + sample_id.len());
    key.extend_from_slice(
        &u64::try_from(captured_at_ms)
            .unwrap_or_default()
            .to_be_bytes(),
    );
    key.extend_from_slice(sample_id.as_bytes());
    key
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
    transaction.delete_table(LOCATIONS)?;
    transaction.delete_table(SAMPLES)?;
    transaction.delete_table(META)?;
    {
        let mut meta = transaction.open_table(META)?;
        meta.insert("schema_version", SCHEMA_VERSION)?;
    }
    transaction.open_table(SAMPLES)?;
    transaction.open_table(LOCATIONS)?;
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
