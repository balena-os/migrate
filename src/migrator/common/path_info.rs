use failure::ResultExt;
use std::path::{Path, PathBuf};

use crate::common::{device_info::DeviceInfo, MigErrCtx, MigError, MigErrorKind};

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
    #[cfg(target_os = "linux")]
    pub fn from_path<P: AsRef<Path>>(
        path: P,
        lsblk_info: &LsblkInfo,
    ) -> Result<Option<PathInfo>, MigError> {
        if !path.as_ref().exists() {
            return Ok(None);
        }

        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to canonicalize path: '{}'", path.as_ref().display()),
            ))?;

        let (drive, partition) = lsblk_info.get_path_devs(path.as_ref())?;
        let device_info = DeviceInfo::new(drive, partition)?;

        let (fs_size, fs_free) = get_fs_space(&abs_path)?;

        Ok(Some(PathInfo {
            device_info,
            path: abs_path,
            fs_size,
            fs_free,
        }))
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

        let device_info = DeviceInfo::new(drive, partition)?;

        let (fs_size, fs_free) = get_fs_space(&abs_path)?;

        Ok(PathInfo {
            device_info,
            path: abs_path,
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "windows")]
    fn from_volume_info(path: &Path, vol_info: &VolumeInfo) -> Result<PathInfo, MigError> {
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

    #[cfg(target_os = "windows")]
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<PathInfo, MigError> {
        if !path.as_ref().exists() {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("The path does not exist: '{}'", path.as_ref().display()),
            ));
        }

        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to canonicalize path: '{}'", path.as_ref().display()),
            ))?;

        Ok(PathInfo::from_volume_info(
            &abs_path,
            DriveInfo::new()?.from_path(abs_path)?,
        )?)
    }
}
