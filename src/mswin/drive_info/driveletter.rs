use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::rc::Rc;

use super::{DeviceProps, HarddiskVolumeInfo};
use crate::mswin::win_api::query_dos_device;
use crate::MigError;

#[derive(Debug)]
pub struct DriveLetterInfo {
    dev_name: String,
    driveletter: String,
    device: String,    
}

impl<'a> DriveLetterInfo {
    pub fn try_from_device(device: &str) -> Result<Option<DriveLetterInfo>, MigError> {
        lazy_static! {
            static ref RE_DL: Regex = Regex::new(r"^([A-Z]):$").unwrap();
        }
        if let Some(cap) = RE_DL.captures(device) {
            Ok(Some(DriveLetterInfo::new(device,cap.get(1).unwrap().as_str())?))
        } else {
            Ok(None)
        }
    }

    fn new(device: &str, dl: &str) -> Result<DriveLetterInfo, MigError> {
        Ok(DriveLetterInfo {
            dev_name: String::from(device),
            driveletter: String::from(dl),
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
        })
    }

    pub fn get_driveletter(&'a self) -> &'a str {
        &self.driveletter
    }
}

impl DeviceProps for DriveLetterInfo {
    fn get_device_name<'a>(&'a self) -> &'a str {
        &self.dev_name
    }

    fn get_device<'a>(&'a self) -> &'a str {
        &self.device
    }
}
