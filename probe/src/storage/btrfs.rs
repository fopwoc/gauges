use std::{fs, path::Path};

use gauges_shared::{BtrfsDeviceErrorMetric, StorageMetric};

use super::{FilesystemSnapshot, block, common_metric};

pub(super) fn filesystem_uuid(snapshot: &FilesystemSnapshot, sys_root: &Path) -> Option<String> {
    let device = block::block_device_name(snapshot, sys_root)?;
    fs::read_dir(sys_root.join("fs/btrfs"))
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| {
            entry
                .path()
                .join("devices")
                .join(&device)
                .exists()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
}

pub(super) fn collect(
    uuid: String,
    snapshots: &[&FilesystemSnapshot],
    sys_root: &Path,
) -> StorageMetric {
    let root = sys_root.join("fs/btrfs").join(&uuid);
    let mut devices = directory_names(&root.join("devices"));
    let mut physical_bytes = devices
        .iter()
        .filter_map(|device| block::device_size_bytes(device, sys_root))
        .sum();
    let logical_bytes = snapshots
        .iter()
        .map(|snapshot| snapshot.total_bytes)
        .max()
        .unwrap_or(0);
    let total_bytes = root_backing_device_bytes(snapshots, sys_root).unwrap_or(logical_bytes);
    let used_bytes = snapshots
        .iter()
        .map(|snapshot| snapshot.used_bytes())
        .max()
        .unwrap_or(0);
    let mut label = fs::read_to_string(root.join("label"))
        .ok()
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty());
    let ioctl = snapshots
        .first()
        .and_then(|snapshot| query_ioctl(&snapshot.mount_point));

    if let Some(ioctl) = &ioctl {
        if physical_bytes == 0 {
            physical_bytes = ioctl.physical_bytes;
        }
        if devices.is_empty() {
            devices = ioctl.devices.clone();
        }
        if label.is_none() {
            label.clone_from(&ioctl.label);
        }
    }
    let label = label.unwrap_or_else(|| fallback_label(snapshots, sys_root, &uuid));

    StorageMetric::Btrfs {
        common: common_metric(
            format!("btrfs:{uuid}"),
            label,
            snapshots,
            total_bytes,
            used_bytes,
        ),
        uuid,
        devices,
        data_profiles: allocation_profiles(&root, "data"),
        metadata_profiles: allocation_profiles(&root, "metadata"),
        logical_bytes,
        physical_bytes,
        allocated_bytes: ioctl.as_ref().map(|metrics| metrics.allocated_bytes),
        allocation_used_bytes: ioctl.as_ref().map(|metrics| metrics.used_bytes),
        device_errors: ioctl.and_then(|metrics| metrics.device_errors),
    }
}

fn root_backing_device_bytes(snapshots: &[&FilesystemSnapshot], sys_root: &Path) -> Option<u64> {
    let root = snapshots
        .iter()
        .find(|snapshot| snapshot.mount_point == Path::new("/"))?;
    let owner = block::storage_owner(root, sys_root)?;
    block::device_size_bytes(&owner, sys_root)
}

fn fallback_label(snapshots: &[&FilesystemSnapshot], sys_root: &Path, uuid: &str) -> String {
    if let Some(root) = snapshots
        .iter()
        .find(|snapshot| snapshot.mount_point == Path::new("/"))
    {
        return block::storage_owner(root, sys_root).unwrap_or_else(|| "root".into());
    }
    if snapshots.len() == 1
        && let Some(name) = snapshots[0].mount_point.file_name()
    {
        return name.to_string_lossy().into_owned();
    }
    format!("Btrfs {}", &uuid[..uuid.len().min(8)])
}

fn allocation_profiles(root: &Path, allocation: &str) -> Vec<String> {
    directory_names(&root.join("allocation").join(allocation))
}

fn directory_names(path: &Path) -> Vec<String> {
    let mut names = fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() || kind.is_symlink())
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

struct BtrfsIoctlMetric {
    label: Option<String>,
    devices: Vec<String>,
    physical_bytes: u64,
    allocated_bytes: u64,
    used_bytes: u64,
    device_errors: Option<BtrfsDeviceErrorMetric>,
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn query_ioctl(mount_point: &Path) -> Option<BtrfsIoctlMetric> {
    use std::{fs::File, os::fd::AsFd};

    use btrfs_uapi::{device, filesystem, space};

    let file = File::open(mount_point).ok()?;
    let fd = file.as_fd();
    let filesystem = filesystem::filesystem_info(fd).ok()?;
    let spaces = space::space_info(fd).ok()?;
    let device_info = device::device_info_all(fd, &filesystem).ok();
    let mut device_errors = BtrfsDeviceErrorMetric::default();
    let mut has_device_stats = false;
    if let Some(devices) = &device_info {
        for info in devices {
            if let Ok(stats) = device::device_stats(fd, info.devid, false) {
                has_device_stats = true;
                device_errors.read = device_errors.read.saturating_add(stats.read_errs);
                device_errors.write = device_errors.write.saturating_add(stats.write_errs);
                device_errors.flush = device_errors.flush.saturating_add(stats.flush_errs);
                device_errors.corruption = device_errors
                    .corruption
                    .saturating_add(stats.corruption_errs);
                device_errors.generation = device_errors
                    .generation
                    .saturating_add(stats.generation_errs);
            }
        }
    }

    Some(BtrfsIoctlMetric {
        label: filesystem::label_get(fd)
            .ok()
            .and_then(|label| label.into_string().ok())
            .filter(|label| !label.is_empty()),
        devices: device_info
            .as_ref()
            .into_iter()
            .flatten()
            .filter_map(|device| {
                Path::new(&device.path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect(),
        physical_bytes: device_info
            .as_ref()
            .into_iter()
            .flatten()
            .map(|device| device.total_bytes)
            .sum(),
        allocated_bytes: spaces.iter().map(|space| space.total_bytes).sum(),
        used_bytes: spaces.iter().map(|space| space.used_bytes).sum(),
        device_errors: has_device_stats.then_some(device_errors),
    })
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn query_ioctl(_mount_point: &Path) -> Option<BtrfsIoctlMetric> {
    None
}
