use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::rc::Rc;
use log::{debug, warn};

use super::{DeviceProps, HarddiskVolumeInfo, PhysicalDriveInfo};

use crate::mswin::win_api::query_dos_device;
use crate::mswin::wmi_utils::{WmiUtils, WmiPartitionInfo};
use crate::MigError;

const MODULE: &str = "mswin::drive_info::hd_partition";

#[derive(Debug)]
pub struct HarddiskPartitionInfo {
    dev_name: String,
    hd_index: u64,
    part_index: u64,
    device: String,
    phys_disk: Option<Rc<PhysicalDriveInfo>>,
    hd_vol: Option<Rc<RefCell<HarddiskVolumeInfo>>>,
    wmi_info: Option<WmiPartitionInfo>,
}

impl<'a> HarddiskPartitionInfo {
    pub fn try_from_device(device: &str, wmi_utils: &WmiUtils) -> Result<Option<HarddiskPartitionInfo>, MigError> {
        lazy_static! {
            static ref RE_HDPART: Regex =
                Regex::new(r"^Harddisk([0-9]+)Partition([0-9]+)$").unwrap();
        }
        if let Some(cap) = RE_HDPART.captures(device) {
            Ok(Some(HarddiskPartitionInfo::new(
                device,
                cap.get(1).unwrap().as_str().parse::<u64>().unwrap(),
                cap.get(2).unwrap().as_str().parse::<u64>().unwrap(),
                wmi_utils,
            )?))
        } else {
            Ok(None)
        }
    }

    fn new(
        device: &str,
        hd_index: u64,
        part_index: u64,
        wmi_utils: &WmiUtils
    ) -> Result<HarddiskPartitionInfo, MigError> {
        // TODO: query WMI partition info
        
        let mut wmi_info: Option<WmiPartitionInfo> = None;
        match wmi_utils.get_partition_info(hd_index, part_index - 1) {
            Ok(pi) => { 
                debug!("{}::new: got WmiPartitionInfo: {:?}", MODULE, pi); 
                wmi_info = Some(pi);                
                },
            Err(why) => { warn!("{}::new: failed to get WmiPartitionInfo: {:?}", MODULE, why); },
        };
        
        Ok(HarddiskPartitionInfo {
            dev_name: String::from(device),
            hd_index,
            part_index,
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
            phys_disk: None,
            hd_vol: None,
            wmi_info: wmi_info,
        })
    }

    pub fn get_hd_index(&self) -> u64 {
        self.hd_index
    }

    pub fn get_part_index(&self) -> u64 {
        self.part_index
    }

    pub fn has_wmi_info(&self) -> bool {
        if let Some(ref _wi) = self.wmi_info {
            true
        } else {
            false
        }
    }

    pub fn is_boot_device(&self) -> Option<bool> {
        if let Some(ref wi) = self.wmi_info {
            Some(wi.boot_partition)
        } else {
            None
        }
    }

    pub fn is_bootable(&self) -> Option<bool> {
        if let Some(ref wi) = self.wmi_info {
            Some(wi.bootable)
        } else {
            None
        }
    }

    pub fn get_size(&self) -> Option<usize> {
        if let Some(ref wi) = self.wmi_info {
            Some(wi.size)
        } else {
            None
        }
    }

    pub fn get_num_blocks(&self) -> Option<usize> {
        if let Some(ref wi) = self.wmi_info {
            Some(wi.number_of_blocks)
        } else {
            None
        }
    }

    pub fn get_start_offset(&self) -> Option<usize> {
        if let Some(ref wi) = self.wmi_info {
            Some(wi.start_offset)
        } else {
            None
        }
    }

    pub fn get_name(&'a self) -> Option<&'a str> {
        if let Some(ref wi) = self.wmi_info {
            Some(&wi.name)
        } else {
            None
        }
    }

    pub fn get_ptype(&'a self) -> Option<&'a str> {
        if let Some(ref wi) = self.wmi_info {
            Some(&wi.ptype)
        } else {
            None
        }
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
