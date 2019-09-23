use std::path::Path;

use crate::{
    common::{device_info::DeviceInfo, path_info::PathInfo, MigError},
    defs::OSArch,
};

pub(crate) trait OSApi {
    fn get_os_arch() -> Result<OSArch, MigError>;
    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError>;
    fn device_info_from_partition<P: AsRef<Path>>(
        &self,
        partition: P,
    ) -> Result<DeviceInfo, MigError>;
}
