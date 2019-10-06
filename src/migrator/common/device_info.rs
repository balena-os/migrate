use log::error;
use std::path::PathBuf;

use crate::{
    common::{path_append, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
};

#[cfg(target_os = "linux")]
use crate::linux::{
    linux_common::get_fs_space,
    lsblk_info::{LsblkDevice, LsblkPartition},
};

#[cfg(target_os = "windows")]
use crate::mswin::drive_info::{DriveInfo, VolumeInfo};

#[derive(Debug, Clone)]
pub(crate) struct DeviceInfo {
    // the drive device path
    pub drive: String,
    // the devices mountpoint
    pub mountpoint: PathBuf,
    // the drive size
    pub drive_size: u64,
    // the partition device path
    pub device: String,
    // the partition index
    // TODO: make optional
    pub index: Option<u16>,
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
    // The file system size
    pub fs_size: u64,
    // the fs free space
    pub fs_free: u64,
}

impl DeviceInfo {
    #[cfg(target_os = "linux")]
    pub fn from_lsblkinfo(
        drive: &LsblkDevice,
        partition: &LsblkPartition,
    ) -> Result<DeviceInfo, MigError> {
        let (mountpoint, fs_size, fs_free) = if let Some(ref mountpoint) = partition.mountpoint {
            let (fs_size, fs_free) = get_fs_space(mountpoint)?;
            (mountpoint.clone(), fs_size, fs_free)
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "The required parameter mountpoint could not be found for '{}'",
                    partition.get_path().display()
                ),
            ));
        };

        Ok(DeviceInfo {
            drive: String::from(drive.get_path().to_string_lossy()),
            mountpoint,
            drive_size: if let Some(size) = drive.size {
                size
            } else {
                error!(
                    "The required parameter drive_size could not be found for '{}'",
                    drive.get_path().display()
                );
                return Err(MigError::displayed());
            },
            device: String::from(partition.get_path().to_string_lossy()),
            index: if let Some(index) = partition.index {
                Some(index)
            } else {
                None
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
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn from_volume_info(vol_info: &VolumeInfo) -> Result<DeviceInfo, MigError> {
        Ok(DeviceInfo {
            // the drive device path
            drive: String::from(vol_info.physical_drive.get_device_id()),
            // the devices mountpoint
            mountpoint: PathBuf::from(vol_info.logical_drive.get_name()),
            // the drive size
            drive_size: vol_info.physical_drive.get_size(),
            // the partition device path
            device: String::from(vol_info.volume.get_device_id()),
            // TODO: the partition index - this value is not correct in windows as hidden partotions are not counted
            index: None,
            // the partition fs type
            fs_type: String::from(vol_info.volume.get_file_system().to_linux_str()),
            // the partition uuid
            uuid: None,
            // the partition partuuid
            part_uuid: Some(vol_info.part_uuid.clone()),
            // the partition label
            part_label: if let Some(label) = vol_info.volume.get_label() {
                Some(String::from(label))
            } else {
                None
            },
            // the partition size
            part_size: vol_info.partition.get_size(),
            fs_size: if let Some(size) = vol_info.volume.get_capacity() {
                size
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "Required parameter size was not found for '{}'",
                        vol_info.volume.get_device_id()
                    ),
                ));
            },
            fs_free: if let Some(free) = vol_info.volume.get_free_space() {
                free
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "Required parameter size was not found for '{}'",
                        vol_info.volume.get_device_id()
                    ),
                ));
            },
        })
    }

    #[cfg(target_os = "windows")]
    pub fn for_efi() -> Result<DeviceInfo, MigError> {
        Ok(DeviceInfo::from_volume_info(
            &DriveInfo::new()?.for_efi_drive()?,
        )?)
    }

    pub fn get_kernel_cmd(&self) -> String {
        if let Some(ref partuuid) = self.part_uuid {
            format!("PARTUUID={}", partuuid)
        } else {
            if let Some(ref uuid) = self.uuid {
                format!("UUID={}", uuid)
            } else {
                self.device.clone()
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
