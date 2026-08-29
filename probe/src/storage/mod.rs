mod block;
mod btrfs;
mod mounts;
mod ubi;

use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use gauges_shared::{FilesystemConstraintMetric, StorageCommon, StorageMetric};
use sysinfo::Disks;
use tracing::warn;

use crate::config::ProbeConfig;

use self::mounts::{MountInfo, load_mountinfo, select_mount};

pub(crate) struct StorageCollector {
    local_filesystems_only: bool,
    include: HashSet<PathBuf>,
    exclude: HashSet<PathBuf>,
    sys_root: PathBuf,
    mountinfo_path: PathBuf,
    mountinfo_warning_reported: bool,
}

impl StorageCollector {
    pub(crate) fn new(config: &ProbeConfig) -> Self {
        Self {
            local_filesystems_only: config.local_filesystems_only,
            include: config.disk_include.iter().map(PathBuf::from).collect(),
            exclude: config.disk_exclude.iter().map(PathBuf::from).collect(),
            sys_root: config.sys_root.clone(),
            mountinfo_path: config.proc_root.join("self/mountinfo"),
            mountinfo_warning_reported: false,
        }
    }

    pub(crate) fn collect(&mut self) -> Vec<StorageMetric> {
        let mounts = load_mountinfo(&self.mountinfo_path);
        if let Err(error) = &mounts
            && self.local_filesystems_only
            && !self.mountinfo_warning_reported
        {
            warn!(
                path = %self.mountinfo_path.display(),
                %error,
                "cannot classify local storage; only explicit includes will be reported"
            );
            self.mountinfo_warning_reported = true;
        }

        let snapshots = Disks::new_with_refreshed_list()
            .iter()
            .map(|disk| FilesystemSnapshot {
                device: disk.name().to_string_lossy().into_owned(),
                mount_point: disk.mount_point().to_path_buf(),
                file_system: disk.file_system().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                free_bytes: filesystem_free_bytes(disk.mount_point())
                    .unwrap_or_else(|| disk.available_space()),
                mount: mounts
                    .as_deref()
                    .ok()
                    .and_then(|mounts| select_mount(disk, mounts))
                    .cloned(),
            })
            .filter(|snapshot| self.should_collect(snapshot))
            .collect::<Vec<_>>();

        collect_storage(&snapshots, &self.sys_root)
    }

    fn should_collect(&self, snapshot: &FilesystemSnapshot) -> bool {
        let explicitly_included = self.include.contains(&snapshot.mount_point);
        if snapshot.total_bytes == 0
            || self.exclude.contains(&snapshot.mount_point)
            || (is_boot_mount(&snapshot.mount_point) && !explicitly_included)
        {
            return false;
        }
        if !self.local_filesystems_only || explicitly_included {
            return true;
        }
        snapshot
            .mount
            .as_ref()
            .is_some_and(|mount| mounts::is_proven_local(mount, &self.sys_root))
    }
}

#[derive(Clone, Debug)]
struct FilesystemSnapshot {
    device: String,
    mount_point: PathBuf,
    file_system: String,
    total_bytes: u64,
    free_bytes: u64,
    mount: Option<MountInfo>,
}

