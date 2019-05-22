use failure::ResultExt;
use log::debug;
use std::path::{Path, PathBuf};

use crate::{
    common::{BootType, MigErrCtx, MigError, MigErrorKind},
    defs::{
        BOOT_PATH, EFI_PATH, MLO_FILE_NAME, NIX_NONE, ROOT_PATH, UBOOT_FILE_NAME, UENV_FILE_NAME,
    },
    linux_common::{call_cmd, MKTEMP_CMD},
};

pub(crate) mod lsblk_info;
pub(crate) use lsblk_info::LsblkInfo;

pub(crate) mod label_type;

pub(crate) mod path_info;
pub(crate) use path_info::PathInfo;

use crate::common::{file_exists, path_append};
use nix::mount::{mount, umount, MsFlags};

#[derive(Debug)]
pub(crate) struct DiskInfo {
    pub root_path: PathInfo,
    // TODO: make boot_path Option<PathInfo> ?
    pub boot_path: PathInfo,
    pub bootmgr_path: Option<PathInfo>,
    pub work_path: PathInfo,
    pub log_path: Option<(PathBuf, String)>,
}

impl DiskInfo {
    pub(crate) fn new(
        boot_type: &BootType,
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
            bootmgr_path: match boot_type {
                BootType::EFI => {
                    // TODO: this is EFI specific stuff in a non EFI specific place - try to concentrate uboot / EFI stuff in dedicated module
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
                }
                BootType::UBoot => DiskInfo::get_uboot_mgr_path(&work_path, &lsblk_info)?,
                _ => None,
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

    // TODO: this is uboot specific stuff in a non uboot specific place - try to concentrate uboot / EFI stuff in dedicated module
    // Try to find a drive containing MLO, uEnv.txt or u-boot.bin, mount it if necessarry and return PathInfo if found
    fn get_uboot_mgr_path(work_path: &Path, lsblk_info: &LsblkInfo) -> Result<Option<PathInfo>, MigError> {
        let (root_dev, root_part) = lsblk_info.get_path_info(ROOT_PATH)?;

        let mut tmp_mountpoint: Option<PathBuf> = None;

        if let Some(ref children) = root_dev.children {
            for partition in children {
                if let Some(ref fstype) = partition.fstype {
                    if fstype == "vfat" || fstype.starts_with("ext") {
                        let mut mounted = false;
                        let mountpoint = match partition.mountpoint {
                            Some(ref mountpoint) => mountpoint,
                            None => {
                                // darn ! we will have to mount it
                                if let None = tmp_mountpoint {
                                    let cmd_res = call_cmd(MKTEMP_CMD, &["-d", "-p", &work_path.to_string_lossy()], true)?;
                                    if cmd_res.status.success() {
                                        tmp_mountpoint = Some(
                                            PathBuf::from(&cmd_res.stdout)
                                            .canonicalize()
                                                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to canonicalize path to mountpoint '{}'", cmd_res.stdout)))?);
                                    } else {
                                        return Err(MigError::from_remark(
                                            MigErrorKind::Upstream,
                                            "Failed to create temporary mount point",
                                        ));
                                    }
                                }

                                let mountpoint = tmp_mountpoint.as_ref().unwrap();

                                mount(
                                    Some(&partition.get_path()),
                                    mountpoint,
                                    NIX_NONE, // Some(fstype),
                                    MsFlags::empty(),
                                    NIX_NONE,
                                )
                                .context(
                                    MigErrCtx::from_remark(
                                        MigErrorKind::Upstream,
                                        &format!(
                                            "Failed to temporarilly mount drive '{}' on '{}",
                                            partition.get_path().display(),
                                            tmp_mountpoint.as_ref().unwrap().display()
                                        ),
                                    ),
                                )?;

                                mounted = true;
                                mountpoint
                            }
                        };

                        if file_exists(path_append(mountpoint, MLO_FILE_NAME))
                            || file_exists(path_append(mountpoint, UENV_FILE_NAME))
                            || file_exists(path_append(mountpoint, UBOOT_FILE_NAME))
                        {
                            return Ok(Some(PathInfo::from_mounted(
                                mountpoint, &root_dev, &root_part,
                            )?));
                        }

                        if mounted {
                            umount(mountpoint).context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("Failed to unmount '{}'", mountpoint.display()),
                            ))?;
                        }
                    }
                }
            }

            Ok(None)
        } else {
            panic!("root drive must have children");
        }
    }
}
