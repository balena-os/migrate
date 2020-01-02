use failure::ResultExt;
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    common::{file_exists, path_append, MigErrCtx, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
};

#[derive(Debug, Clone)]
pub(crate) struct Partition {
    pub name: String,
    pub kname: String,
    pub maj_min: String,
    pub ro: bool,
    pub uuid: Option<String>,
    pub fstype: Option<String>,
    pub mountpoint: Option<PathBuf>,
    pub label: Option<String>,
    pub part_table_type: String,
    pub part_entry_type: String,
    pub partuuid: Option<String>,
    pub size: u64,
    pub index: u16,
}

impl Partition {
    fn new(
        lsblk_result: &HashMap<String, String>,
        udev_result: &HashMap<String, String>,
    ) -> Result<Partition, MigError> {
        Ok(Partition {
            name: if let Some(val) = lsblk_result.get("NAME") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param NAME",
                ));
            },
            kname: if let Some(val) = lsblk_result.get("KNAME") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param KNAME",
                ));
            },
            maj_min: if let Some(val) = lsblk_result.get("MAJ:MIN") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param MAJ:MIN",
                ));
            },
            uuid: if let Some(val) = lsblk_result.get("UUID") {
                Some(val.clone())
            } else {
                None
            },
            size: if let Some(val) = lsblk_result.get("SIZE") {
                val.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("new: Failed to parse size from string: {}", val),
                ))?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param SIZE",
                ));
            },
            label: if let Some(val) = lsblk_result.get("LABEL") {
                Some(val.clone())
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param LABEL",
                ));
            },
            mountpoint: if let Some(val) = lsblk_result.get("MOUNTPOINT") {
                Some(PathBuf::from(val))
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param MOUNTPOINT",
                ));
            },
            fstype: if let Some(val) = lsblk_result.get("FSTYPE") {
                Some(val.clone())
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param FSTYPE",
                ));
            },
            ro: if let Some(val) = lsblk_result.get("RO") {
                val == "1"
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve lsblk_param RO",
                ));
            },
            part_table_type: if let Some(val) = lsblk_result.get("ID_PART_TABLE_TYPE") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve udev_param ID_PART_TABLE_TYPE",
                ));
            },
            part_entry_type: if let Some(val) = lsblk_result.get("ID_PART_ENTRY_TYPE") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve udev_param ID_PART_ENTRY_TYPE",
                ));
            },
            partuuid: if let Some(val) = udev_result.get("ID_FS_UUID") {
                Some(val.clone())
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve udev_param ID_FS_UUID",
                ));
            },
            index: if let Some(val) = udev_result.get("ID_PART_ENTRY_NUMBER") {
                val.parse::<u16>().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("new: failed to parse index from {}", val),
                ))?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "new: Failed to retrieve udev_param ID_PART_ENTRY_NUMBER",
                ));
            },
        })
    }

    pub fn get_path(&self) -> PathBuf {
        path_append("/dev", &self.name)
    }

    pub fn get_linux_path(&self) -> Result<PathBuf, MigError> {
        let dev_path = if let Some(ref uuid) = self.uuid {
            path_append(DISK_BY_UUID_PATH, uuid)
        } else {
            if let Some(ref partuuid) = self.partuuid {
                path_append(DISK_BY_PARTUUID_PATH, partuuid)
            } else {
                if let Some(ref label) = self.label {
                    path_append(DISK_BY_LABEL_PATH, label)
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!("No unique device path found for device: '{}'", self.name),
                    ));
                }
            }
        };
        if file_exists(&dev_path) {
            Ok(dev_path)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Could not locate device path: '{}'", dev_path.display()),
            ))
        }
    }
}
