use std::rc::{Rc};
use log::{debug};
use crate::migrator::{
    MigError, 
    MigErrorKind,
    mswin::win_api::{ 
        query_dos_device,
        wmi_api::{WmiAPI},
    },
};
use super::{QueryRes, LogicalDrive, NS_CVIM2};

const MODULE: &str = "mswin::wmi_utils::partition";
// const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";        

#[derive(Debug)]
pub struct Partition {
    name: String,
    device_id: String,
    bootable: bool,
    size: u64,
    number_of_blocks: u64,
    ptype: String,
    boot_partition: bool,
    disk_index: u64,
    partition_index: u64,
    start_offset: u64,
    device: String,
}

impl<'a> Partition {
    pub(crate) fn new(disk_index: u64, res_map: QueryRes ) -> Result<Partition,MigError> {
        let partition_index = res_map.get_int_property("Index")? as u64;
        
        Ok(Partition { 
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
            device: String::from(query_dos_device(Some(&format!("Harddisk{}Partition{}",disk_index, partition_index + 1)))?.get(0).unwrap().as_ref()),
            bootable: res_map.get_bool_property("Bootable")?, 
            size: res_map.get_uint_property("Size")?,
            number_of_blocks: res_map.get_uint_property("NumberOfBlocks")?,
            ptype: String::from(res_map.get_string_property("Type")?), // TODO: parse this value GPT / System 
            boot_partition: res_map.get_bool_property("BootPartition")?,
            start_offset: res_map.get_uint_property("StartingOffset")?,            
            disk_index,
            partition_index,
        })
    }

    pub fn query_logical_drive(&self) -> Result<Option<LogicalDrive>, MigError> {
        let query = &format!("ASSOCIATORS OF {{Win32_DiskPartition.DeviceID='{}'}} WHERE AssocClass = Win32_LogicalDiskToPartition",self.device_id);
        debug!("{}::query_logical_drive: performing WMI Query: '{}'", MODULE, query);
        
        let mut q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        match q_res.len() {
            0 => Ok(None), 
            1 => {
                let res_map = QueryRes::new(q_res.pop().unwrap());
                Ok(Some(LogicalDrive::new(res_map)?))
            },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::query_logical_drive: invalid result cout for query, expected 1, got  {}",MODULE, q_res.len()))), 
        }
    }

    pub fn get_hd_index(&self) -> u64 {
        self.disk_index
    }

    pub fn get_part_index(&self) -> u64 {
        self.partition_index
    }

    pub fn is_boot_device(&self) -> bool {
        self.boot_partition
    }

    pub fn is_bootable(&self) -> bool {
        self.bootable
    }

    pub fn get_size(&self) -> u64 {
        self.size
    }

    pub fn get_num_blocks(&self) -> u64 {
        self.number_of_blocks
    }

    pub fn get_start_offset(&self) -> u64 {
        self.start_offset
    }

    pub fn get_name(&'a self) -> &'a str {
        &self.name
    }

    pub fn get_ptype(&'a self) -> &'a str {
        &self.ptype
    }

    pub fn get_device_id(&'a self) -> &'a str {
        &self.device_id
    }

    pub fn get_device(&'a self) -> &'a str {
        &self.device
    }

}
