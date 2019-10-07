use failure::ResultExt;
use std::path::{Path, PathBuf};

use crate::{
    common::{
        config::migrate_config::DeviceSpec, os_api::OSApiImpl, path_info::PathInfo, MigErrCtx,
        MigError, MigErrorKind,
    },
    defs::{FileType, OSArch},
    linux::{
        linux_common::{expect_type, get_os_arch, get_os_name},
        lsblk_info::LsblkInfo,
    },
};

pub(crate) struct LinuxAPI {
    lsblk_info: LsblkInfo,
}

impl LinuxAPI {
    pub fn new() -> Result<LinuxAPI, MigError> {
        Ok(LinuxAPI {
            lsblk_info: LsblkInfo::all()?,
        })
    }
}

impl OSApiImpl for LinuxAPI {
    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError> {
        Ok(path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Unable to canonicalize path: '{}'", path.as_ref().display()),
            ))?)
    }

    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        get_os_arch()
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        get_os_name()
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        PathInfo::from_lsblk_info(path, &self.lsblk_info)
    }

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        expect_type(file.as_ref(), ftype)
    }

    fn device_path_from_partition(&self, device: &DeviceSpec) -> Result<PathBuf, MigError> {
        let (_drive, partition) = match device {
            DeviceSpec::DevicePath(dev_path) => self
                .lsblk_info
                .get_devices_for_partition(dev_path.as_path())?,
            DeviceSpec::PartUuid(partuuid) => self.lsblk_info.get_devices_for_partuuid(partuuid)?,
            DeviceSpec::Path(path) => self.lsblk_info.get_devices_for_path(path)?,
            DeviceSpec::Uuid(uuid) => self.lsblk_info.get_devices_for_uuid(uuid)?,
            DeviceSpec::Label(label) => self.lsblk_info.get_devices_for_label(label)?,
        };

        Ok(partition.get_linux_path()?)
    }
}
