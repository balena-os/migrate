#[cfg(target_os = "linux")]
use log::error;

use std::path::PathBuf;

use crate::{
    common::{path_append, MigError},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
};

#[cfg(target_os = "linux")]
use crate::{
    common::MigErrorKind,
    linux::lsblk_info::{block_device::BlockDevice, partition::Partition}
};

#[cfg(target_os = "windows")]
use crate::{
    common::os_api::{OSApi},
    mswin::drive_info::VolumeInfo,
};
use crate::common::os_api::OSApiImpl;

#[derive(Debug, Clone)]
pub(crate) struct DeviceInfo {
    // the drive device path
    pub drive: String,
    // the drive size
    pub drive_size: u64,
    // the partition device path
    pub device: String,
    // the partition index
    // TODO: make optional
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
}

impl DeviceInfo {
    #[cfg(target_os = "linux")]
    pub fn from_lsblkinfo(
        drive: &BlockDevice,
        partition: &Partition,
    ) -> Result<DeviceInfo, MigError> {
        Ok(DeviceInfo {
            drive: String::from(drive.get_path().to_string_lossy()),
            drive_size: drive.size,
            device: String::from(partition.get_path().to_string_lossy()),
            index: partition.index,
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
            part_label: partition.label.clone(),
            part_size: partition.size,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn from_volume_info(vol_info: &VolumeInfo) -> Result<DeviceInfo, MigError> {
        Ok(DeviceInfo {
            // the drive device path
            drive: String::from(vol_info.physical_drive.get_device_id()),
            // the drive size
            drive_size: vol_info.physical_drive.get_size(),
            // the partition device path
            device: String::from(vol_info.volume.get_device_id()),
            // TODO: the partition index - this value is not correct in windows as hidden partions are not counted
            index: 0,
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
        })
    }

    #[cfg(target_os = "windows")]
    pub fn for_efi() -> Result<DeviceInfo, MigError> {
        OSApiImpl::new()?.device_info_for_efi()
    }

    #[allow(dead_code)]
    // TODO: used by RPI
    pub fn get_kernel_cmd(&self) -> String {
        if let Some(ref uuid) = self.uuid {
            format!("UUID={}", uuid)
        } else if let Some(ref partuuid) = self.part_uuid {
            format!("PARTUUID={}", partuuid)
        } else {
            self.device.clone()
        }
    }

    pub fn get_alt_path(&self) -> PathBuf {
        if let Some(ref uuid) = self.uuid {
            path_append(DISK_BY_UUID_PATH, uuid)
        } else if let Some(ref partuuid) = self.part_uuid {
            path_append(DISK_BY_PARTUUID_PATH, partuuid)
        } else if let Some(ref label) = self.part_label {
            path_append(DISK_BY_LABEL_PATH, label)
        } else {
            path_append("/dev", &self.device)
        }
    }
}
