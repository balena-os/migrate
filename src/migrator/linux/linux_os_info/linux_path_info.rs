use std::path::{Path, PathBuf};

use crate::common::os_info::{OSInfo, PathInfo};

#[derive(Debug, Clone, PartialEq)]
pub struct LinuxPathInfo {
    abs_path: PathBuf,
    drive: PathBuf,
    partition: PathBuf,
    mountpoint: PathBuf,
    drive_size: u64,
    fs_type: String,
    fs_size: u64,
    fs_free: u64,
    uuid: Option<String>,
    part_uuid: Option<String>,
    label: Option<String>,
}

impl PathInfo for LinuxPathInfo {
    fn get_path(&self) -> &Path {
        self.abs_path.as_path()
    }

    fn get_drive(&self) -> &Path {
        self.drive.as_path()
    }
    fn get_partition(&self) -> &Path {
        self.partition.as_path()
    }
    fn get_mountpoint(&self) -> Option<&Path> {
        if let Some(ref mp) = self.mountpoint {
            Some(mp.as_path())
        } else {
            None
        }
    }

    fn get_drive_size(&self) -> u64 {
        self.drive_size
    }

    fn get_fs_type(&self) -> &str {
        self.fs_type.as_str()
    }

    fn get_fs_size(&self) -> u64 {
        self.fs_size
    }

    fn get_fs_free(&self) -> u64 {
        self.fs_free
    }

    fn get_uuid(&self) -> Option<&str> {
        if let Some(ref uuid) = self.uuid {
            Some(uuid.as_str())
        } else {
            None
        }
    }

    fn get_part_uuid(&self) -> Option<&str> {
        if let Some(ref part_uuid) = self.part_uuid {
            Some(part_uuid.as_str())
        } else {
            None
        }
    }

    fn get_label(&self) -> Option<&str> {
        if let Some(ref label) = self.label {
            Some(label.as_str())
        } else {
            None
        }
    }
}