impl FilesystemSnapshot {
    fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }

    fn usage_percent(&self) -> f32 {
        percent(self.used_bytes(), self.total_bytes)
    }

    fn constraint(&self) -> FilesystemConstraintMetric {
        FilesystemConstraintMetric {
            mount_point: self.mount_point.to_string_lossy().into_owned(),
            file_system: self.file_system.clone(),
            total_bytes: self.total_bytes,
            used_bytes: self.used_bytes(),
            usage_percent: self.usage_percent(),
        }
    }

    fn dedupe_key(&self) -> FilesystemKey {
        self.mount
            .as_ref()
            .map(|mount| FilesystemKey::DeviceNumber(mount.major, mount.minor))
            .unwrap_or_else(|| FilesystemKey::Snapshot(self.device.clone(), self.total_bytes))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FilesystemKey {
    DeviceNumber(u32, u32),
    Snapshot(String, u64),
}

fn collect_storage(snapshots: &[FilesystemSnapshot], sys_root: &Path) -> Vec<StorageMetric> {
    let mut btrfs_groups = BTreeMap::<String, Vec<&FilesystemSnapshot>>::new();
    let mut ubi_groups = BTreeMap::<String, Vec<&FilesystemSnapshot>>::new();
    let mut disk_groups = BTreeMap::<String, Vec<&FilesystemSnapshot>>::new();
    let mut filesystem_fallbacks = Vec::<&FilesystemSnapshot>::new();

    for snapshot in snapshots {
        if snapshot.file_system.eq_ignore_ascii_case("btrfs")
            && let Some(uuid) = btrfs::filesystem_uuid(snapshot, sys_root)
        {
            btrfs_groups.entry(uuid).or_default().push(snapshot);
            continue;
        }
        if snapshot.file_system.eq_ignore_ascii_case("ubifs")
            && let Some(volume) = ubi::volume_name(snapshot, sys_root)
        {
            ubi_groups.entry(volume).or_default().push(snapshot);
            continue;
        }
        if let Some(owner) = block::storage_owner(snapshot, sys_root) {
            disk_groups.entry(owner).or_default().push(snapshot);
        } else {
            filesystem_fallbacks.push(snapshot);
        }
    }

    let mut storage = Vec::with_capacity(
        btrfs_groups.len() + ubi_groups.len() + disk_groups.len() + filesystem_fallbacks.len(),
    );
    storage.extend(
        btrfs_groups
            .into_iter()
            .map(|(uuid, snapshots)| btrfs::collect(uuid, &snapshots, sys_root)),
    );
    storage.extend(
        ubi_groups
            .into_iter()
            .map(|(volume, snapshots)| ubi::collect(volume, &snapshots, sys_root)),
    );
    storage.extend(
        disk_groups
            .into_iter()
            .map(|(device, snapshots)| collect_disk(device, &snapshots, sys_root)),
    );
    storage.extend(filesystem_fallbacks.into_iter().map(collect_filesystem));
    storage.sort_by(|left, right| left.common().label.cmp(&right.common().label));
    storage
}

fn collect_filesystem(snapshot: &FilesystemSnapshot) -> StorageMetric {
    let snapshots = [snapshot];
    StorageMetric::Filesystem {
        common: common_metric(
            format!(
                "filesystem:{}:{}",
                snapshot.device,
                snapshot.mount_point.display()
            ),
            snapshot.mount_point.to_string_lossy().into_owned(),
            &snapshots,
            snapshot.total_bytes,
            snapshot.used_bytes(),
        ),
        source: snapshot.device.clone(),
        file_system: snapshot.file_system.clone(),
    }
}

fn collect_disk(
    device: String,
    snapshots: &[&FilesystemSnapshot],
    sys_root: &Path,
) -> StorageMetric {
    let unique = unique_filesystems(snapshots);
    let fallback_total = unique.iter().map(|snapshot| snapshot.total_bytes).sum();
    let total_bytes = block::device_size_bytes(&device, sys_root).unwrap_or(fallback_total);
    let used_bytes = unique
        .iter()
        .map(|snapshot| snapshot.used_bytes())
        .sum::<u64>()
        .min(total_bytes);
    let mut file_systems = snapshots
        .iter()
        .map(|snapshot| snapshot.file_system.clone())
        .collect::<Vec<_>>();
    file_systems.sort();
    file_systems.dedup();

    StorageMetric::Disk {
        common: common_metric(
            format!("disk:{device}"),
            device.clone(),
            snapshots,
            total_bytes,
            used_bytes,
        ),
        device,
        file_systems,
    }
}

fn common_metric(
    id: String,
    label: String,
    snapshots: &[&FilesystemSnapshot],
    total_bytes: u64,
    used_bytes: u64,
) -> StorageCommon {
    let mut mount_points = snapshots
        .iter()
        .map(|snapshot| snapshot.mount_point.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    mount_points.sort();
    mount_points.dedup();
    let fullest_filesystem = snapshots
        .iter()
        .max_by(|left, right| {
            left.usage_percent()
                .partial_cmp(&right.usage_percent())
                .unwrap_or(Ordering::Equal)
        })
        .map(|snapshot| snapshot.constraint());

    StorageCommon {
        id,
        label,
        mount_points,
        total_bytes,
        used_bytes,
        usage_percent: percent(used_bytes, total_bytes),
        fullest_filesystem,
    }
}

fn unique_filesystems<'a>(snapshots: &'a [&'a FilesystemSnapshot]) -> Vec<&'a FilesystemSnapshot> {
    let mut unique = HashMap::<FilesystemKey, &FilesystemSnapshot>::new();
    for snapshot in snapshots {
        unique.entry(snapshot.dedupe_key()).or_insert(snapshot);
    }
    unique.into_values().collect()
}

fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32
    }
}

fn filesystem_free_bytes(path: &Path) -> Option<u64> {
    let statistics = rustix::fs::statvfs(path).ok()?;
    let block_size = if statistics.f_frsize > 0 {
        statistics.f_frsize
    } else {
        statistics.f_bsize
    };
    statistics.f_bfree.checked_mul(block_size)
}

fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_bool(path: impl AsRef<Path>) -> Option<bool> {
    match read_u64(path)? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn is_boot_mount(path: &Path) -> bool {
    path == Path::new("/boot") || path.starts_with("/boot/")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn groups_partitions_by_whole_disk_and_keeps_fullest_filesystem() {
        let fixture = Fixture::new();
        fixture.partition("nvme0n1", "nvme0n1p2", 2_000);
        fixture.partition("nvme0n1", "nvme0n1p3", 2_000);
        let snapshots = vec![
            fixture.snapshot("/dev/nvme0n1p2", "/", "ext4", 1_200, 600, (259, 2)),
            fixture.snapshot("/dev/nvme0n1p3", "/var", "xfs", 700, 70, (259, 3)),
        ];

        let result = collect_storage(&snapshots, fixture.path());

        assert_eq!(result.len(), 1);
        let StorageMetric::Disk {
            common,
            device,
            file_systems,
        } = &result[0]
        else {
            panic!("expected disk metric");
        };
        assert_eq!(device, "nvme0n1");
        assert_eq!(common.total_bytes, 2_000 * 512);
        assert_eq!(common.used_bytes, 1_230);
        assert_eq!(common.mount_points, ["/", "/var"]);
        assert_eq!(
            common.fullest_filesystem.as_ref().unwrap().mount_point,
            "/var"
        );
        assert_eq!(file_systems, &["ext4", "xfs"]);
    }

    #[test]
    fn builds_btrfs_pool_from_uuid_members_and_profiles() {
        let fixture = Fixture::new();
        fixture.block("sdb", 1_000);
        fixture.block("sdc", 1_000);
        fixture.btrfs("pool-uuid", "data", &["sdb", "sdc"], "raid1");
        let snapshots = vec![
            fixture.snapshot("/dev/sdb", "/mnt/data", "btrfs", 1_000, 400, (0, 120)),
            fixture.snapshot("/dev/sdb", "/mnt/data/home", "btrfs", 1_000, 400, (0, 120)),
        ];

        let result = collect_storage(&snapshots, fixture.path());

        let StorageMetric::Btrfs {
            common,
            devices,
            data_profiles,
            physical_bytes,
            ..
        } = &result[0]
        else {
            panic!("expected btrfs metric");
        };
        assert_eq!(common.label, "data");
        assert_eq!(common.total_bytes, 1_000);
        assert_eq!(common.used_bytes, 600);
        assert_eq!(devices, &["sdb", "sdc"]);
        assert_eq!(data_profiles, &["raid1"]);
        assert_eq!(*physical_bytes, 2_000 * 512);
    }

    #[test]
    fn btrfs_root_uses_whole_backing_disk_and_keeps_logical_capacity() {
        let fixture = Fixture::new();
        fixture.partition("nvme0n1", "nvme0n1p2", 2_000);
        fixture.btrfs("root-uuid", "", &["nvme0n1p2"], "single");
        let snapshots = vec![fixture.snapshot("/dev/nvme0n1p2", "/", "btrfs", 900, 600, (0, 33))];

        let result = collect_storage(&snapshots, fixture.path());

        let StorageMetric::Btrfs {
            common,
            logical_bytes,
            ..
        } = &result[0]
        else {
            panic!("expected btrfs metric");
        };
        assert_eq!(common.label, "nvme0n1");
        assert_eq!(common.total_bytes, 2_000 * 512);
        assert_eq!(*logical_bytes, 900);
        assert_eq!(common.fullest_filesystem.as_ref().unwrap().total_bytes, 900);
    }

    #[test]
    fn builds_ubi_health_from_sysfs_without_external_commands() {
        let fixture = Fixture::new();
        fixture.ubi();
        let snapshots =
            vec![fixture.snapshot("ubi0:rootfs_data", "/overlay", "ubifs", 100, 20, (0, 20))];

        let result = collect_storage(&snapshots, fixture.path());

        let StorageMetric::Ubi {
            common,
            volume,
            bad_pebs,
            max_erase_count,
            mtd_health,
            ..
        } = &result[0]
        else {
            panic!("expected UBI metric");
        };
        assert_eq!(common.label, "rootfs_data");
        assert_eq!(volume, "ubi0_5");
        assert_eq!(*bad_pebs, Some(2));
        assert_eq!(*max_erase_count, Some(41));
        assert_eq!(mtd_health.as_ref().unwrap().corrected_bits, Some(148));
    }

    struct Fixture {
        directory: TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                directory: tempfile::tempdir().unwrap(),
            }
        }

        fn path(&self) -> &Path {
            self.directory.path()
        }

        fn block(&self, name: &str, sectors: u64) {
            let path = self.path().join("class/block").join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("size"), sectors.to_string()).unwrap();
        }

        fn partition(&self, parent: &str, name: &str, parent_sectors: u64) {
            self.block(parent, parent_sectors);
            let path = self.path().join("class/block").join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("partition"), "1").unwrap();
        }

        fn btrfs(&self, uuid: &str, label: &str, devices: &[&str], profile: &str) {
            let root = self.path().join("fs/btrfs").join(uuid);
            fs::create_dir_all(root.join("allocation/data").join(profile)).unwrap();
            fs::create_dir_all(root.join("allocation/metadata").join(profile)).unwrap();
            fs::write(root.join("label"), label).unwrap();
            for device in devices {
                fs::create_dir_all(root.join("devices").join(device)).unwrap();
            }
        }

        fn ubi(&self) {
            let ubi = self.path().join("class/ubi/ubi0");
            let volume = self.path().join("class/ubi/ubi0_5");
            let mtd = self.path().join("class/mtd/mtd5");
            fs::create_dir_all(&ubi).unwrap();
            fs::create_dir_all(&volume).unwrap();
            fs::create_dir_all(&mtd).unwrap();
            for (name, value) in [
                ("mtd_num", "5"),
                ("total_eraseblocks", "2040"),
                ("avail_eraseblocks", "3"),
                ("bad_peb_count", "2"),
                ("reserved_for_bad", "40"),
                ("max_ec", "41"),
                ("ro_mode", "0"),
            ] {
                fs::write(ubi.join(name), value).unwrap();
            }
            fs::write(volume.join("name"), "rootfs_data").unwrap();
            fs::write(volume.join("data_bytes"), "198590464").unwrap();
            fs::write(volume.join("corrupted"), "0").unwrap();
            fs::write(mtd.join("corrected_bits"), "148").unwrap();
            fs::write(mtd.join("ecc_failures"), "0").unwrap();
        }

        fn snapshot(
            &self,
            device: &str,
            mount_point: &str,
            file_system: &str,
            total_bytes: u64,
            free_bytes: u64,
            device_number: (u32, u32),
        ) -> FilesystemSnapshot {
            FilesystemSnapshot {
                device: device.into(),
                mount_point: mount_point.into(),
                file_system: file_system.into(),
                total_bytes,
                free_bytes,
                mount: Some(MountInfo {
                    mount_id: 1,
                    major: device_number.0,
                    minor: device_number.1,
                    root: "/".into(),
                    mount_point: mount_point.into(),
                    file_system: file_system.into(),
                    source: device.into(),
                }),
            }
        }
    }
}
