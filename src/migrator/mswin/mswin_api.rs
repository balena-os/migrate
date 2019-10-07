use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error};
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::common::MigErrCtx;
use crate::{
    common::{
        config::migrate_config::DeviceSpec, device_info::DeviceInfo, os_api::OSApiImpl,
        path_info::PathInfo, MigError, MigErrorKind,
    },
    defs::{FileType, OSArch},
    mswin::{
        drive_info::DriveInfo,
        win_api::get_volume_disk_extents,
        wmi_utils::{MountPoint, WMIOSInfo, WmiUtils},
    },
};

const UNC_RE: &str = r#"^\\\\(\?|\.)\\([A-Z]:.*)$"#;

pub(crate) struct MSWinApi {
    os_info: WMIOSInfo,
    drive_info: DriveInfo,
}

impl MSWinApi {
    pub fn new() -> Result<MSWinApi, MigError> {
        Ok(MSWinApi {
            os_info: WmiUtils::get_os_info()?,
            drive_info: DriveInfo::new()?,
        })
    }
}

impl OSApiImpl for MSWinApi {
    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError> {
        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to canonicalize path: '{}'", path.as_ref().display()),
            ))?;

        lazy_static! {
            static ref UNC_REGEX: Regex = Regex::new(UNC_RE).unwrap();
        }

        if let Some(captures) = UNC_REGEX.captures(&*abs_path.to_string_lossy()) {
            debug!("UNC regex matched: '{}'", abs_path.display());
            Ok(PathBuf::from(captures.get(2).unwrap().as_str()))
        } else {
            debug!("UNC regex did not match: '{}'", abs_path.display());
            Ok(abs_path)
        }
    }

    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        Ok(self.os_info.os_arch.clone())
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        Ok(self.os_info.os_name.clone())
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        Ok(PathInfo::from_volume_info(
            path.as_ref(),
            self.drive_info.from_path(path.as_ref())?,
        )?)
    }

    fn device_path_from_partition(&self, device: &DeviceSpec) -> Result<PathBuf, MigError> {
        let volume_info = match device {
            DeviceSpec::DevicePath(dev_path) => {
                error!(
                    "Device path '{}' is not supported for windows partition specifyer",
                    dev_path.display()
                );
                return Err(MigError::displayed());
            }
            DeviceSpec::Label(label) => self.drive_info.from_label(label)?,
            DeviceSpec::PartUuid(partuuid) => self.drive_info.from_partuuid(partuuid)?,
            DeviceSpec::Path(path) => &self.drive_info.from_path(path)?,
            DeviceSpec::Uuid(uuid) => {
                error!(
                    "Device uuid '{}' is not supported for windows partition specifier",
                    uuid
                );
                return Err(MigError::displayed());
            }
        };

        Ok(volume_info.get_linux_path())
    }

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        // TODO: do something smarter than nothing
        return Ok(());
    }
}
