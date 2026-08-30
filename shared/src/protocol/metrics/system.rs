use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpuMetric {
    pub usage_percent: f32,
    pub temperature_celsius: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetric {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMetric {
    pub interface: String,
    pub received_bytes_per_second: f64,
    pub transmitted_bytes_per_second: f64,
    pub total_received_bytes: u64,
    pub total_transmitted_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetric {
    pub device: String,
    pub driver: Option<String>,
    pub usage_percent: Option<f32>,
    pub temperature_celsius: Option<f32>,
    pub vram_total_bytes: Option<u64>,
    pub vram_used_bytes: Option<u64>,
}
