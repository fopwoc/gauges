use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ProbeConfig {
    pub device_id: String,
    pub token: String,
    pub hub_url: String,
    pub database_path: PathBuf,
    pub collection_interval_seconds: u64,
    pub retry_interval_seconds: u64,
    pub retention_hours: u64,
    pub upload_batch_size: usize,
    pub request_timeout_seconds: u64,
    pub physical_networks_only: bool,
    pub network_include: Vec<String>,
    pub network_exclude: Vec<String>,
    pub local_filesystems_only: bool,
    pub disk_include: Vec<String>,
    pub disk_exclude: Vec<String>,
    pub etc_root: PathBuf,
    pub sys_root: PathBuf,
    pub proc_root: PathBuf,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            device_id: String::new(),
            token: String::new(),
            hub_url: "http://127.0.0.1:8080".into(),
            database_path: "/var/lib/gauges/probe.redb".into(),
            collection_interval_seconds: 10,
            retry_interval_seconds: 60,
            retention_hours: 24,
            upload_batch_size: 250,
            request_timeout_seconds: 15,
            physical_networks_only: true,
            network_include: Vec::new(),
            network_exclude: Vec::new(),
            local_filesystems_only: true,
            disk_include: Vec::new(),
            disk_exclude: Vec::new(),
            etc_root: "/etc".into(),
            sys_root: "/sys".into(),
            proc_root: "/proc".into(),
        }
    }
}

impl ProbeConfig {
    pub fn load() -> Result<Self> {
        let explicit_path = config_path_from_args()?;
        let environment_path = env::var_os("CONFIG_PATH").map(PathBuf::from);
        let path_was_requested = explicit_path.is_some() || environment_path.is_some();
        let path = explicit_path
            .or(environment_path)
            .unwrap_or_else(|| PathBuf::from("/etc/gauges/probe.toml"));

        let mut config = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?
        } else if !path_was_requested {
            Self::default()
        } else {
            bail!("configured file {} does not exist", path.display());
        };

        config.apply_environment()?;
        config.validate()?;
        Ok(config)
    }

    fn apply_environment(&mut self) -> Result<()> {
        set_string("DEVICE_ID", &mut self.device_id);
        set_string("TOKEN", &mut self.token);
        set_string("HUB_URL", &mut self.hub_url);
        set_path("DATABASE_PATH", &mut self.database_path);
        set_path("ETC_ROOT", &mut self.etc_root);
        set_path("SYS_ROOT", &mut self.sys_root);
        set_path("PROC_ROOT", &mut self.proc_root);
        set_number(
            "COLLECTION_INTERVAL_SECONDS",
            &mut self.collection_interval_seconds,
        )?;
        set_number("RETRY_INTERVAL_SECONDS", &mut self.retry_interval_seconds)?;
        set_number("RETENTION_HOURS", &mut self.retention_hours)?;
        set_number("UPLOAD_BATCH_SIZE", &mut self.upload_batch_size)?;
        set_number("REQUEST_TIMEOUT_SECONDS", &mut self.request_timeout_seconds)?;
        set_bool("PHYSICAL_NETWORKS_ONLY", &mut self.physical_networks_only)?;
        set_list("NETWORK_INCLUDE", &mut self.network_include);
        set_list("NETWORK_EXCLUDE", &mut self.network_exclude);
        set_bool("LOCAL_FILESYSTEMS_ONLY", &mut self.local_filesystems_only)?;
        set_list("DISK_INCLUDE", &mut self.disk_include);
        set_list("DISK_EXCLUDE", &mut self.disk_exclude);
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.device_id.trim().is_empty() {
            bail!("device_id is required (or set DEVICE_ID)");
        }
        if self.token.is_empty() {
            bail!("token is required (or set TOKEN)");
        }
        if !(self.hub_url.starts_with("http://") || self.hub_url.starts_with("https://")) {
            bail!("hub_url must start with http:// or https://");
        }
        if self.collection_interval_seconds == 0
            || self.retry_interval_seconds == 0
            || self.retention_hours == 0
            || self.upload_batch_size == 0
            || self.request_timeout_seconds == 0
        {
            bail!("all durations, retention, and upload_batch_size must be greater than zero");
        }
        Ok(())
    }
}

fn config_path_from_args() -> Result<Option<PathBuf>> {
    let mut args = env::args_os().skip(1);
    let Some(argument) = args.next() else {
        return Ok(None);
    };
    if argument == "--config" || argument == "-c" {
        return args
            .next()
            .map(PathBuf::from)
            .map(Some)
            .context("--config requires a path");
    }
    bail!(
        "unknown argument {:?}; usage: gauges-probe [--config PATH]",
        argument
    )
}

fn set_string(name: &str, target: &mut String) {
    if let Ok(value) = env::var(name) {
        *target = value;
    }
}

fn set_path(name: &str, target: &mut PathBuf) {
    if let Some(value) = env::var_os(name) {
        *target = value.into();
    }
}

fn set_number<T>(name: &str, target: &mut T) -> Result<()>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    if let Ok(value) = env::var(name) {
        *target = value
            .parse()
            .with_context(|| format!("{name} must be a positive integer"))?;
    }
    Ok(())
}

fn set_bool(name: &str, target: &mut bool) -> Result<()> {
    if let Ok(value) = env::var(name) {
        *target = value
            .parse()
            .with_context(|| format!("{name} must be true or false"))?;
    }
    Ok(())
}

fn set_list(name: &str, target: &mut Vec<String>) {
    if let Ok(value) = env::var(name) {
        *target = value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_filter_defaults_are_conservative() {
        let config = ProbeConfig::default();

        assert!(config.local_filesystems_only);
        assert!(config.disk_include.is_empty());
        assert!(config.disk_exclude.is_empty());
        assert_eq!(config.etc_root, PathBuf::from("/etc"));
        assert_eq!(config.proc_root, PathBuf::from("/proc"));
    }

    #[test]
    fn disk_filter_toml_fields_are_deserialized() {
        let config: ProbeConfig = toml::from_str(
            r#"
                local_filesystems_only = false
                disk_include = ["/srv/data"]
                disk_exclude = ["/boot"]
                etc_root = "/host/etc"
                proc_root = "/host/proc"
            "#,
        )
        .unwrap();

        assert!(!config.local_filesystems_only);
        assert_eq!(config.disk_include, ["/srv/data"]);
        assert_eq!(config.disk_exclude, ["/boot"]);
        assert_eq!(config.etc_root, PathBuf::from("/host/etc"));
        assert_eq!(config.proc_root, PathBuf::from("/host/proc"));
    }
}
