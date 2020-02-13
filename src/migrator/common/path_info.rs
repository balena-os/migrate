use std::path::{Path, PathBuf};

use crate::common::{
    MigErrorKind,
    device_info::DeviceInfo,
    MigError,
    os_api::{OSApi, OSApiImpl},
};

#[cfg(target_os = "linux")]
use failure::ResultExt;

#[cfg(target_os = "linux")]
use crate::{
    common::MigErrCtx,
    linux::lsblk_info::{block_device::BlockDevice, partition::Partition, LsblkInfo},
    linux::linux_common::get_fs_space,
};

#[cfg(target_os = "windows")]
use crate::mswin::drive_info::VolumeInfo;

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
    // the devices mountpoint
    pub mountpoint: PathBuf,
    // The file system size
    pub fs_size: u64,
    // the fs free space
    pub fs_free: u64,
}

impl PathInfo {
    //#[cfg(target_os = "linux")]
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<PathInfo, MigError> {
        let os_api = OSApiImpl::new()?;
        os_api.path_info_from_path(&os_api.canonicalize(path)?)
    }

    #[cfg(target_os = "linux")]
    pub fn from_mounted<P1: AsRef<Path>, P2: AsRef<Path>>(
        path: P1,
        _mountpoint: P2,
        drive: &BlockDevice,
        partition: &Partition,
    ) -> Result<PathInfo, MigError> {
        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to canonicalize path: '{}'", path.as_ref().display()),
            ))?;

        let device_info = DeviceInfo::from_lsblkinfo(drive, partition)?;

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

        Ok(PathInfo {
            device_info,
            path: abs_path,
            mountpoint,
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn from_lsblk_info<P: AsRef<Path>>(
        path: P,
        lsblk_info: &LsblkInfo,
    ) -> Result<PathInfo, MigError> {
        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to canonicalize path: '{}'", path.as_ref().display()),
            ))?;

        let (drive, partition) = lsblk_info.get_devices_for_path(&abs_path)?;

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

        Ok(PathInfo {
            device_info: DeviceInfo::from_lsblkinfo(drive, partition)?,
            path: abs_path,
            mountpoint,
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn from_volume_info(path: &Path, vol_info: &VolumeInfo) -> Result<PathInfo, MigError> {
        Ok(PathInfo {
            // the physical device info
            device_info: DeviceInfo::from_volume_info(vol_info)?,
            // the absolute path
            path: path.to_path_buf(),
            mountpoint: PathBuf::from(vol_info.logical_drive.get_name()),
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

}
