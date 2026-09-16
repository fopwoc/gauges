use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use gauges_shared::{
    CpuMetric, GpuMetric, MemoryMetric, MetricSample, NetworkMetric, ProbeIdentity,
};
use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};
use sysinfo::{Components, Networks, System};
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    config::ProbeConfig, drive_temperature, smart_health::SmartHealthCollector,
    storage::StorageCollector,
};

pub struct MetricsCollector {
    system: System,
    etc_root: PathBuf,
    sys_root: PathBuf,
    physical_networks_only: bool,
    network_include: Vec<String>,
    network_exclude: Vec<String>,
    previous_network_totals: HashMap<String, (u64, u64)>,
    previous_network_at: Instant,
    power: PowerCollector,
    nvidia: Option<Nvml>,
    storage: StorageCollector,
    smart_health: SmartHealthCollector,
}

impl MetricsCollector {
    pub fn new(config: &ProbeConfig) -> Self {
        let nvidia = match Nvml::init() {
            Ok(nvml) => Some(nvml),
            Err(error) => {
                if has_drm_driver(&config.sys_root, "nvidia") {
                    error!(%error, "NVIDIA DRM is present but NVML could not be initialized");
                } else {
                    debug!(%error, "NVML is unavailable; NVIDIA metrics are disabled");
                }
                None
            }
        };
        Self {
            system: System::new_all(),
            etc_root: config.etc_root.clone(),
            sys_root: config.sys_root.clone(),
            physical_networks_only: config.physical_networks_only,
            network_include: config.network_include.clone(),
            network_exclude: config.network_exclude.clone(),
            previous_network_totals: HashMap::new(),
            previous_network_at: Instant::now(),
            power: PowerCollector::default(),
            nvidia,
            storage: StorageCollector::new(config),
            smart_health: SmartHealthCollector::default(),
        }
    }

    pub fn identity(&self) -> ProbeIdentity {
        let mut gpus = collect_drm_gpus(&self.sys_root);
        if let Some(nvml) = &self.nvidia {
            gpus.extend(collect_nvidia_gpus(nvml));
        }
        probe_identity(
            cpu_model(&self.system),
            gpu_type_names(&gpus),
            &self.etc_root,
        )
    }

    pub fn collect(&mut self) -> Result<MetricSample> {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();

        let now = Instant::now();
        let network_elapsed = now
            .duration_since(self.previous_network_at)
            .max(Duration::from_millis(1));
        let networks = self.collect_networks(network_elapsed);
        self.previous_network_at = now;

        let memory = MemoryMetric {
            total_bytes: self.system.total_memory(),
            used_bytes: self.system.used_memory(),
            available_bytes: self.system.available_memory(),
            swap_total_bytes: self.system.total_swap(),
            swap_used_bytes: self.system.used_swap(),
        };

        let mut gpus = collect_drm_gpus(&self.sys_root);
        if let Some(nvml) = &self.nvidia {
            gpus.extend(collect_nvidia_gpus(nvml));
        }

        let captured_at_ms = unix_time_ms()?;
        let mut storage = self.storage.collect();
        self.smart_health
            .collect(&mut storage, &self.sys_root, captured_at_ms);

        Ok(MetricSample {
            sample_id: Uuid::new_v4().to_string(),
            captured_at_ms,
            cpu: CpuMetric {
                usage_percent: self.system.global_cpu_usage(),
                temperature_celsius: cpu_temperature(),
            },
            memory,
            storage,
            drive_temperatures: drive_temperature::collect(&self.sys_root),
            networks,
            gpus,
            power_watts: self.power.read_watts(&self.sys_root),
            uptime_seconds: System::uptime(),
        })
    }

    fn collect_networks(&mut self, elapsed: Duration) -> Vec<NetworkMetric> {
        let networks = Networks::new_with_refreshed_list();
        let mut result = Vec::new();
        for (name, data) in &networks {
            if !self.should_collect_network(name) {
                continue;
            }
            let received = data.total_received();
            let transmitted = data.total_transmitted();
            let (rx_rate, tx_rate) = self
                .previous_network_totals
                .get(name)
                .map(|(old_rx, old_tx)| {
                    (
                        received.saturating_sub(*old_rx) as f64 / elapsed.as_secs_f64(),
                        transmitted.saturating_sub(*old_tx) as f64 / elapsed.as_secs_f64(),
                    )
                })
                .unwrap_or((0.0, 0.0));
            self.previous_network_totals
                .insert(name.to_owned(), (received, transmitted));
            result.push(NetworkMetric {
                interface: name.to_owned(),
                received_bytes_per_second: rx_rate,
                transmitted_bytes_per_second: tx_rate,
                total_received_bytes: received,
                total_transmitted_bytes: transmitted,
            });
        }
        result.sort_by(|left, right| left.interface.cmp(&right.interface));
        result
    }

