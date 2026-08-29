use std::{
    fs,
    path::{Path, PathBuf},
};

use sysinfo::Disk;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MountInfo {
    pub(super) mount_id: u64,
    pub(super) major: u32,
    pub(super) minor: u32,
    pub(super) root: PathBuf,
    pub(super) mount_point: PathBuf,
    pub(super) file_system: String,
    pub(super) source: String,
}

pub(super) fn load_mountinfo(path: &Path) -> Result<Vec<MountInfo>, String> {
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
    let (major, minor) = fields[2]
        .split_once(':')
        .ok_or_else(|| "invalid device number".to_string())?;

    Ok(MountInfo {
        mount_id: fields[0]
            .parse()
            .map_err(|_| "invalid mount id".to_string())?,
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

pub(super) fn select_mount<'a>(disk: &Disk, mounts: &'a [MountInfo]) -> Option<&'a MountInfo> {
    let raw_source = disk.name().to_string_lossy();
    let source = decode_mount_field(&raw_source).unwrap_or_else(|_| raw_source.into_owned());
    let file_system = disk.file_system().to_string_lossy();
    mounts
        .iter()
        .filter(|mount| {
            mount.mount_point == disk.mount_point()
                && mount.source == source
                && mount.file_system == file_system
        })
        .max_by_key(|mount| mount.mount_id)
}

pub(super) fn is_proven_local(mount: &MountInfo, sys_root: &Path) -> bool {
    let filesystem = mount.file_system.to_ascii_lowercase();
    if matches!(filesystem.as_str(), "ubifs" | "jffs2") {
        return true;
    }
    if is_virtual_filesystem(&filesystem) || is_disallowed_device_name(&mount.source) {
        return false;
    }
    if filesystem == "btrfs" {
        return source_block_device_exists(&mount.source, sys_root);
    }
    if mount.root != Path::new("/") && mount.mount_point != Path::new("/") {
        return false;
    }
    if mount.major == 1 || mount.major == 7 {
        return false;
    }
    sys_root
        .join("dev/block")
        .join(format!("{}:{}", mount.major, mount.minor))
        .exists()
}

fn source_block_device_exists(source: &str, sys_root: &Path) -> bool {
    let path = Path::new(source);
    let Some(name) = path.file_name() else {
        return false;
    };
    path.starts_with("/dev")
        && !is_disallowed_device_name(source)
        && sys_root.join("class/block").join(name).exists()
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
        decoded.push(
            (bytes[index + 1] - b'0') * 64
                + (bytes[index + 2] - b'0') * 8
                + (bytes[index + 3] - b'0'),
        );
        index += 4;
    }
    String::from_utf8(decoded).map_err(|_| "mount field is not UTF-8".into())
}

fn is_virtual_filesystem(file_system: &str) -> bool {
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
            | "squashfs"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mountinfo_escapes_and_optional_fields() {
        let mounts = parse_mountinfo(
            "36 25 8:1 /root\\040dir /mnt\\040disk rw shared:7 master:1 - ext4 /dev/disk\\040one rw\n",
        )
        .unwrap();

        assert_eq!(mounts[0].root, Path::new("/root dir"));
        assert_eq!(mounts[0].mount_point, Path::new("/mnt disk"));
        assert_eq!(mounts[0].source, "/dev/disk one");
        assert_eq!((mounts[0].major, mounts[0].minor), (8, 1));
    }

    #[test]
    fn malformed_mountinfo_fails_closed() {
        assert!(parse_mountinfo("").is_err());
        assert!(parse_mountinfo("not mountinfo\n").is_err());
    }
}
