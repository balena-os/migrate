use log::error;
use std::path::Path;

use crate::{
    common::{device_info::DeviceInfo, os_api::OSApi, path_info::PathInfo, MigError},
    defs::{OSArch, FileType},
    linux::{ linux_common::{get_os_arch, get_os_name, expect_type}, lsblk_info::LsblkInfo},
};

pub(crate) struct LinuxAPI<'a> {
    lsblk_info: &'a LsblkInfo,
}

impl LinuxAPI<'_> {
    pub fn new(lsblk_info: &LsblkInfo) -> LinuxAPI {
        LinuxAPI { lsblk_info }
    }
}

impl OSApi for LinuxAPI<'_> {
    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        get_os_arch()
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        get_os_name()
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        if let Some(path_info) = PathInfo::from_path(path.as_ref(), self.lsblk_info)? {
            Ok(path_info)
        } else {
            error!(
                "Unable to create path info from '{}'",
                path.as_ref().display()
            );
            Err(MigError::displayed())
        }
    }

    fn device_info_from_partition<P: AsRef<Path>>(&self, part: P) -> Result<DeviceInfo, MigError> {
        let (drive, partition) = self.lsblk_info.get_devinfo_from_partition(part.as_ref())?;
        Ok(DeviceInfo::new(drive, partition)?)
    }

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        expect_type(file.as_ref(), ftype)
    }
}
