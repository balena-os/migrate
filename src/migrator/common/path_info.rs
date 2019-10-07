use failure::ResultExt;

use std::path::{Path, PathBuf};

use crate::common::{
    device_info::DeviceInfo,
    os_api::{OSApi, OSApiImpl},
    MigErrCtx, MigError, MigErrorKind,
};

#[cfg(target_os = "linux")]
use crate::linux::lsblk_info::{LsblkDevice, LsblkInfo, LsblkPartition};

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

        Ok(PathInfo {
            device_info,
            path: abs_path,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn from_lsblk_info<P: AsRef<Path>>(
        path: P,
        lsblk_info: &LsblkInfo,
    ) -> Result<PathInfo, MigError> {
        let (drive, partition) = lsblk_info.get_devices_for_path(path.as_ref())?;

        Ok(PathInfo {
            device_info: DeviceInfo::from_lsblkinfo(drive, partition)?,
            path: PathBuf::from(path.as_ref()),
        })
    }

    #[cfg(target_os = "windows")]
    pub fn from_volume_info(path: &Path, vol_info: &VolumeInfo) -> Result<PathInfo, MigError> {
        Ok(PathInfo {
            // the physical device info
            device_info: DeviceInfo::from_volume_info(vol_info)?,
            // the absolute path
            path: path.to_path_buf(),
        })
    }
}
