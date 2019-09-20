use failure::ResultExt;
use log::error;
use std::path::{Path, PathBuf};

use crate::{
    common::{path_append, MigErrCtx, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
};

#[cfg(target_os = "linux")]
use crate::linux::linux_common::get_fs_space;
#[cfg(target_os = "linux")]
use crate::linux::lsblk_info::LsblkInfo;
use crate::linux::lsblk_info::{LsblkDevice, LsblkPartition};

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    // the absolute path
    pub path: PathBuf,
    // the partition device path
    pub drive: PathBuf,
    // the partition fs type
    pub drive_size: u64,
    // the partition fs type
    pub device: PathBuf,
    // the partition index
    pub index: u16,
    // the devices mountpoint
    pub mountpoint: PathBuf,
    // the drive device path
    pub fs_type: String,
    // the partition read only flag
    // pub mount_ro: bool,
    // the partition uuid
    pub uuid: Option<String>,
    // the partition partuuid
    pub part_uuid: Option<String>,
    // the partition label
    pub part_label: Option<String>,
    // the partition size
    pub part_size: u64,
    // the fs size
    pub fs_size: u64,
    // the fs free space
    pub fs_free: u64,
}

impl PathInfo {
    pub fn get_kernel_cmd(&self) -> String {
        if let Some(ref partuuid) = self.part_uuid {
            format!("PARTUUID={}", partuuid)
        } else {
            if let Some(ref uuid) = self.uuid {
                format!("UUID={}", uuid)
            } else {
                String::from(self.path.to_string_lossy())
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
        if let Some(ref mountpoint) = partition.mountpoint {
            Ok(Some(PathInfo::from_parts(
                abs_path, mountpoint, drive, partition,
            )?))
        } else {
            error!("Refusing to create PathInfo from unmounted partiontion");
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
        PathInfo::from_parts(abs_path, mountpoint.as_ref(), drive, partition)
    }

    #[cfg(target_os = "linux")]
    fn from_parts(
        abs_path: PathBuf,
        mountpoint: &Path,
        drive: &LsblkDevice,
        partition: &LsblkPartition,
    ) -> Result<PathInfo, MigError> {
        let (fs_size, fs_free) = get_fs_space(&abs_path)?;

        Ok(PathInfo {
            path: abs_path,
            drive: drive.get_path(),
            drive_size: if let Some(size) = drive.size {
                size
            } else {
                error!("Refusing to create PathInfo with missing drive size");
                return Err(MigError::displayed());
            },
            device: partition.get_path(),
            index: if let Some(index) = partition.index {
                index
            } else {
                error!("Refusing to create PathInfo with missing partition index");
                return Err(MigError::displayed());
            },
            mountpoint: mountpoint.to_path_buf(),
            // the drive device path
            fs_type: if let Some(ref fs_type) = partition.fstype {
                fs_type.clone()
            } else {
                error!("Refusing to create PathInfo with missing partition fs type");
                return Err(MigError::displayed());
            },
            uuid: partition.uuid.clone(),
            part_uuid: partition.partuuid.clone(),
            part_label: partition.partlabel.clone(),
            // the partition size
            part_size: if let Some(size) = partition.size {
                size
            } else {
                error!("Refusing to create PathInfo with missing partition size");
                return Err(MigError::displayed());
            },
            fs_size,
            fs_free,
        })
    }
}
