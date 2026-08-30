use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use gauges_hub::{ConfiguredDevice, HubConfig};
use gauges_shared::{
    CpuMetric, FilesystemConstraintMetric, MemoryMetric, MetricSample, NetworkMetric,
    ProbeIdentity, StorageCommon, StorageMetric,
};

#[allow(dead_code)]
pub fn test_config(database_path: PathBuf, console_path: PathBuf) -> HubConfig {
    HubConfig {
        host: "127.0.0.1".into(),
        port: 8_080,
        database_path,
        console_path,
        devices: BTreeMap::from([(
            "server-a".into(),
            ConfiguredDevice {
                id: "server-a".into(),
                display_name: "Server A".into(),
                token: "secret".into(),
            },
        )]),
        cleanup_interval: Duration::from_secs(3_600),
        online_threshold: Duration::from_secs(30),
        max_ingest_batch_size: 10,
    }
}

pub fn test_identity() -> ProbeIdentity {
    ProbeIdentity {
        hostname: "server-a".into(),
        distro: "Alpine Linux".into(),
        distro_version: "3.22".into(),
        kernel_version: "6.12.0".into(),
        ip_address: Some("192.0.2.1".into()),
        cpu_model: Some("AMD Ryzen 7 5700X 8-Core Processor".into()),
        gpu_types: vec!["AMD (amdgpu)".into(), "NVIDIA GeForce RTX 4090".into()],
    }
}

pub fn test_sample(id: &str, captured_at_ms: i64) -> MetricSample {
    MetricSample {
        sample_id: id.into(),
        captured_at_ms,
        cpu: CpuMetric {
            usage_percent: 25.0,
            temperature_celsius: Some(50.0),
        },
        memory: MemoryMetric {
            total_bytes: 1_000,
            used_bytes: 400,
            available_bytes: 600,
            swap_total_bytes: 100,
            swap_used_bytes: 10,
        },
        storage: vec![StorageMetric::Disk {
            common: StorageCommon {
                id: "disk:sda".into(),
                label: "sda".into(),
                mount_points: vec!["/".into()],
                total_bytes: 10_000,
                used_bytes: 2_000,
                usage_percent: 20.0,
                fullest_filesystem: Some(FilesystemConstraintMetric {
                    mount_point: "/".into(),
                    file_system: "ext4".into(),
                    total_bytes: 10_000,
                    used_bytes: 2_000,
                    usage_percent: 20.0,
                }),
            },
            device: "sda".into(),
            file_systems: vec!["ext4".into()],
        }],
        networks: vec![NetworkMetric {
            interface: "eth0".into(),
            received_bytes_per_second: 1_024.0,
            transmitted_bytes_per_second: 512.0,
            total_received_bytes: 20_000,
            total_transmitted_bytes: 10_000,
        }],
        gpus: Vec::new(),
        power_watts: Some(12.5),
        uptime_seconds: 60,
    }
}
