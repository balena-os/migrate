use std::rc::{Rc};
use std::cell::{RefCell};
use lazy_static::lazy_static;
use regex::{Regex};

use super::{HarddiskVolumeInfo, DeviceProps};
use crate::{MigError};
use crate::mswin::win_api::{query_dos_device};


#[derive(Debug)]
pub struct DriveLetterInfo {
    dev_name: String,
    device: String,
    hd_vol: Option<Rc<RefCell<HarddiskVolumeInfo>>>
}

impl<'a> DriveLetterInfo {
    pub fn try_from_device(device: &str) -> Result<Option<DriveLetterInfo>,MigError> {
        lazy_static! {
            static ref RE_DL: Regex = Regex::new(r"^([A-Z]:)$").unwrap();
        }
        if let Some(cap) = RE_DL.captures(device) {
            Ok(Some(DriveLetterInfo::new(device)?))
        } else {
            Ok(None)
        }
    }

    fn new(device: &str) -> Result<DriveLetterInfo,MigError> {
        Ok(DriveLetterInfo{
            dev_name: String::from(device),                                    
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
            hd_vol: None})
   }

    pub(crate) fn set_hd_vol(&mut self, vol: & Rc<RefCell<HarddiskVolumeInfo>>) -> () {
        // TODO: what if it is already set ?
        self.hd_vol = Some(vol.clone())
    }

    pub fn get_hd_vol(&'a self) -> &'a Option<Rc<RefCell<HarddiskVolumeInfo>>> {
        &self.hd_vol
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
