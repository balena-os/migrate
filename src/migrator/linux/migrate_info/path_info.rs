use failure::ResultExt;
use log::{debug, trace};
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use crate::{
    common::{dir_exists, format_size_with_unit, MigErrCtx, MigError, MigErrorKind},
    linux::{
        linux_common::{get_fs_space, get_kernel_root_info},
        linux_defs::ROOT_PATH,
        migrate_info::lsblk_info::{LsblkDevice, LsblkInfo, LsblkPartition},
        EnsuredCmds,
    },
};

const MODULE: &str = "linux_common::path_info";

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    // the absolute path
    pub path: PathBuf,
    // the drive device path
    pub drive: PathBuf,
    // the drive size
    pub drive_size: u64,
    // the partition device path
    pub device: PathBuf,
    // the partition index
    pub index: u16,
    // the partition mountpoint
    pub mountpoint: PathBuf,
    // the file system type
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
    /*    pub fn get_portable_path(&self) -> PathBuf {
        if let Some(ref part_uuid) = self.part_uuid {
            path_append(DISK_BY_PARTUUID_PATH, part_uuid)
        } else {
            self.path.clone()
        }
    } */

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

    pub fn from_mounted<P1: AsRef<Path>, P2: AsRef<Path>>(
        cmds: &EnsuredCmds,
        abs_path: &P1,
        mountpoint: &P2,
        device: &LsblkDevice,
        partition: &LsblkPartition,
    ) -> Result<PathInfo, MigError> {
        let path = abs_path.as_ref().to_path_buf();
        let mountpoint = PathBuf::from(mountpoint.as_ref());

        trace!("from_mounted: entered with: path: '{}', mountpoint: '{}', device: '{}', partition: '{}'", path.display(), mountpoint.display(), device.name, partition.name);

        debug!("looking fo path: '{}'", path.display());

        // get filesystem space for device

        let (fs_size, fs_free) = get_fs_space(cmds, &path)?;

        let result = PathInfo {
            path: path,
            device: PathBuf::from(partition.get_path()),
            index: if let Some(index) = partition.index {
                index
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "index not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            mountpoint: PathBuf::from(mountpoint),
            drive: PathBuf::from(device.get_path()),
            drive_size: if let Some(size) = device.size {
                size
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "size not found for device: '{}'",
                        device.get_path().display()
                    ),
                ));
            },

            fs_type: if let Some(ref fs_type) = partition.fstype {
                fs_type.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "fs_type not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            //mount_ro: partition.ro == "1",
            uuid: partition.uuid.clone(),
            part_uuid: partition.partuuid.clone(),
            part_label: partition.partlabel.clone(),
            part_size: if let Some(size) = partition.size {
                size
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "size not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            fs_size,
            fs_free,
        };

        debug!(
            "PathInfo::new: '{}' lsblk result: '{:?}'",
            result.path.display(),
            result
        );

        Ok(result)
    }

    pub fn new<P: AsRef<Path>>(
        cmds: &EnsuredCmds,
        path: P,
        lsblk_info: &LsblkInfo,
    ) -> Result<Option<PathInfo>, MigError> {
        let path = path.as_ref();

        trace!("PathInfo::new: entered with: '{}'", path.display());

        if !dir_exists(path)? {
            return Ok(None);
        }

        let abs_path = std::fs::canonicalize(path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::new: failed to create absolute path from {}",
                MODULE,
                path.display()
            ),
        ))?;

        debug!("looking fo path: '{}'", abs_path.display());

        let (device, partition) = if abs_path == Path::new(ROOT_PATH) {
            let (root_device, _root_fs_type) = get_kernel_root_info()?;
            lsblk_info.get_devinfo_from_partition(root_device)?
        } else {
            lsblk_info.get_path_info(&abs_path)?
        };

        if let Some(ref mountpoint) = partition.mountpoint {
            Ok(Some(PathInfo::from_mounted(
                cmds, &abs_path, mountpoint, &device, &partition,
            )?))
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!(
                    "No mountpoint found for partition: '{}'",
                    partition.get_path().display()
                ),
            ))
        }
    }
}

impl Display for PathInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "path: {} device: {}, uuid: {}, fstype: {}, size: {}, fs_size: {}, fs_free: {}",
            self.path.display(),
            self.device.display(),
            if let Some(ref uuid) = self.uuid {
                uuid.as_str()
            } else {
                "-"
            },
            self.fs_type,
            format_size_with_unit(self.part_size),
            format_size_with_unit(self.fs_size),
            format_size_with_unit(self.fs_free)
        )
    }
}
