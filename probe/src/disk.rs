use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use sysinfo::Disks;
use tracing::warn;

use gauges_shared::DiskMetric;

use crate::config::ProbeConfig;

pub(crate) struct DiskCollector {
    local_filesystems_only: bool,
    include: HashSet<PathBuf>,
    exclude: HashSet<PathBuf>,
    sys_root: PathBuf,
    mountinfo_path: PathBuf,
    mountinfo_warning_reported: bool,
}

impl DiskCollector {
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

    pub(crate) fn collect(&mut self) -> Vec<DiskMetric> {
        let disks = Disks::new_with_refreshed_list();
        let snapshots = disks
            .iter()
            .map(|disk| DiskSnapshot {
                device: disk.name().to_string_lossy().into_owned(),
                mount_point: disk.mount_point().to_path_buf(),
                file_system: disk.file_system().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
            })
            .collect::<Vec<_>>();

        if !self.local_filesystems_only {
            return filter_snapshots(
                &snapshots,
                None,
                false,
                &self.include,
                &self.exclude,
                &self.sys_root,
            );
        }

        let mounts = load_mountinfo(&self.mountinfo_path);
        if let Err(error) = &mounts
            && !self.mountinfo_warning_reported
        {
            warn!(
                path = %self.mountinfo_path.display(),
                %error,
                "cannot classify local filesystems; only explicit disk includes will be reported"
            );
            self.mountinfo_warning_reported = true;
        }

        filter_snapshots(
            &snapshots,
            mounts.as_deref().ok(),
            true,
            &self.include,
            &self.exclude,
            &self.sys_root,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MountInfo {
    mount_id: u64,
    major: u32,
    minor: u32,
    root: PathBuf,
    mount_point: PathBuf,
    file_system: String,
    source: String,
}

#[derive(Clone, Debug)]
struct DiskSnapshot {
    device: String,
    mount_point: PathBuf,
    file_system: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DedupeKey {
    DeviceNumber(u32, u32),
    Snapshot(String, u64),
}

struct Candidate {
    metric: DiskMetric,
    dedupe_key: DedupeKey,
    explicitly_included: bool,
}

fn load_mountinfo(path: &Path) -> Result<Vec<MountInfo>, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("failed to read mountinfo: {error}"))?;
    parse_mountinfo(&contents)
}

fn parse_mountinfo(contents: &str) -> Result<Vec<MountInfo>, String> {
    if contents.is_empty() {
        return Err("mountinfo is empty".into());
    }

    contents
        .lines()
        .enumerate()
        .map(|(index, line)| {
            parse_mountinfo_line(line)
                .map_err(|error| format!("invalid mountinfo line {}: {error}", index + 1))
        })
        .collect()
}

fn parse_mountinfo_line(line: &str) -> Result<MountInfo, String> {
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() < 10 {
        return Err("too few fields".into());
    }
    let separator = fields
        .iter()
        .enumerate()
        .skip(6)
        .find_map(|(index, field)| (*field == "-").then_some(index))
        .ok_or_else(|| "missing optional-field separator".to_string())?;
    if separator + 3 >= fields.len() {
        return Err("missing filesystem fields".into());
    }

    let mount_id = fields[0]
        .parse()
        .map_err(|_| "invalid mount id".to_string())?;
    let (major, minor) = fields[2]
        .split_once(':')
        .ok_or_else(|| "invalid device number".to_string())?;

    Ok(MountInfo {
        mount_id,
        major: major
            .parse()
            .map_err(|_| "invalid device major number".to_string())?,
        minor: minor
            .parse()
            .map_err(|_| "invalid device minor number".to_string())?,
        root: PathBuf::from(decode_mount_field(fields[3])?),
        mount_point: PathBuf::from(decode_mount_field(fields[4])?),
        file_system: decode_mount_field(fields[separator + 1])?,
        source: decode_mount_field(fields[separator + 2])?,
    })
}

fn decode_mount_field(field: &str) -> Result<String, String> {
    let bytes = field.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 3 >= bytes.len()
            || !bytes[index + 1..=index + 3]
                .iter()
                .all(|byte| (b'0'..=b'7').contains(byte))
        {
            return Err("invalid mount field escape".into());
        }
        let value = (bytes[index + 1] - b'0') * 64
            + (bytes[index + 2] - b'0') * 8
            + (bytes[index + 3] - b'0');
        decoded.push(value);
        index += 4;
    }
    String::from_utf8(decoded).map_err(|_| "mount field is not UTF-8".into())
}

fn filter_snapshots(
    snapshots: &[DiskSnapshot],
    mounts: Option<&[MountInfo]>,
    local_filesystems_only: bool,
    include: &HashSet<PathBuf>,
    exclude: &HashSet<PathBuf>,
    sys_root: &Path,
) -> Vec<DiskMetric> {
    let mut selected = HashMap::<DedupeKey, Candidate>::new();
    for snapshot in snapshots {
        if snapshot.total_bytes == 0 || exclude.contains(&snapshot.mount_point) {
            continue;
        }

        let explicitly_included = include.contains(&snapshot.mount_point);
        let mount = mounts.and_then(|mounts| select_mount(snapshot, mounts));
        if local_filesystems_only
            && !explicitly_included
            && !mount.is_some_and(|mount| is_proven_local(mount, sys_root))
        {
            continue;
        }

        let used = snapshot
            .total_bytes
            .saturating_sub(snapshot.available_bytes);
        let metric = DiskMetric {
            device: snapshot.device.clone(),
            mount_point: snapshot.mount_point.to_string_lossy().into_owned(),
            file_system: snapshot.file_system.clone(),
            total_bytes: snapshot.total_bytes,
            used_bytes: used,
            usage_percent: used as f32 / snapshot.total_bytes as f32 * 100.0,
        };
        let dedupe_key = mount
            .map(|mount| DedupeKey::DeviceNumber(mount.major, mount.minor))
            .unwrap_or_else(|| DedupeKey::Snapshot(snapshot.device.clone(), snapshot.total_bytes));
        let candidate = Candidate {
            metric,
            dedupe_key,
            explicitly_included,
        };
        match selected.get(&candidate.dedupe_key) {
            Some(current) if !candidate_preferred(&candidate, current) => {}
            _ => {
                selected.insert(candidate.dedupe_key.clone(), candidate);
            }
        }
    }

    let mut result = selected
        .into_values()
        .map(|candidate| candidate.metric)
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
    result
}

fn candidate_preferred(candidate: &Candidate, current: &Candidate) -> bool {
    match candidate
        .explicitly_included
        .cmp(&current.explicitly_included)
    {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => preferred_over(&candidate.metric, &current.metric),
    }
}

fn select_mount<'a>(snapshot: &DiskSnapshot, mounts: &'a [MountInfo]) -> Option<&'a MountInfo> {
    // sysinfo decodes the mountpoint from /proc/mounts but leaves fs_spec
    // escaped, while mountinfo applies the same octal escaping to its source.
    let source = decode_mount_field(&snapshot.device).unwrap_or_else(|_| snapshot.device.clone());
    mounts
        .iter()
        .filter(|mount| {
            mount.mount_point == snapshot.mount_point
                && mount.source == source
                && mount.file_system == snapshot.file_system
        })
        .max_by_key(|mount| mount.mount_id)
}