    fn should_collect_network(&self, name: &str) -> bool {
        if self.network_exclude.iter().any(|item| item == name) {
            return false;
        }
        if !self.network_include.is_empty() {
            return self.network_include.iter().any(|item| item == name);
        }
        if !self.physical_networks_only {
            return name != "lo";
        }
        let interface = self.sys_root.join("class/net").join(name);
        interface.join("device").exists() || interface.join("wireless").exists()
    }
}

pub fn probe_identity(
    cpu_model: Option<String>,
    gpu_types: Vec<String>,
    etc_root: &Path,
) -> ProbeIdentity {
    let (distro, distro_version) = distro_identity(etc_root);
    ProbeIdentity {
        hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
        distro,
        distro_version,
        kernel_version: System::kernel_version().unwrap_or_else(|| "unknown".into()),
        ip_address: local_ip_address::local_ip().ok().map(|ip| ip.to_string()),
        cpu_model,
        gpu_types,
    }
}

fn cpu_model(system: &System) -> Option<String> {
    system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim())
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
}

fn distro_identity(etc_root: &Path) -> (String, String) {
    if let Ok(contents) = fs::read_to_string(etc_root.join("os-release")) {
        let fields = contents
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let (key, value) = line.split_once('=')?;
                Some((key.trim(), value.trim().trim_matches(['\'', '"'])))
            })
            .collect::<HashMap<_, _>>();
        if let Some(name) = fields.get("NAME").or_else(|| fields.get("ID")) {
            let version = fields
                .get("VERSION")
                .or_else(|| fields.get("VERSION_ID"))
                .or_else(|| fields.get("BUILD_ID"))
                .copied()
                .unwrap_or("Unknown");
            return ((*name).to_owned(), version.to_owned());
        }
    }

    let os = os_info::get();
    (os.os_type().to_string(), os.version().to_string())
}

