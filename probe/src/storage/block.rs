use std::{fs, path::Path};

use super::{FilesystemSnapshot, read_u64};

const LINUX_BLOCK_SECTOR_BYTES: u64 = 512;

pub(super) fn storage_owner(snapshot: &FilesystemSnapshot, sys_root: &Path) -> Option<String> {
    let block = block_device_name(snapshot, sys_root)?;
    let class_path = sys_root.join("class/block").join(&block);
    if !class_path.join("partition").exists() {
        return Some(block);
    }

    fs::canonicalize(&class_path)
        .ok()
        .and_then(|path| {
            path.parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|parent| parent != "block")
        .or_else(|| partition_parent_name(&block))
}

pub(super) fn block_device_name(snapshot: &FilesystemSnapshot, sys_root: &Path) -> Option<String> {
    if let Some(mount) = &snapshot.mount {
        let path = sys_root
            .join("dev/block")
            .join(format!("{}:{}", mount.major, mount.minor));
        if let Ok(target) = fs::canonicalize(path)
            && let Some(name) = target.file_name()
        {
            return Some(name.to_string_lossy().into_owned());
        }
    }

    let name = Path::new(&snapshot.device)
        .file_name()?
        .to_string_lossy()
        .into_owned();
    sys_root
        .join("class/block")
        .join(&name)
        .exists()
        .then_some(name)
}

pub(super) fn device_size_bytes(device: &str, sys_root: &Path) -> Option<u64> {
    read_u64(sys_root.join("class/block").join(device).join("size"))?
        .checked_mul(LINUX_BLOCK_SECTOR_BYTES)
}

fn partition_parent_name(name: &str) -> Option<String> {
    if let Some(index) = name.rfind('p') {
        let suffix = &name[index + 1..];
        if !suffix.is_empty()
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
            && (name.starts_with("nvme") || name.starts_with("mmcblk") || name.starts_with("md"))
        {
            return Some(name[..index].to_owned());
        }
    }

    let uses_trailing_partition_number = ["sd", "vd", "xvd", "hd"]
        .iter()
        .any(|prefix| name.starts_with(prefix));
    let split = name
        .trim_end_matches(|character: char| character.is_ascii_digit())
        .len();
    (uses_trailing_partition_number && split < name.len() && split > 0)
        .then(|| name[..split].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_common_partition_parents() {
        assert_eq!(partition_parent_name("sda2").as_deref(), Some("sda"));
        assert_eq!(
            partition_parent_name("nvme0n1p2").as_deref(),
            Some("nvme0n1")
        );
        assert_eq!(
            partition_parent_name("mmcblk0p5").as_deref(),
            Some("mmcblk0")
        );
        assert_eq!(partition_parent_name("dm-0"), None);
    }
}
