use lazy_static::lazy_static;
use regex::Regex;

use crate::mswin::win_api::query_dos_device;
use crate::MigError;

#[derive(Debug)]
pub struct PhysicalDriveInfo {
    dev_name: String,
    index: u64,
    device: String,
}

impl<'a> PhysicalDriveInfo {
    pub fn try_from_device(device: &str) -> Result<Option<PhysicalDriveInfo>, MigError> {
        lazy_static! {
            static ref RE_PD: Regex = Regex::new(r"^PhysicalDrive([0-9]+)$").unwrap();
        }
        if let Some(cap) = RE_PD.captures(device) {
            Ok(Some(PhysicalDriveInfo::new(
                device,
                cap.get(1).unwrap().as_str().parse::<u64>().unwrap(),
            )?))
        } else {
            Ok(None)
        }
    }

    fn new(device: &str, index: u64) -> Result<PhysicalDriveInfo, MigError> {
        Ok(PhysicalDriveInfo {
            dev_name: String::from(device),
            index: index,
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
        })
    }

    pub fn get_dev_name(&'a self) -> &'a str {
        &self.dev_name
    }

    pub fn get_index(&self) -> u64 {
        self.index
    }

    pub fn get_device(&'a self) -> &'a str {
        &self.device
    }
}