fn is_proven_local(mount: &MountInfo, sys_root: &Path) -> bool {
    if mount.root != Path::new("/") && mount.mount_point != Path::new("/") {
        return false;
    }
    if is_disallowed_device_name(&mount.source) || mount.major == 1 || mount.major == 7 {
        return false;
    }

    let filesystem = mount.file_system.to_ascii_lowercase();
    if matches!(filesystem.as_str(), "ubifs" | "jffs2") {
        return true;
    }
    if is_always_virtual_filesystem(&filesystem) {
        return false;
    }

    let block_path = sys_root
        .join("dev/block")
        .join(format!("{}:{}", mount.major, mount.minor));
    if !block_path.exists() {
        return false;
    }
    fs::canonicalize(&block_path)
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .is_some_and(|name| !is_disallowed_device_name(&name))
}

fn is_always_virtual_filesystem(file_system: &str) -> bool {
    matches!(
        file_system,
        "9p" | "aufs"
            | "autofs"
            | "binfmt_misc"
            | "bpf"
            | "ceph"
            | "cgroup"
            | "cgroup2"
            | "cifs"
            | "configfs"
            | "debugfs"
            | "devpts"
            | "devtmpfs"
            | "efivarfs"
            | "glusterfs"
            | "hugetlbfs"
            | "mqueue"
            | "nfs"
            | "nfs4"
            | "nsfs"
            | "overlay"
            | "proc"
            | "pstore"
            | "ramfs"
            | "rpc_pipefs"
            | "securityfs"
            | "selinuxfs"
            | "smb3"
            | "sysfs"
            | "tmpfs"
            | "tracefs"
    )
}

