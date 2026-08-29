use std::{fs, path::Path};

use gauges_shared::{MtdHealthMetric, StorageMetric};

use super::{FilesystemSnapshot, common_metric, read_bool, read_u64};

pub(super) fn volume_name(snapshot: &FilesystemSnapshot, sys_root: &Path) -> Option<String> {
    let source_name = Path::new(&snapshot.device)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    if let Some(name) = source_name
        && is_ubi_volume_name(&name)
        && sys_root.join("class/ubi").join(&name).exists()
    {
        return Some(name);
    }

    let (ubi_device, label) = snapshot.device.split_once(':')?;
    fs::read_dir(sys_root.join("class/ubi"))
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (is_ubi_volume_name(&name)
                && name.starts_with(&format!("{ubi_device}_"))
                && fs::read_to_string(entry.path().join("name"))
                    .is_ok_and(|value| value.trim() == label))
            .then_some(name)
        })
}

pub(super) fn collect(
    volume: String,
    snapshots: &[&FilesystemSnapshot],
    sys_root: &Path,
) -> StorageMetric {
    let ubi_device = volume
        .split_once('_')
        .map_or(volume.as_str(), |(ubi, _)| ubi);
    let ubi_root = sys_root.join("class/ubi").join(ubi_device);
    let volume_root = sys_root.join("class/ubi").join(&volume);
    let label = fs::read_to_string(volume_root.join("name"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| volume.clone());
    let total_bytes = snapshots
        .iter()
        .map(|snapshot| snapshot.total_bytes)
        .max()
        .unwrap_or(0);
    let used_bytes = snapshots
        .iter()
        .map(|snapshot| snapshot.used_bytes())
        .max()
        .unwrap_or(0);
    let mtd_device = read_u64(ubi_root.join("mtd_num")).map(|number| format!("mtd{number}"));
    let mtd_health = mtd_device.as_ref().map(|device| {
        let root = sys_root.join("class/mtd").join(device);
        MtdHealthMetric {
            corrected_bits: read_u64(root.join("corrected_bits")),
            ecc_failures: read_u64(root.join("ecc_failures")),
            bad_blocks: read_u64(root.join("bad_blocks")),
            reserved_bad_blocks: read_u64(root.join("bbt_blocks")),
            bitflip_threshold: read_u64(root.join("bitflip_threshold")),
        }
    });

    StorageMetric::Ubi {
        common: common_metric(
            format!("ubi:{volume}"),
            label,
            snapshots,
            total_bytes,
            used_bytes,
        ),
        ubi_device: ubi_device.to_owned(),
        volume,
        mtd_device,
        volume_bytes: read_u64(volume_root.join("data_bytes")),
        total_pebs: read_u64(ubi_root.join("total_eraseblocks")),
        available_pebs: read_u64(ubi_root.join("avail_eraseblocks")),
        bad_pebs: read_u64(ubi_root.join("bad_peb_count")),
        reserved_for_bad_pebs: read_u64(ubi_root.join("reserved_for_bad")),
        max_erase_count: read_u64(ubi_root.join("max_ec")),
        read_only: read_bool(ubi_root.join("ro_mode")),
        corrupted: read_bool(volume_root.join("corrupted")),
        mtd_health,
    }
}

fn is_ubi_volume_name(name: &str) -> bool {
    let Some((device, volume)) = name.split_once('_') else {
        return false;
    };
    device.strip_prefix("ubi").is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    }) && !volume.is_empty()
        && volume.bytes().all(|byte| byte.is_ascii_digit())
}
