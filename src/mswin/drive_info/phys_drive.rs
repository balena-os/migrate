use lazy_static::lazy_static;
use regex::Regex;


use crate::MigError;
use crate::mswin::{
    wmi_utils::{WmiDriveInfo, WmiUtils},
    win_api::query_dos_device,
    MSWMigrator,
};

#[derive(Debug)]
pub struct PhysicalDriveInfo {
    dev_name: String,
    index: u64,
    device: String,
    wmi_info: WmiDriveInfo,
}

impl<'a> PhysicalDriveInfo {
    pub(crate) fn try_from_device(device: &str, migrator: &MSWMigrator) -> Result<Option<PhysicalDriveInfo>, MigError> {
        lazy_static! {
            static ref RE_PD: Regex = Regex::new(r"^PhysicalDrive([0-9]+)$").unwrap();
        }
        if let Some(cap) = RE_PD.captures(device) {
            Ok(Some(PhysicalDriveInfo::new(
                device,
                cap.get(1).unwrap().as_str().parse::<u64>().unwrap(),
                migrator
            )?))
        } else {
            Ok(None)
        }
    }

    fn new(device: &str, index: u64, migrator: &MSWMigrator) -> Result<PhysicalDriveInfo, MigError> {
        Ok(PhysicalDriveInfo {
            dev_name: String::from(device),
            index: index,
            device: query_dos_device(Some(device))?.get(0).unwrap().clone(),
            wmi_info: migrator.get_wmi_utils().get_drive_info(index)?,
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

    pub fn get_size(&self) -> u64 {
        self.wmi_info.size
    }

    pub fn get_partitions(&self) -> i32 {
        self.wmi_info.partitions
    }

    pub fn get_bytes_per_sector(&self) -> i32 {
        self.wmi_info.bytes_per_sector
    }

    pub fn get_status(&'a self) -> &'a str {
        &self.wmi_info.status
    }

    pub fn get_media_type(&'a self) -> &'a str {
        &self.wmi_info.media_type
    }

    pub fn get_compression_method(&'a self) -> &'a str {
        &self.wmi_info.compression_method
    }

    pub fn get_wmi_name(&'a self) -> &'a str {
        &self.wmi_info.name
    }

}
