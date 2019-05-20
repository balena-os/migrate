use log::debug;
use std::path::{Path, PathBuf};

use crate::{
    common::{MigError, MigErrorKind},
    defs::{BOOT_PATH, EFI_PATH, ROOT_PATH},
};

pub(crate) mod lsblk_info;
pub(crate) use lsblk_info::LsblkInfo;

pub(crate) mod label_type;

pub(crate) mod path_info;
pub(crate) use path_info::PathInfo;

#[derive(Debug)]
pub(crate) struct DiskInfo {
    pub root_path: PathInfo,
    pub boot_path: PathInfo,
    pub efi_path: Option<PathInfo>,
    pub work_path: PathInfo,
    pub log_path: Option<(PathBuf, String)>,
}

impl DiskInfo {
    pub(crate) fn new(
        efi_boot: bool,
        work_path: &Path,
        log_dev: Option<&Path>,
    ) -> Result<DiskInfo, MigError> {
        // find the root device in lsblk output
        let lsblk_info = LsblkInfo::new()?;

        let result = DiskInfo {
            root_path: if let Some(path_info) = PathInfo::new(ROOT_PATH, &lsblk_info)? {
                path_info
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        ROOT_PATH
                    ),
                ));
            },
            boot_path: if let Some(path_info) = PathInfo::new(BOOT_PATH, &lsblk_info)? {
                path_info
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        BOOT_PATH
                    ),
                ));
            },
            work_path: if let Some(path_info) = PathInfo::new(work_path, &lsblk_info)? {
                path_info
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        work_path.display()
                    ),
                ));
            },
            efi_path: if efi_boot {
                if let Some(path_info) = PathInfo::new(EFI_PATH, &lsblk_info)? {
                    Some(path_info)
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!(
                            "the device for path '{}' could not be established",
                            EFI_PATH
                        ),
                    ));
                }
            } else {
                None
            },
            // TODO: take care of log path or discard the option
            log_path: if let Some(log_dev) = log_dev {
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
            },
        };

        debug!("Diskinfo: {:?}", result);

        Ok(result)
    }
}
