use std::path::Path;

use crate::{
    defs::{FileType,},
    common::{device_info::DeviceInfo, path_info::PathInfo, MigError,},
    defs::OSArch,
};

pub(crate) trait OSApi {
    fn get_os_arch(&self) -> Result<OSArch, MigError>;
    fn get_os_name(&self) -> Result<String, MigError>;

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError>;
    fn device_info_from_partition<P: AsRef<Path>>(
        &self,
        partition: P,
    ) -> Result<DeviceInfo, MigError>;
    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError>;
}
