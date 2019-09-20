use std::path::{Path};
use log::error;

use crate::{
    defs::OSArch,
    common::{ os_api::{OSApi,}, MigError, path_info::PathInfo, },
    linux::{
        linux_common::get_os_arch,
        lsblk_info::LsblkInfo,
    },
};

pub (crate) struct LinuxAPI<'a> {
    lsblk_info: &'a LsblkInfo,
}

impl LinuxAPI<'_> {
    pub fn new(lsblk_info: & LsblkInfo) -> LinuxAPI {
        LinuxAPI{
            lsblk_info,
        }
    }
}

impl OSApi for LinuxAPI<'_> {
    fn get_os_arch() -> Result<OSArch, MigError> {
        get_os_arch()
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo,MigError> {
        if let Some(path_info) = PathInfo::from_path(path.as_ref(),self.lsblk_info)? {
            Ok(path_info)
        } else {
            error!("Unable to create path info from '{}'", path.as_ref().display());
            Err(MigError::displayed())
        }
    }


    fn path_info_from_partition<P: AsRef<Path>>(&self, part: P) -> Result<PathInfo,MigError> {
       let (drive, partition) = self.lsblk_info.get_devinfo_from_partition(part.as_ref())?;
       if let Some(ref mountpoint) = partition.mountpoint {
           Ok(PathInfo::from_mounted(mountpoint.as_path(), mountpoint.as_path(), drive, partition)?)
       } else {
           error!("Unable to create path info from unmounted partition '{}'", part.as_ref().display());
           Err(MigError::displayed())
       }
    }
}