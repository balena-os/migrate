use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::rc::Rc;

use super::{DeviceProps, HarddiskVolumeInfo, PhysicalDriveInfo};

use crate::mswin::win_api::query_dos_device;
use crate::mswin::wmi_utils::{WmiUtils,WmiPartitionInfo};
use crate::MigError;


#[derive(Debug)]
pub struct HarddiskPartitionInfo {
    dev_name: String,
    hd_index: u64,
    part_index: u64,
    device: String,
    phys_disk: Option<Rc<PhysicalDriveInfo>>,
    hd_vol: Option<Rc<RefCell<HarddiskVolumeInfo>>>,
}

impl<'a> HarddiskPartitionInfo {
    pub fn try_from_device(device: &str) -> Result<Option<HarddiskPartitionInfo>, MigError> {
        lazy_static! {
            static ref RE_HDPART: Regex =
                Regex::new(r"^Harddisk([0-9]+)Partition([0-9]+)$").unwrap();
        }
        if let Some(cap) = RE_HDPART.captures(device) {
            Ok(Some(HarddiskPartitionInfo::new(
                device,
                cap.get(1).unwrap().as_str().parse::<u64>().unwrap(),
                cap.get(2).unwrap().as_str().parse::<u64>().unwrap(),
            )?))
        } else {
            Ok(None)
        }
    }

    fn new(
        device: &str,
        hd_index: u64,
        part_index: u64,
    ) -> Result<HarddiskPartitionInfo, MigError> {
        // TODO: query WMI partition info
        let part_info = WmiUtils::new()?.get_partition_info(hd_index, part_index)?;

        Ok(HarddiskPartitionInfo {
            dev_name: String::from(device),
            hd_index,
            part_index,
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
            phys_disk: None,
            hd_vol: None,
        })
    }

    pub fn get_hd_index(&self) -> u64 {
        self.hd_index
    }

    pub fn get_part_index(&self) -> u64 {
        self.part_index
    }

    pub fn get_phys_disk(&'a self) -> &'a Option<Rc<PhysicalDriveInfo>> {
        &self.phys_disk
    }

    pub fn get_hd_vol(&'a self) -> &'a Option<Rc<RefCell<HarddiskVolumeInfo>>> {
        &self.hd_vol
    }

    pub(crate) fn set_phys_disk(&mut self, pd: &Rc<PhysicalDriveInfo>) -> () {
        // TODO: what if it is already set ?
        self.phys_disk = Some(pd.clone())
    }

    pub(crate) fn set_hd_vol(&mut self, vol: &Rc<RefCell<HarddiskVolumeInfo>>) -> () {
        // TODO: what if it is already set ?
        self.hd_vol = Some(vol.clone())
    }
}

impl DeviceProps for HarddiskPartitionInfo {
    fn get_device_name<'a>(&'a self) -> &'a str {
        &self.dev_name
    }

    fn get_device<'a>(&'a self) -> &'a str {
        &self.device
    }
}
