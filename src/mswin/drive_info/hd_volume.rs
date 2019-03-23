use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::fmt::{self, Debug};
use std::rc::{Rc, Weak};

use super::{DeviceProps, HarddiskPartitionInfo};
use crate::mswin::win_api::query_dos_device;
use crate::MigError;

//use crate::mswin::drive_info::hd_partition::HarddiskPartitionInfo;

pub struct HarddiskVolumeInfo {
    dev_name: String,
    index: u64,
    device: String,
    hd_part: Option<Weak<RefCell<HarddiskPartitionInfo>>>,
}

impl<'a> HarddiskVolumeInfo {
    pub fn try_from_device(device: &str) -> Result<Option<HarddiskVolumeInfo>, MigError> {
        lazy_static! {
            static ref RE_HDVOL: Regex = Regex::new(r"^HarddiskVolume([0-9]+)$").unwrap();
        }
        if let Some(cap) = RE_HDVOL.captures(device) {
            Ok(Some(HarddiskVolumeInfo::new(
                device,
                cap.get(1).unwrap().as_str().parse::<u64>().unwrap(),
            )?))
        } else {
            Ok(None)
        }
    }

    fn new(device: &str, index: u64) -> Result<HarddiskVolumeInfo, MigError> {
        Ok(HarddiskVolumeInfo {
            dev_name: String::from(device),
            index: index,
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
            hd_part: None,
        })
    }

    pub fn get_index(&self) -> u64 {
        self.index
    }

    pub fn get_hd_part(&'a self) -> &'a Option<Weak<RefCell<HarddiskPartitionInfo>>> {
        &self.hd_part
    }

    pub(crate) fn set_hd_part(&mut self, part: &Rc<RefCell<HarddiskPartitionInfo>>) -> () {
        // TODO: what if it is already set ?
        self.hd_part = Some(Rc::downgrade(part))
    }
}

// need this to break infinite cycle introduced by weak backref to hdpart
impl Debug for HarddiskVolumeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut dep_dev = String::from("None");
        if let Some(hdp) = &self.hd_part {
            if let Some(hdp) = hdp.upgrade() {
                dep_dev = format!(
                    "HarddiskPartition({},{})",
                    hdp.as_ref().borrow().get_hd_index(),
                    hdp.as_ref().borrow().get_part_index()
                )
            } else {
                // consider error
                dep_dev = String::from("invalid");
            }
        }
        write!(
            f,
            "HarddiskVolumeInfo {{ dev_name: {}, index: {}, device: {}, hdpart: {} }}",
            self.dev_name, self.index, self.device, dep_dev
        )
    }
}

impl DeviceProps for HarddiskVolumeInfo {
    fn get_device_name<'a>(&'a self) -> &'a str {
        &self.dev_name
    }

    fn get_device<'a>(&'a self) -> &'a str {
        &self.device
    }
}
