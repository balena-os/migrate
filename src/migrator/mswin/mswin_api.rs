use log::debug;
use std::path::{Path, PathBuf};

use crate::common::MigErrorKind;
use crate::mswin::win_api::get_volume_disk_extents;
use crate::mswin::wmi_utils::WmiUtils;
use crate::{
    common::{device_info::DeviceInfo, os_api::OSApi, path_info::PathInfo, MigError},
    defs::{FileType, OSArch},
    mswin::wmi_utils::{MountPoint, WMIOSInfo},
};

pub(crate) struct MSWinApi<'a> {
    os_info: &'a WMIOSInfo,
}

impl MSWinApi<'_> {
    pub fn new(os_info: &WMIOSInfo) -> Result<MSWinApi, MigError> {
        Ok(MSWinApi { os_info })
    }
}

impl OSApi for MSWinApi<'_> {
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
