use log::{debug};
use std::path::{Path, PathBuf};

use crate::{
    common::{OSArch, MigError, MigErrorKind},
    defs::{
        BOOT_PATH,  ROOT_PATH,
    },
};

pub(crate) mod lsblk_info;
pub(crate) use lsblk_info::LsblkInfo;

pub(crate) mod label_type;

pub(crate) mod path_info;
pub(crate) use path_info::PathInfo;

#[derive(Debug)]
pub(crate) struct DeviceInfo {
    pub os_name: String,
    pub os_arch: OSArch,
    pub lsblk_info: LsblkInfo,
    pub root_path: PathInfo,
    pub boot_path: PathInfo,
    pub work_path: PathInfo,
    pub log_path: Option<(PathBuf, String)>,
}

// TODO: /etc path just in case

impl DeviceInfo {
    pub(crate) fn new(
        os_name: &str,
        os_arch: &OSArch,
        work_path: &Path,
        log_dev: Option<&Path>,
    ) -> Result<DeviceInfo, MigError> {
        // find the root device in lsblk output
        let lsblk_info = LsblkInfo::new()?;
        let root_path =
            if let Some(path_info) = PathInfo::new(ROOT_PATH, &lsblk_info)? {
                path_info
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        ROOT_PATH
                    ),
                ));
            };
        let boot_path =
            if let Some(path_info) = PathInfo::new(BOOT_PATH, &lsblk_info)? {
                path_info
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        BOOT_PATH
                    ),
                ));
            };
        let work_path =
            if let Some(path_info) = PathInfo::new(work_path, &lsblk_info)? {
                path_info
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        work_path.display()
                    ),
                ));
            };

        let log_path =
            if let Some(log_dev) = log_dev {
                let (_lsblk_drive, lsblk_part) = lsblk_info.get_devinfo_from_partition(log_dev)?;
                Some((
                    lsblk_part.get_path(),
                    if let Some(ref fs_type) = lsblk_part.fstype {
                        fs_type.clone()
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvState,
                            &format!("Log fstype was not initialized for '{}'", log_dev.display()),
                        ));
                    },
                ))
            } else {
                None
            };


        let result = DeviceInfo {
            os_name: String::from(os_name),
            os_arch: *os_arch.clone(),
            lsblk_info,
            root_path,
            boot_path,
            work_path,
            log_path,
        };

        debug!("Diskinfo: {:?}", result);

        Ok(result)
    }
}
