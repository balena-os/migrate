use failure::ResultExt;
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    common::{file_exists, path_append, MigErrCtx, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
};

#[derive(Debug, Clone)]
pub(crate) struct BlockDevice {
    pub name: String,
    pub kname: String,
    pub maj_min: String,
    pub uuid: Option<String>,
    pub size: u64,
    pub children: Option<Vec<LsblkPartition>>,
}

impl<'a> LsblkDevice {
    pub fn from_device_path<P: AsRef<Path>>(device: P) -> Result<LsblkDevice, MigError> {
        debug!("lsblk_device_from_device: {}", device.as_ref().display());

        let mut lsblk_results = call_lsblk(Some(&*device.as_ref().to_string_lossy()))?;
        if let Some(lsblk_result) = lsblk_results.pop() {
            let mut lsblk_device: LsblkDevice =
                LsblkDevice::new(&lsblk_result, &call_udevadm(device)?)?;
            // add partitions
            for lsblk_result in lsblk_results {
                if let Some(dev_name) = lsblk_result.get("NAME") {
                    let udev_result = LsblkInfo::call_udevadm(&dev_name)?;
                    if let Some(dev_type) = udev_result.get("DEVTYPE") {
                        match dev_type.as_str() {
                            "partition" => {
                                let partition = Partition::new(&lsblk_result, &udev_result)?;
                                if let Some(ref mut children) = lsblk_device.children {
                                    children.push(partition)
                                } else {
                                    let mut children: Vec<LsblkPartition> = Vec::new();
                                    children.push(partition);
                                    lsblk_device.children = Some(children)
                                }
                            },
                            _ => Err(MigError::from_remark(MigErrorKind::InvParam,
                                                           &format!("lsblk_device:from_device: invalid device type, expected partition, got: '{}'", dev_type))),
                        }
                    }
                } else {
                    Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        "lsblk_device:from_dev_path: Failed to retrieved udevadm parameter DEVTYPE",
                    ))
                }
            }
            Ok(lsblk_device)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                "lsblk_device:from_dev_path: No result from call_lsblk",
            ))
        }
    }

    fn new(
        lsblk_result: &HashMap<String, String>,
        udevadm_params: &HashMap<String, String>,
    ) -> Result<LsblkDevice, MigError> {
        Ok(LsblkDevice {
            // lsblk params: NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,SIZE
            name: if let Some(val) = lsblk_result.get("NAME") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "LsblkDevice:new: Failed to retrieve lsblk_param NAME",
                ));
            },
            kname: if let Some(val) = lsblk_result.get("KNAME") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "LsblkDevice:new: Failed to retrieve lsblk_param KNAME",
                ));
            },
            maj_min: if let Some(val) = lsblk_result.get("MAJ:MIN") {
                val.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "LsblkDevice:new: Failed to retrieve lsblk_param MAJ:MIN",
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
                    &format!("LsblkDevice:new: Failed to parse size from string: {}", val),
                ))?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "LsblkDevice:new: Failed to retrieve lsblk_param SI",
                ));
            },
            children: None,
        })
    }

    pub fn get_devinfo_from_part_name(
        &'a self,
        part_name: &str,
    ) -> Result<&'a LsblkPartition, MigError> {
        if let Some(ref children) = self.children {
            if let Some(part_info) = children.iter().find(|&part| part.name == part_name) {
                Ok(part_info)
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "The partition was not found in lsblk output '{}'",
                        part_name
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("The device was not found in lsblk output '{}'", part_name),
            ))
        }
    }

    pub fn get_path(&self) -> PathBuf {
        PathBuf::from(&format!("/dev/{}", self.name))
    }
}
