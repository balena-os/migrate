use failure::ResultExt;
use std::path::{Path, PathBuf};

use crate::{
    common::{
        config::migrate_config::DeviceSpec, device_info::DeviceInfo, os_api::OSApi,
        path_info::PathInfo, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{FileType, OSArch},
    linux::{
        linux_common::{expect_type, get_mem_info, get_os_arch, get_os_name},
        lsblk_info::LsblkInfo,
    },
};

pub(crate) struct LinuxAPI {
    lsblk_info: LsblkInfo,
}

impl LinuxAPI {
    pub fn new() -> Result<LinuxAPI, MigError> {
        Ok(LinuxAPI {
            lsblk_info: LsblkInfo::new()?,
        })
    }
}

impl OSApi for LinuxAPI {
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

    fn device_info_from_devspec(&self, device: &DeviceSpec) -> Result<DeviceInfo, MigError> {
        let (drive, partition) = match device {
            DeviceSpec::DevicePath(dev_path) => self
                .lsblk_info
                .get_devices_for_partition(dev_path.as_path())?,
            DeviceSpec::PartUuid(partuuid) => self.lsblk_info.get_devices_for_partuuid(partuuid)?,
            DeviceSpec::Path(path) => self.lsblk_info.get_devices_for_path(path)?,
            DeviceSpec::Uuid(uuid) => self.lsblk_info.get_devices_for_uuid(uuid)?,
            DeviceSpec::Label(label) => self.lsblk_info.get_devices_for_label(label)?,
        };

        Ok(DeviceInfo::from_lsblkinfo(&drive, &partition)?)
    }

    fn get_mem_info(&self) -> Result<(u64, u64), MigError> {
        get_mem_info()
    }

    fn path_info_for_efi(&self) -> Result<PathInfo, MigError> {
        Err(MigError::from_remark(
            MigErrorKind::InvState,
            "path_info_for_efi is no implemented in linux_api",
        ))
    }

    fn to_linux_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError> {
        Ok(PathBuf::from(path.as_ref()))
    }
}
