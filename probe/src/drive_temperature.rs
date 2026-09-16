use std::{collections::BTreeMap, fs, path::Path};

use gauges_shared::DriveTemperatureMetric;

pub(crate) fn collect(sys_root: &Path) -> Vec<DriveTemperatureMetric> {
    let block_root = sys_root.join("class/block");
    let Ok(entries) = fs::read_dir(&block_root) else {
        return Vec::new();
    };
    let devices = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.join("partition").exists() {
                return None;
            }
            Some((
                entry.file_name().to_string_lossy().into_owned(),
                fs::canonicalize(path).ok()?,
            ))
        })
        .collect::<Vec<_>>();

    let Ok(sensors) = fs::read_dir(sys_root.join("class/hwmon")) else {
        return Vec::new();
    };
    let mut temperatures = BTreeMap::new();
    for sensor in sensors.flatten() {
        let path = sensor.path();
        let Ok(name) = fs::read_to_string(path.join("name")) else {
            continue;
        };
        if !matches!(name.trim(), "drivetemp" | "nvme") {
            continue;
        }
        let Ok(sensor_device) = fs::canonicalize(path.join("device")) else {
            continue;
        };
        let Some(temperature) = fs::read_to_string(path.join("temp1_input"))
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
            .map(|millidegrees| millidegrees as f32 / 1_000.0)
        else {
            continue;
        };
        for (device, block_path) in &devices {
            if block_path.starts_with(&sensor_device) {
                temperatures.insert(device.clone(), temperature);
            }
        }
    }
    temperatures
        .into_iter()
        .map(|(device, temperature_celsius)| DriveTemperatureMetric {
            device,
            temperature_celsius,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn matches_drive_sensors_to_whole_block_devices() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let physical = root.join("devices/controller/disk");
        fs::create_dir_all(physical.join("block/sda/sda1")).unwrap();
        fs::create_dir_all(root.join("class/block")).unwrap();
        fs::create_dir_all(root.join("class/hwmon/hwmon0")).unwrap();
        symlink(physical.join("block/sda"), root.join("class/block/sda")).unwrap();
        symlink(
            physical.join("block/sda/sda1"),
            root.join("class/block/sda1"),
        )
        .unwrap();
        fs::write(physical.join("block/sda/sda1/partition"), "1\n").unwrap();
        symlink(&physical, root.join("class/hwmon/hwmon0/device")).unwrap();
        fs::write(root.join("class/hwmon/hwmon0/name"), "drivetemp\n").unwrap();
        fs::write(root.join("class/hwmon/hwmon0/temp1_input"), "37750\n").unwrap();

        assert_eq!(
            collect(root),
            vec![DriveTemperatureMetric {
                device: "sda".into(),
                temperature_celsius: 37.75,
            }]
        );
    }
}
