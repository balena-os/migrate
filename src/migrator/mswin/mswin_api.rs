use log::debug;
use std::path::{Path, PathBuf};

use crate::{
    common::{device_info::DeviceInfo, os_api::OSApi, path_info::PathInfo, MigError, MigErrorKind},
    defs::{FileType, OSArch},
    mswin::{
        win_api::get_volume_disk_extents,
        wmi_utils::{MountPoint, WMIOSInfo, WmiUtils},
    },
};

pub(crate) struct MSWinApi {
    os_info: WMIOSInfo,
}

impl MSWinApi {
    pub fn new() -> Result<MSWinApi, MigError> {
        Ok(MSWinApi {
            os_info: WmiUtils::get_os_info()?,
        })
    }
}

impl OSApi for MSWinApi {
    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        Ok(self.os_info.os_arch.clone())
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        Ok(self.os_info.os_name.clone())
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        unimplemented!()
    }

    fn device_info_from_partition<P: AsRef<Path>>(
        &self,
        partition: P,
    ) -> Result<DeviceInfo, MigError> {
        unimplemented!()
    }

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        unimplemented!()
    }
}
