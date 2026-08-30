use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfiguredDevice {
    pub id: String,
    pub display_name: String,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct HubConfig {
    pub host: String,
    pub port: u16,
    pub database_path: PathBuf,
    pub console_path: PathBuf,
    pub devices: BTreeMap<String, ConfiguredDevice>,
    pub cleanup_interval: Duration,
    pub online_threshold: Duration,
    pub max_ingest_batch_size: usize,
}

impl HubConfig {
    pub fn from_environment() -> Result<Self> {
        Self::from_map(&std::env::vars().collect())
    }

    pub fn from_map(environment: &BTreeMap<String, String>) -> Result<Self> {
        Ok(Self {
            host: non_empty(environment, "HTTP_HOST").unwrap_or_else(|| "0.0.0.0".into()),
            port: positive(environment, "HTTP_PORT", 8_080)?,
            database_path: non_empty(environment, "DATABASE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/data/gauges.redb".into()),
            console_path: non_empty(environment, "CONSOLE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/opt/gauges/console".into()),
            devices: parse_devices(
                environment
                    .get("DEVICES_JSON")
                    .context("DEVICES_JSON is required")?,
            )?,
            cleanup_interval: Duration::from_secs(positive(
                environment,
                "CLEANUP_INTERVAL_SECONDS",
                60_u64,
            )?),
            online_threshold: Duration::from_secs(positive(
                environment,
                "ONLINE_THRESHOLD_SECONDS",
                30_u64,
            )?),
            max_ingest_batch_size: positive(environment, "MAX_INGEST_BATCH_SIZE", 512_usize)?,
        })
    }
}

#[derive(Deserialize)]
struct DeviceConfigEntry {
    id: String,
    name: String,
    token: String,
}

fn parse_devices(value: &str) -> Result<BTreeMap<String, ConfiguredDevice>> {
    let entries: Vec<DeviceConfigEntry> =
        serde_json::from_str(value).context("DEVICES_JSON must be a valid JSON array")?;
    ensure!(
        !entries.is_empty(),
        "DEVICES_JSON must contain at least one device"
    );

    let mut devices = BTreeMap::new();
    for entry in entries {
        let id = entry.id.trim().to_owned();
        let display_name = entry.name.trim().to_owned();
        ensure!(
            valid_device_id(&id),
            "device id must contain only letters, digits, dots, underscores, or hyphens"
        );
        ensure!(
            !display_name.is_empty() && display_name.len() <= 128,
            "device name must contain 1 to 128 characters"
        );
        ensure!(!entry.token.is_empty(), "device token must not be empty");
        if devices
            .insert(
                id.clone(),
                ConfiguredDevice {
                    id: id.clone(),
                    display_name,
                    token: entry.token,
                },
            )
            .is_some()
        {
            bail!("duplicate device id in DEVICES_JSON: {id}");
        }
    }
    Ok(devices)
}

fn valid_device_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn non_empty(environment: &BTreeMap<String, String>, name: &str) -> Option<String> {
    environment
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn positive<T>(environment: &BTreeMap<String, String>, name: &str, default: T) -> Result<T>
where
    T: Copy + Default + PartialOrd + std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let Some(raw) = environment.get(name) else {
        return Ok(default);
    };
    let value = raw
        .parse::<T>()
        .with_context(|| format!("{name} must be a positive integer"))?;
    ensure!(value > T::default(), "{name} must be greater than zero");
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_defaults_and_devices() {
        let config = HubConfig::from_map(&BTreeMap::from([(
            "DEVICES_JSON".into(),
            r#"[{"id":"server-a","name":"Server A","token":"secret"},{"id":"router","name":"Router","token":"base64=="}]"#.into(),
        )]))
        .unwrap();

        assert_eq!(config.port, 8_080);
        assert_eq!(config.database_path, PathBuf::from("/data/gauges.redb"));
        assert_eq!(config.devices["server-a"].display_name, "Server A");
        assert_eq!(config.devices["router"].token, "base64==");
    }

    #[test]
    fn rejects_duplicate_devices() {
        let result = HubConfig::from_map(&BTreeMap::from([(
            "DEVICES_JSON".into(),
            r#"[{"id":"server-a","name":"One","token":"one"},{"id":"server-a","name":"Two","token":"two"}]"#.into(),
        )]));

        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("duplicate device id")
        );
    }
}
