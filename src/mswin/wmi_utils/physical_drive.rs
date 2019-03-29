use std::rc::{Rc};

use log::{debug};
use crate::mig_error::{MigError, MigErrorKind};
use crate::mswin::win_api::wmi_api::{WmiAPI};
use super::{QueryRes, Partition};

const MODULE: &str = "mswin::wmi_utils::physical_drive";
const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";        

#[derive(Debug)]
pub struct PhysicalDrive {
    wmi_api: Rc<WmiAPI>,
    pub name: String,
    pub device_id: String,
    pub size: u64,
    pub media_type: String,
    pub status: String,    
    pub bytes_per_sector: i32,
    pub partitions: i32,
    pub compression_method: String,
    pub disk_index: u64,
}

impl PhysicalDrive {
    pub(crate) fn get_query_all() -> &'static str {
        QUERY_ALL
    }

    pub(crate) fn get_query_by_index(index: u64) -> String {
        format!("SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive WHERE Index={}",index)
    }

    pub(crate) fn new(wmi_api: Rc<WmiAPI>, res_map: QueryRes) -> Result<PhysicalDrive,MigError> {
        Ok(PhysicalDrive{            
            wmi_api, 
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
            media_type: String::from(res_map.get_string_property("MediaType")?),  // TODO: parse this value fixed / removable
            size: res_map.get_uint_property("Size")?,
            status: String::from(res_map.get_string_property("Status")?),
            bytes_per_sector: res_map.get_int_property("BytesPerSector")?,
            partitions: res_map.get_int_property("Partitions")?,
            compression_method: String::from(res_map.get_string_property("CompressionMethod")?),
            disk_index: res_map.get_int_property("Index")? as u64,
        })
    }

    pub fn query_partitions(&mut self) -> Result<Vec<Partition>, MigError> {
        let query = &format!("ASSOCIATORS OF {{Win32_DiskDrive.DeviceID='{}'}} WHERE AssocClass = Win32_DiskDriveToDiskPartition",self.device_id);
        debug!("{}::query_partitions: performing WMI Query: '{}'", MODULE, query);
        let q_res = self.wmi_api.raw_query(query)?;
/*        let mut result: Vec<Partition> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(res);
            result.push(PhysicalDrive::new(self.wmi_api.clone(), res_map)?);
        }
        Ok(result)
*/
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}