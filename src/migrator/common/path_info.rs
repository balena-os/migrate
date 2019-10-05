use failure::ResultExt;

use std::path::{Path, PathBuf};

use crate::common::{
    device_info::DeviceInfo,
    os_api::{OSApi, OSApiImpl},
    MigErrCtx, MigError, MigErrorKind,
};

#[cfg(target_os = "linux")]
use crate::linux::{
    linux_common::get_fs_space,
    lsblk_info::{LsblkDevice, LsblkInfo, LsblkPartition},
};

#[cfg(target_os = "windows")]
use crate::mswin::{
    drive_info::{DriveInfo, VolumeInfo},
    wmi_utils::MountPoint,
};

/*
Contains full Information on a path including
- DeviceInfo: what drive & partition the path resides on with drive size
- File System information: mountpoint FS size & free space
*/

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    // the physical device info
    pub device_info: DeviceInfo,
    // the absolute path
    pub path: PathBuf,
    // the partition read only flag
    // pub mount_ro: bool,
    // The file system size
    pub fs_size: u64,
    // the fs free space
    pub fs_free: u64,
}

impl PathInfo {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<PathInfo, MigError> {
        let os_api = OSApi::new()?;
        os_api.path_info_from_path(&os_api.canonicalize(path)?)
    }

    #[cfg(target_os = "linux")]
    pub fn from_mounted<P1: AsRef<Path>, P2: AsRef<Path>>(
        path: P1,
        _mountpoint: P2,
        drive: &LsblkDevice,
        partition: &LsblkPartition,
    ) -> Result<PathInfo, MigError> {
        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to canonicalize path: '{}'", path.as_ref().display()),
            ))?;

        let device_info = DeviceInfo::from_lsblkinfo(drive, partition)?;

        let (fs_size, fs_free) = get_fs_space(&abs_path)?;

        Ok(PathInfo {
            device_info,
            path: abs_path,
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn from_lsblk_info<P: AsRef<Path>>(
        path: P,
        lsblk_info: &LsblkInfo,
    ) -> Result<PathInfo, MigError> {
        let (drive, partition) = lsblk_info.get_path_devs(path.as_ref())?;

        let (fs_size, fs_free) = get_fs_space(partition.get_path())?;
        Ok(PathInfo {
            device_info: DeviceInfo {
                // the drive device path
                drive: String::from(drive.get_path().to_string_lossy()),
                // the devices mountpoint
                mountpoint: if let Some(ref mountpoint) = partition.mountpoint {
                    mountpoint.clone()
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvState,
                        &format!(
                            "Device is not mounted: '{}'",
                            partition.get_path().display()
                        ),
                    ));
                },
                // the drive size
                drive_size: if let Some(size) = drive.size {
                    size
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvState,
                        &format!(
                            "No disk size parameter found for Device : '{}'",
                            drive.get_path().display()
                        ),
                    ));
                },
                // the partition device path
                device: String::from(partition.get_path().to_string_lossy()),
                // the partition index
                // TODO: make optional
                index: partition.index,
                // the partition fs type
                fs_type: if let Some(ref fstype) = partition.fstype {
                    fstype.clone()
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvState,
                        &format!(
                            "No fstype parameter found for Device : '{}'",
                            partition.get_path().display()
                        ),
                    ));
                },
                // the partition uuid
                uuid: partition.uuid.clone(),
                // the partition partuuid
                part_uuid: partition.partuuid.clone(),
                // the partition label
                part_label: partition.label.clone(),
                // the partition size
                part_size: if let Some(size) = partition.size {
                    size
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvState,
                        &format!(
                            "No partition size parameter found for Device : '{}'",
                            partition.get_path().display()
                        ),
                    ));
                },
            },
            path: PathBuf::from(path.as_ref()),
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn from_volume_info(path: &Path, vol_info: &VolumeInfo) -> Result<PathInfo, MigError> {
        Ok(PathInfo {
            // the physical device info
            device_info: DeviceInfo::from_volume_info(vol_info),
            // the absolute path
            path: path.to_path_buf(),
            // the partition read only flag
            // pub mount_ro: bool,
            // The file system size
            fs_size: if let Some(capacity) = vol_info.volume.get_capacity() {
                capacity
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("No fs capacity found for path '{}'", path.display()),
                ));
            },
            // the fs free space
            fs_free: if let Some(free_space) = vol_info.volume.get_free_space() {
                free_space
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("No fs free_space found for path '{}'", path.display()),
                ));
            },
        })
    }
}
