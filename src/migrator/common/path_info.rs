use failure::ResultExt;
use log::error;
use std::path::{Path, PathBuf};

use crate::common::{device_info::DeviceInfo, MigErrCtx, MigError, MigErrorKind};

#[cfg(target_os = "linux")]
use crate::linux::{
    linux_common::get_fs_space,
    lsblk_info::{LsblkDevice, LsblkInfo, LsblkPartition},
};

#[cfg(target_os = "windows")]
use crate::mswin::wmi_utils::MountPoint;

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

        if let Some(ref mountpoint) = partition.mountpoint {
            let (fs_size, fs_free) = get_fs_space(&abs_path)?;

            Ok(Some(PathInfo {
                device_info,
                path: abs_path,
                mountpoint: mountpoint.to_path_buf(),
                fs_size,
                fs_free,
            }))
        } else {
            error!("Refusing to create PathInfo from unmounted partition");
            return Err(MigError::displayed());
        }
    }

    #[cfg(target_os = "linux")]
    pub fn from_mounted<P1: AsRef<Path>, P2: AsRef<Path>>(
        path: P1,
        mountpoint: P2,
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
            mountpoint: mountpoint.as_ref().to_path_buf(),
            fs_size,
            fs_free,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn for_efi<P: AsRef<Path>>(path: P) -> Result<PathInfo, MigError> {
        unimplemented!()
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

        let mountpoints = MountPoint::query_path(abs_path)?;

        unimplemented!()
    }
}
