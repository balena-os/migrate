use log::error;
use std::path::PathBuf;

use crate::{
    common::{path_append, MigError},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
};

#[cfg(target_os = "linux")]
use crate::linux::lsblk_info::{LsblkDevice, LsblkPartition};

#[derive(Debug, Clone)]
pub(crate) struct DeviceInfo {
    // the drive device path
    pub drive: PathBuf,
    // the drive size
    pub drive_size: u64,
    // the partition device path
    pub device: PathBuf,
    // the partition index
    pub index: u16,
    // the partition fs type
    pub fs_type: String,
    // the partition uuid
    pub uuid: Option<String>,
    // the partition partuuid
    pub part_uuid: Option<String>,
    // the partition label
    pub part_label: Option<String>,
    // the partition size
    pub part_size: u64,
    // the fs size
}

impl DeviceInfo {
    #[cfg(target_os = "linux")]
    pub fn new(drive: &LsblkDevice, partition: &LsblkPartition) -> Result<DeviceInfo, MigError> {
        Ok(DeviceInfo {
            drive: drive.get_path(),
            drive_size: if let Some(size) = drive.size {
                size
            } else {
                error!(
                    "The required parameter drive_size could not be found for '{}'",
                    drive.get_path().display()
                );
                return Err(MigError::displayed());
            },
            device: partition.get_path(),
            index: if let Some(index) = partition.index {
                index
            } else {
                error!(
                    "The required parameter index could not be found for '{}'",
                    partition.get_path().display()
                );
                return Err(MigError::displayed());
            },
            fs_type: if let Some(ref fstype) = partition.fstype {
                fstype.clone()
            } else {
                error!(
                    "The required parameter fs type could not be found for '{}'",
                    partition.get_path().display()
                );
                return Err(MigError::displayed());
            },
            uuid: partition.uuid.clone(),
            part_uuid: partition.partuuid.clone(),
            part_label: partition.partlabel.clone(),
            part_size: if let Some(size) = partition.size {
                size
            } else {
                error!(
                    "The required parameter size could not be found for '{}'",
                    partition.get_path().display()
                );
                return Err(MigError::displayed());
            },
        })
    }

    pub fn get_kernel_cmd(&self) -> String {
        if let Some(ref partuuid) = self.part_uuid {
            format!("PARTUUID={}", partuuid)
        } else {
            if let Some(ref uuid) = self.uuid {
                format!("UUID={}", uuid)
            } else {
                String::from(self.device.to_string_lossy())
            }
        }
    }

    pub fn get_alt_path(&self) -> PathBuf {
        if let Some(ref partuuid) = self.part_uuid {
            path_append(DISK_BY_PARTUUID_PATH, partuuid)
        } else {
            if let Some(ref uuid) = self.uuid {
                path_append(DISK_BY_UUID_PATH, uuid)
            } else {
                if let Some(ref label) = self.part_label {
                    path_append(DISK_BY_LABEL_PATH, label)
                } else {
                    path_append("/dev", &self.device)
                }
            }
        }
    }
}
