use std::rc::{Rc};

use log::{debug};
use crate::mig_error::{MigError, MigErrorKind};
use crate::mswin::win_api::wmi_api::{WmiAPI};
use super::{QueryRes, Partition};

const MODULE: &str = "mswin::wmi_utils::physical_drive";
const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";        

query = "ASSOCIATORS OF {Win32_DiskPartition.DeviceID='" + partition.DeviceID + "'} WHERE AssocClass = Win32_LogicalDiskToPartition"

#[derive(Debug)]
pub struct LogicalDrive {
    wmi_api: Rc<WmiAPI>,
    pub name: String,
    pub device_id: String,
}

impl LogicalDrive {
    pub(crate) fn new(wmi_api: &Rc<WmiAPI>, res_map: QueryRes ) -> Result<Partition,MigError> {
        Ok(LogicalDrive { 
            wmi_api: wmi_api.clone(),
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
        })
    }
}
