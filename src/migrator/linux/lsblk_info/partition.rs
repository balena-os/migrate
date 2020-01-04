use std::path::{Path, PathBuf};

use crate::linux::lsblk_info::ResultParams;
use crate::{
    common::{file_exists, path_append, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::lsblk_info::{call_lsblk_for, call_udevadm},
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
    pub fn new(lsblk_result: &ResultParams) -> Result<Partition, MigError> {
        let name = format!("/dev/{}", lsblk_result.get_str("NAME")?);
        let udev_result = call_udevadm(&name)?;
        Ok(Partition {
            name: String::from(lsblk_result.get_str("NAME")?),
            kname: String::from(lsblk_result.get_str("KNAME")?),
            maj_min: String::from(lsblk_result.get_str("MAJ:MIN")?),
            uuid: lsblk_result.get_opt_str("UUID"),
            size: lsblk_result.get_u64("SIZE")?,
            label: lsblk_result.get_opt_str("LABEL"),
            mountpoint: lsblk_result.get_opt_pathbuf("MOUNTPOINT"),
            fstype: lsblk_result.get_opt_str("FSTYPE"),
            ro: lsblk_result.get_str("RO")? == "1",
            part_table_type: String::from(udev_result.get_str("ID_PART_TABLE_TYPE")?),
            part_entry_type: String::from(udev_result.get_str("ID_PART_ENTRY_TYPE")?),
            partuuid: udev_result.get_opt_str("ID_FS_UUID"),
            index: udev_result.get_u16("ID_PART_ENTRY_NUMBER")?,
        })
    }

    pub fn from_path<P: AsRef<Path>>(partition: P) -> Result<Partition, MigError> {
        let lsblk_results = call_lsblk_for(&partition)?;
        // expect just one result of type partition
        if lsblk_results.len() == 1 {
            let udev_result = call_udevadm(&partition)?;
            match udev_result.get_str("DEVTYPE")? {
                "partition" => Ok(Partition::new(&lsblk_results[0])?),
                _ => Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "call_lsblk_for_part: invalid device type, expected partition, got: '{}'",
                        udev_result.get_str("DEVTYPE")?
                    ),
                )),
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "call_lsblk_for_part: Invalid number of lsblk results encountered: {}",
                    lsblk_results.len()
                ),
            ))
        }
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
