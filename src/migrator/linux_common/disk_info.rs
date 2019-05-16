use failure::ResultExt;
use log::info;
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_json;
use std::fs::{read_to_string, File};
use std::path::{Path, PathBuf};

use crate::{
    common::{MigErrCtx, MigError, MigErrorKind},
    defs::KERNEL_CMDLINE_PATH,
    linux_common::{call_cmd, get_root_info, path_info::PathInfo, FDISK_CMD, LSBLK_CMD},
};

pub(crate) mod lsblk_info;
pub(crate) use lsblk_info::{LsblkDevice, LsblkInfo, LsblkPartition};

pub(crate) mod part_label_type;
pub(crate) use part_label_type::PartLabelType;

const GPT_EFI_PART: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";

const DISK_LABEL_REGEX: &str = r#"^Disklabel type:\s*(\S+)$"#;

pub(crate) struct DiskInfo {
    pub drive_dev: PathBuf,
    pub drive_size: u64,
    pub part_label_type: PartLabelType,
    pub drive_uuid: String,
    pub root_path: Option<PathInfo>,
    pub boot_path: Option<PathInfo>,
    pub efi_path: Option<PathInfo>,
    pub work_path: Option<PathInfo>,
    pub log_path: Option<PathInfo>,
}

impl DiskInfo {
    pub(crate) fn new(efi_boot: bool, work_path: &Path) -> Result<DiskInfo, MigError> {
        // Start with root from kernel command line
        let (root_device, root_fs_type) = get_root_info()?;

        let root_dev_name = if let Some(name) = root_device.file_name() {
            name.to_string_lossy()
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "new: failed to block device name from path '{}'",
                    root_device.display()
                ),
            ));
        };

        // find the root device in lsblk output
        let (root_dev, root_part) = LsblkInfo::new()?.get_devinfo_from_partition(&root_device)?;

        // check mountpoint
        if let Some(ref mountpoint) = root_part.mountpoint {
            if mountpoint != "/" {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "new: expected mountpoint '/' but found '{}' in lsblk output for root partition device '{}'  ",
                        mountpoint,
                        root_device.display(),
                    ),
                ));
            }
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "new: expected mountpoint '/' not found in lsblk output for root partition device '{}'  ",
                    root_device.display()
                ),
            ));
        }

        info!("Found root device '{}'", root_part.name,);

        // establish partition table type
        let root_dev_path = format!("/dev/{}", root_dev.name);

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    pub(crate) fn default() -> DiskInfo {
        DiskInfo {
            drive_dev: PathBuf::from(""),
            drive_uuid: String::from(""),
            drive_size: 0,
            part_label_type: PartLabelType::OTHER,
            root_path: None,
            boot_path: None,
            efi_path: None,
            log_path: None,
            work_path: None,
        }
    }
}
