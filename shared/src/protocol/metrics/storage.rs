use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveTemperatureMetric {
    pub device: String,
    pub temperature_celsius: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StorageCommon {
    pub id: String,
    pub label: String,
    pub mount_points: Vec<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
    pub fullest_filesystem: Option<FilesystemConstraintMetric>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemConstraintMetric {
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StorageMetric {
    Disk {
        #[serde(flatten)]
        common: StorageCommon,
        device: String,
        file_systems: Vec<String>,
    },
    Filesystem {
        #[serde(flatten)]
        common: StorageCommon,
        source: String,
        file_system: String,
    },
    Btrfs {
        #[serde(flatten)]
        common: StorageCommon,
        uuid: String,
        devices: Vec<String>,
        data_profiles: Vec<String>,
        metadata_profiles: Vec<String>,
        logical_bytes: u64,
        physical_bytes: u64,
        allocated_bytes: Option<u64>,
        allocation_used_bytes: Option<u64>,
        device_errors: Option<BtrfsDeviceErrorMetric>,
        #[serde(default)]
        smart_health: Vec<SmartDeviceHealthMetric>,
    },
    Ubi {
        #[serde(flatten)]
        common: StorageCommon,
        ubi_device: String,
        volume: String,
        mtd_device: Option<String>,
        volume_bytes: Option<u64>,
        total_pebs: Option<u64>,
        available_pebs: Option<u64>,
        bad_pebs: Option<u64>,
        reserved_for_bad_pebs: Option<u64>,
        max_erase_count: Option<u64>,
        read_only: Option<bool>,
        corrupted: Option<bool>,
        mtd_health: Option<MtdHealthMetric>,
    },
}

impl StorageMetric {
    pub fn common(&self) -> &StorageCommon {
        match self {
            Self::Disk { common, .. }
            | Self::Filesystem { common, .. }
            | Self::Btrfs { common, .. }
            | Self::Ubi { common, .. } => common,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BtrfsDeviceErrorMetric {
    pub read: u64,
    pub write: u64,
    pub flush: u64,
    pub corruption: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SmartDeviceHealthMetric {
    pub device: String,
    pub status: SmartHealthStatus,
    pub checked_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SmartHealthStatus {
    Pending,
    Passed,
    Failed,
    Standby,
    Unavailable,
}

impl BtrfsDeviceErrorMetric {
    pub fn total(&self) -> u64 {
        self.read
            .saturating_add(self.write)
            .saturating_add(self.flush)
            .saturating_add(self.corruption)
            .saturating_add(self.generation)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MtdHealthMetric {
    pub corrected_bits: Option<u64>,
    pub ecc_failures: Option<u64>,
    pub bad_blocks: Option<u64>,
    pub reserved_bad_blocks: Option<u64>,
    pub bitflip_threshold: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_is_a_flat_discriminated_union() {
        let metric = StorageMetric::Disk {
            common: StorageCommon {
                id: "disk:sda".into(),
                label: "sda".into(),
                mount_points: vec!["/".into()],
                total_bytes: 1_000,
                used_bytes: 200,
                usage_percent: 20.0,
                fullest_filesystem: None,
            },
            device: "sda".into(),
            file_systems: vec!["ext4".into()],
        };

        let json = serde_json::to_value(&metric).unwrap();
        assert_eq!(json["kind"], "disk");
        assert_eq!(json["id"], "disk:sda");
        assert_eq!(json["mountPoints"][0], "/");
        assert_eq!(json["fileSystems"][0], "ext4");
        assert_eq!(
            serde_json::from_value::<StorageMetric>(json).unwrap(),
            metric
        );
    }
}
