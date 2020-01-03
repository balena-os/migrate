use log::debug;
use std::path::{Path, PathBuf};

use crate::linux::lsblk_info::ResultParams;
use crate::{
    common::{MigError, MigErrorKind},
    linux::lsblk_info::{call_lsblk_for, call_udevadm, partition::Partition},
};

#[derive(Debug, Clone)]
pub(crate) struct BlockDevice {
    pub name: String,
    pub kname: String,
    pub maj_min: String,
    pub uuid: Option<String>,
    pub size: u64,
    pub children: Option<Vec<Partition>>,
}

impl<'a> BlockDevice {
    pub fn from_device_path<P: AsRef<Path>>(device: P) -> Result<BlockDevice, MigError> {
        debug!("lsblk_device_from_device: {}", device.as_ref().display());

        let lsblk_results = call_lsblk_for(&device)?;
        if let Some(lsblk_result) = lsblk_results.get(0) {
            let mut lsblk_device: BlockDevice = BlockDevice::new(&lsblk_result)?;
            // add partitions
            for lsblk_result in lsblk_results.iter().skip(1) {
                let dev_name = lsblk_result.get_str("NAME")?;
                let udev_result = call_udevadm(&dev_name)?;
                match udev_result.get_str("DEVTYPE")? {
                    "partition" => {
                        let partition = Partition::new(&lsblk_result, &udev_result)?;
                        if let Some(ref mut children) = lsblk_device.children {
                            children.push(partition)
                        } else {
                            let mut children: Vec<Partition> = Vec::new();
                            children.push(partition);
                            lsblk_device.children = Some(children)
                        }
                    }
                    _ => {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                            "from_device_path: invalid device type, expected partition, got: '{}'",
                            udev_result.get_str("DEVTYPE")?
                        ),
                        ))
                    }
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

    pub fn new(lsblk_result: &ResultParams) -> Result<BlockDevice, MigError> {
        Ok(BlockDevice {
            // lsblk params: NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,SIZE
            name: String::from(lsblk_result.get_str("NAME")?),
            kname: String::from(lsblk_result.get_str("KNAME")?),
            maj_min: String::from(lsblk_result.get_str("MAJ:MIN")?),
            uuid: lsblk_result.get_opt_str("UUID"),
            size: lsblk_result.get_u64("SIZE")?,
            children: None,
        })
    }

    pub fn get_devinfo_from_part_name(
        &'a self,
        part_name: &str,
    ) -> Result<&'a Partition, MigError> {
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