fn gpu_type_names(gpus: &[GpuMetric]) -> Vec<String> {
    let mut names = gpus
        .iter()
        .map(|gpu| match gpu.driver.as_deref() {
            Some(driver) if driver.starts_with("nvidia (") && driver.ends_with(')') => driver
                .strip_prefix("nvidia (")
                .and_then(|name| name.strip_suffix(')'))
                .unwrap_or(driver)
                .to_owned(),
            Some("amdgpu") => "AMD (amdgpu)".to_owned(),
            Some("radeon") => "AMD (radeon)".to_owned(),
            Some("i915") => "Intel (i915)".to_owned(),
            Some("xe") => "Intel (xe)".to_owned(),
            Some("nouveau") => "NVIDIA (nouveau)".to_owned(),
            Some(driver) => driver.to_owned(),
            None => gpu.device.clone(),
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn cpu_temperature() -> Option<f32> {
    let components = Components::new_with_refreshed_list();
    let preferred = components
        .iter()
        .filter(|component| {
            let label = component.label().to_ascii_lowercase();
            ["package", "cpu", "tctl", "tdie", "coretemp"]
                .iter()
                .any(|needle| label.contains(needle))
        })
        .filter_map(|component| component.temperature())
        .filter(|value| value.is_finite())
        .max_by(f32::total_cmp);
    preferred.or_else(|| {
        components
            .iter()
            .filter_map(|component| component.temperature())
            .filter(|value| value.is_finite())
            .max_by(f32::total_cmp)
    })
}

fn collect_drm_gpus(sys_root: &Path) -> Vec<GpuMetric> {
    let drm_root = sys_root.join("class/drm");
    let Ok(entries) = fs::read_dir(drm_root) else {
        return Vec::new();
    };
    let mut result = entries
        .flatten()
        .filter(|entry| is_card_device(&entry.file_name()))
        .filter_map(|entry| {
            let device = entry.path().join("device");
            if !device.exists() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let driver = fs::read_link(device.join("driver")).ok().and_then(|path| {
                path.file_name()
                    .map(|part| part.to_string_lossy().into_owned())
            });
            // NVIDIA is collected through NVML below. Its DRM sysfs nodes do
            // not expose the utilization/VRAM contract and would duplicate it.
            if driver.as_deref() == Some("nvidia") {
                return None;
            }
            Some(GpuMetric {
                device: name,
                driver,
                usage_percent: read_number(device.join("gpu_busy_percent"))
                    .map(|value| value as f32),
                temperature_celsius: read_gpu_temperature(&device),
                vram_total_bytes: read_number(device.join("mem_info_vram_total")),
                vram_used_bytes: read_number(device.join("mem_info_vram_used")),
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.device.cmp(&right.device));
    result
}

fn has_drm_driver(sys_root: &Path, expected: &str) -> bool {
    let Ok(entries) = fs::read_dir(sys_root.join("class/drm")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        is_card_device(&entry.file_name())
            && fs::read_link(entry.path().join("device/driver"))
                .ok()
                .and_then(|path| path.file_name().map(|name| name == expected))
                .unwrap_or(false)
    })
}

fn collect_nvidia_gpus(nvml: &Nvml) -> Vec<GpuMetric> {
    let Ok(count) = nvml.device_count() else {
        return Vec::new();
    };
    (0..count)
        .filter_map(|index| {
            let device = nvml.device_by_index(index).ok()?;
            let name = device.name().ok();
            let utilization = device.utilization_rates().ok();
            let memory = device.memory_info().ok();
            Some(GpuMetric {
                device: format!("nvidia{index}"),
                driver: Some(match name {
                    Some(name) => format!("nvidia ({name})"),
                    None => "nvidia".into(),
                }),
                usage_percent: utilization.map(|value| value.gpu as f32),
                temperature_celsius: device
                    .temperature(TemperatureSensor::Gpu)
                    .ok()
                    .map(|value| value as f32),
                vram_total_bytes: memory.as_ref().map(|value| value.total),
                vram_used_bytes: memory.as_ref().map(|value| value.used),
            })
        })
        .collect()
}

fn is_card_device(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.strip_prefix("card").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
    })
}

fn read_gpu_temperature(device: &Path) -> Option<f32> {
    let entries = fs::read_dir(device.join("hwmon")).ok()?;
    entries
        .flatten()
        .find_map(|entry| read_number(entry.path().join("temp1_input")))
        .map(|millidegrees| millidegrees as f32 / 1000.0)
}

fn read_number(path: PathBuf) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[derive(Default)]
struct PowerCollector {
    previous: HashMap<PathBuf, (u64, u64)>,
    previous_at: Option<Instant>,
}

impl PowerCollector {
    fn read_watts(&mut self, sys_root: &Path) -> Option<f64> {
        let root = sys_root.join("class/powercap");
        let entries = fs::read_dir(root).ok()?;
        let mut current = HashMap::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("intel-rapl:") || name.starts_with("amd-rapl:"))
                || name.matches(':').count() != 1
            {
                continue;
            }
            let path = entry.path();
            let Some(energy) = read_number(path.join("energy_uj")) else {
                continue;
            };
            let max = read_number(path.join("max_energy_range_uj")).unwrap_or(u64::MAX);
            current.insert(path, (energy, max));
        }

        let now = Instant::now();
        let elapsed = self.previous_at.map(|then| now.duration_since(then));
        let mut delta_microjoules = 0_u64;
        let mut comparable = 0_usize;
        for (path, (energy, max)) in &current {
            if let Some((old_energy, _)) = self.previous.get(path) {
                let delta = if energy >= old_energy {
                    energy - old_energy
                } else {
                    max.saturating_sub(*old_energy).saturating_add(*energy)
                };
                delta_microjoules = delta_microjoules.saturating_add(delta);
                comparable += 1;
            }
        }
        self.previous = current;
        self.previous_at = Some(now);

        match (elapsed, comparable) {
            (Some(duration), count) if count > 0 && !duration.is_zero() => {
                Some(delta_microjoules as f64 / 1_000_000.0 / duration.as_secs_f64())
            }
            _ => None,
        }
    }
}

fn unix_time_ms() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?;
    Ok(duration.as_millis().try_into().unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{distro_identity, gpu_type_names};
    use gauges_shared::GpuMetric;
    use tempfile::TempDir;

    fn gpu(device: &str, driver: Option<&str>) -> GpuMetric {
        GpuMetric {
            device: device.to_owned(),
            driver: driver.map(str::to_owned),
            usage_percent: None,
            temperature_celsius: None,
            vram_total_bytes: None,
            vram_used_bytes: None,
        }
    }

    #[test]
    fn handshake_gpu_types_are_readable_stable_and_deduplicated() {
        let gpus = vec![
            gpu("card0", Some("amdgpu")),
            gpu("card1", Some("amdgpu")),
            gpu("nvidia0", Some("nvidia (NVIDIA GeForce RTX 4090)")),
            gpu("card2", Some("i915")),
        ];

        assert_eq!(
            gpu_type_names(&gpus),
            vec!["AMD (amdgpu)", "Intel (i915)", "NVIDIA GeForce RTX 4090",],
        );
    }

    #[test]
    fn os_release_identifies_openwrt_without_os_info_support() {
        let directory = TempDir::new().unwrap();
        fs::write(
            directory.path().join("os-release"),
            "NAME=\"OpenWrt\"\nVERSION=\"25.12.5\"\nID=openwrt\n",
        )
        .unwrap();

        assert_eq!(
            distro_identity(directory.path()),
            ("OpenWrt".to_owned(), "25.12.5".to_owned()),
        );
    }
}