fn is_disallowed_device_name(device: &str) -> bool {
    let name = Path::new(device)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    ["loop", "ram", "zram"].iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

fn preferred_over(candidate: &DiskMetric, current: &DiskMetric) -> bool {
    let candidate_root = candidate.mount_point == "/";
    let current_root = current.mount_point == "/";
    match candidate_root.cmp(&current_root) {
        Ordering::Greater => return true,
        Ordering::Less => return false,
        Ordering::Equal => {}
    }

    let candidate_depth = Path::new(&candidate.mount_point).components().count();
    let current_depth = Path::new(&current.mount_point).components().count();
    candidate_depth < current_depth
        || (candidate_depth == current_depth && candidate.mount_point < current.mount_point)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn parses_escapes_and_optional_fields() {
        let mounts = parse_mountinfo(
            "36 25 8:1 /root\\040dir /mnt\\040disk rw shared:7 master:1 - ext4 /dev/disk\\040one rw\n",
        )
        .unwrap();

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].root, Path::new("/root dir"));
        assert_eq!(mounts[0].mount_point, Path::new("/mnt disk"));
        assert_eq!(mounts[0].source, "/dev/disk one");
        assert_eq!((mounts[0].major, mounts[0].minor), (8, 1));
    }

    #[test]
    fn includes_block_backed_ext_and_device_mapper() {
        let fixture = Fixture::new();
        fixture.block(8, 1);
        fixture.block(253, 0);
        let mounts = parse_mountinfo(
            "1 0 8:1 / /data rw - ext4 /dev/sda1 rw\n2 0 253:0 / /crypt rw - xfs /dev/dm-0 rw\n",
        )
        .unwrap();
        let result = fixture.filter(
            vec![
                snapshot("/dev/sda1", "/data", "ext4", 100),
                snapshot("/dev/dm-0", "/crypt", "xfs", 200),
            ],
            Some(&mounts),
            true,
            &[],
            &[],
        );

        assert_eq!(mount_points(&result), ["/crypt", "/data"]);
    }

    #[test]
    fn joins_sysinfo_source_with_mountinfo_octal_escapes() {
        let fixture = Fixture::new();
        fixture.block(8, 4);
        let mounts =
            parse_mountinfo("1 0 8:4 / /media/data\\040disk rw - ext4 /dev/disk\\040one rw\n")
                .unwrap();
        let result = fixture.filter(
            vec![snapshot(
                "/dev/disk\\040one",
                "/media/data disk",
                "ext4",
                100,
            )],
            Some(&mounts),
            true,
            &[],
            &[],
        );

        assert_eq!(mount_points(&result), ["/media/data disk"]);
    }

    #[test]
    fn excludes_docker_overlay_and_bind_mount() {
        let fixture = Fixture::new();
        fixture.block(8, 1);
        let mounts = parse_mountinfo(
            "1 0 0:42 / / rw - overlay overlay rw\n2 0 8:1 /docker/hosts /etc/hosts rw - ext4 /dev/sda1 rw\n",
        )
        .unwrap();
        let result = fixture.filter(
            vec![
                snapshot("overlay", "/", "overlay", 100),
                snapshot("/dev/sda1", "/etc/hosts", "ext4", 100),
            ],
            Some(&mounts),
            true,
            &[],
            &[],
        );

        assert!(result.is_empty());
    }

    #[test]
    fn accepts_btrfs_root_but_subvolume_needs_exact_include() {
        let fixture = Fixture::new();
        fixture.block(8, 2);
        let mounts = parse_mountinfo(
            "1 0 8:2 /@ / rw - btrfs /dev/sda2 rw\n2 0 8:2 /@home /home rw - btrfs /dev/sda2 rw\n",
        )
        .unwrap();
        let snapshots = vec![
            snapshot("/dev/sda2", "/", "btrfs", 100),
            snapshot("/dev/sda2", "/home", "btrfs", 100),
        ];

        let automatic = fixture.filter(snapshots.clone(), Some(&mounts), true, &[], &[]);
        assert_eq!(mount_points(&automatic), ["/"]);
        let included = fixture.filter(snapshots, Some(&mounts), true, &["/home"], &[]);
        assert_eq!(mount_points(&included), ["/home"]);
    }

    #[test]
    fn accepts_openwrt_ubifs_and_jffs2_without_sysfs_block_device() {
        let fixture = Fixture::new();
        let mounts = parse_mountinfo(
            "1 0 0:20 / /overlay rw - ubifs ubi0:rootfs rw\n2 0 31:4 / /rom rw - jffs2 /dev/mtdblock4 rw\n",
        )
        .unwrap();
        let result = fixture.filter(
            vec![
                snapshot("ubi0:rootfs", "/overlay", "ubifs", 100),
                snapshot("/dev/mtdblock4", "/rom", "jffs2", 100),
            ],
            Some(&mounts),
            true,
            &[],
            &[],
        );

        assert_eq!(mount_points(&result), ["/overlay", "/rom"]);
    }

    #[test]
    fn excludes_network_memory_loop_and_zram_filesystems() {
        let fixture = Fixture::new();
        fixture.block(1, 0);
        fixture.block(7, 0);
        fixture.block(254, 0);
        let mounts = parse_mountinfo(
            "1 0 0:1 / /nfs rw - nfs server:/share rw\n2 0 0:2 / /tmp rw - tmpfs tmpfs rw\n3 0 7:0 / /loop rw - ext4 /dev/loop0 rw\n4 0 254:0 / /zram rw - ext4 /dev/zram0 rw\n5 0 1:0 / /ram rw - ext4 /dev/ram0 rw\n6 0 0:3 / /fuse rw - fuse.sshfs sshfs rw\n7 0 0:4 / /proc rw - proc proc rw\n",
        )
        .unwrap();
        let result = fixture.filter(
            vec![
                snapshot("server:/share", "/nfs", "nfs", 100),
                snapshot("tmpfs", "/tmp", "tmpfs", 100),
                snapshot("/dev/loop0", "/loop", "ext4", 100),
                snapshot("/dev/zram0", "/zram", "ext4", 100),
                snapshot("/dev/ram0", "/ram", "ext4", 100),
                snapshot("sshfs", "/fuse", "fuse.sshfs", 100),
                snapshot("proc", "/proc", "proc", 100),
            ],
            Some(&mounts),
            true,
            &[],
            &[],
        );

        assert!(result.is_empty());
    }

    #[test]
    fn missing_or_malformed_mountinfo_fails_closed_except_exact_includes() {
        let fixture = Fixture::new();
        let snapshots = vec![
            snapshot("/dev/sda1", "/", "ext4", 100),
            snapshot("mystery", "/keep", "unknown", 100),
        ];

        assert!(
            fixture
                .filter(snapshots.clone(), None, true, &[], &[])
                .is_empty()
        );
        assert_eq!(
            mount_points(&fixture.filter(snapshots, None, true, &["/keep"], &[])),
            ["/keep"]
        );
        assert!(parse_mountinfo("not mountinfo\n").is_err());
    }

    #[test]
    fn deduplicates_device_number_using_root_depth_and_lexical_preference() {
        let fixture = Fixture::new();
        fixture.block(8, 1);
        fixture.block(8, 2);
        fixture.block(8, 3);
        let mounts = parse_mountinfo(
            "1 0 8:1 / /mnt/z rw - ext4 /dev/sda1 rw\n2 0 8:1 / /data rw - ext4 /dev/sda1 rw\n3 0 8:1 / / rw - ext4 /dev/sda1 rw\n4 0 8:2 / /deep/path rw - ext4 /dev/sda2 rw\n5 0 8:2 / /shallow rw - ext4 /dev/sda2 rw\n6 0 8:3 / /z rw - ext4 /dev/sda3 rw\n7 0 8:3 / /a rw - ext4 /dev/sda3 rw\n",
        )
        .unwrap();
        let result = fixture.filter(
            vec![
                snapshot("/dev/sda1", "/mnt/z", "ext4", 100),
                snapshot("/dev/sda1", "/data", "ext4", 100),
                snapshot("/dev/sda1", "/", "ext4", 100),
                snapshot("/dev/sda2", "/deep/path", "ext4", 200),
                snapshot("/dev/sda2", "/shallow", "ext4", 200),
                snapshot("/dev/sda3", "/z", "ext4", 300),
                snapshot("/dev/sda3", "/a", "ext4", 300),
            ],
            Some(&mounts),
            true,
            &[],
            &[],
        );

        assert_eq!(mount_points(&result), ["/", "/a", "/shallow"]);
    }

    #[test]
    fn exclude_wins_and_disabled_local_filter_restores_virtual_entries() {
        let fixture = Fixture::new();
        let snapshots = vec![
            snapshot("overlay", "/", "overlay", 100),
            snapshot("overlay", "/copy", "overlay", 100),
        ];
        let result = fixture.filter(snapshots, None, false, &["/"], &["/"]);

        assert_eq!(mount_points(&result), ["/copy"]);
    }

    fn snapshot(
        device: &str,
        mount_point: &str,
        file_system: &str,
        total_bytes: u64,
    ) -> DiskSnapshot {
        DiskSnapshot {
            device: device.into(),
            mount_point: mount_point.into(),
            file_system: file_system.into(),
            total_bytes,
            available_bytes: total_bytes / 2,
        }
    }

    fn mount_points(metrics: &[DiskMetric]) -> Vec<&str> {
        metrics
            .iter()
            .map(|metric| metric.mount_point.as_str())
            .collect()
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

        fn block(&self, major: u32, minor: u32) {
            fs::create_dir_all(
                self.directory
                    .path()
                    .join("dev/block")
                    .join(format!("{major}:{minor}")),
            )
            .unwrap();
        }

        fn filter(
            &self,
            snapshots: Vec<DiskSnapshot>,
            mounts: Option<&[MountInfo]>,
            local_only: bool,
            include: &[&str],
            exclude: &[&str],
        ) -> Vec<DiskMetric> {
            filter_snapshots(
                &snapshots,
                mounts,
                local_only,
                &include.iter().map(PathBuf::from).collect::<HashSet<_>>(),
                &exclude.iter().map(PathBuf::from).collect::<HashSet<_>>(),
                self.directory.path(),
            )
        }
    }
}
