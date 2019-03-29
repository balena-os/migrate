use std::rc::{Rc};

use crate::mig_error::{MigError, MigErrorKind};
use crate::mswin::win_api::wmi_api::{WmiAPI};
use super::{QueryRes};

const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";        

#[derive(Debug)]
pub struct Partition {
    wmi_api: Rc<WmiAPI>,
    pub name: String,
    pub device_id: String,
    pub bootable: bool,
    pub size: u64,
    pub number_of_blocks: u64,
    pub ptype: String,
    pub boot_partition: bool,
    pub disk_index: u64,
    pub partition_index: u64,
    pub start_offset: u64,
}

impl Partition {
    pub(crate) fn new(wmi_api: &Rc<WmiAPI>, disk_index: u64, res_map: QueryRes ) -> Result<Partition,MigError> {
        Ok(Partition { 
            wmi_api: wmi_api.clone(),
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
            bootable: res_map.get_bool_property("Bootable")?, 
            size: res_map.get_uint_property("Size")?,
            number_of_blocks: res_map.get_uint_property("NumberOfBlocks")?,
            ptype: String::from(res_map.get_string_property("Type")?), // TODO: parse this value GPT / System 
            boot_partition: res_map.get_bool_property("BootPartition")?,
            start_offset: res_map.get_uint_property("StartingOffset")?,
            disk_index,
            partition_index: res_map.get_int_property("Index")? as u64,
        })
    }
}