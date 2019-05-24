use failure::ResultExt;
use log::{debug, trace};
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use crate::{
    common::{dir_exists, format_size_with_unit, MigErrCtx, MigError, MigErrorKind},
    linux_common::{
        get_root_info, get_fs_space,
        disk_info::lsblk_info::{
            LsblkInfo, LsblkDevice, LsblkPartition},
    },
    defs::ROOT_PATH,
};

const MODULE: &str = "linux_common::path_info";

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
    pub mount_ro: bool,
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
    pub fn from_mounted<P1: AsRef<Path>, P2: AsRef<Path>>(
        abs_path: &P1,
        mountpoint: &P2,
        device: &LsblkDevice,
        partition: &LsblkPartition,
    ) -> Result<PathInfo, MigError> {
        let path = abs_path.as_ref().to_path_buf();
        let mountpoint = PathBuf::from(mountpoint.as_ref());

        trace!("from_mounted: entered with: path: '{}', mountpoint: '{}', device: '{}', partition: '{}'", path.display(), mountpoint.display(), device.name, partition.name);

        debug!("looking fo path: '{}'", path.display());


        let res_path = path.to_string_lossy();

        // get filesystem space for device

        let (fs_size, fs_free) = get_fs_space(&path)?;

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
            drive_size: if let Some(ref size) = device.size {
                size.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Could not parse drive size for partition: '{}'",
                        partition.get_path().display()
                    ),
                ))?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "drive size not found for partition: '{}'",
                        partition.get_path().display()
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
            mount_ro: partition.ro == "1",
            uuid: partition.uuid.clone(),
            part_uuid: partition.partuuid.clone(),
            part_label: partition.partlabel.clone(),
            part_size: if let Some(ref size) = partition.size {
                size.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "size could not be parsed for partition: '{}'",
                        partition.get_path().display()
                    ),
                ))?
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
            let (root_device, _root_fs_type) = get_root_info()?;
            lsblk_info.get_devinfo_from_partition(root_device)?
        } else {
            lsblk_info.get_path_info(&abs_path)?
        };


        if let Some(ref mountpoint) = partition.mountpoint {
            Ok(Some(PathInfo::from_mounted(
                &abs_path, mountpoint, &device, &partition,
            )?))
        } else {
            Err(MigError::from_remark(MigErrorKind::InvState, &format!("No mountpoint found for partition: '{}'", partition.get_path().display())))
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


