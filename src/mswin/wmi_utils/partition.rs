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