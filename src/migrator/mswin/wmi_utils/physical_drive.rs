use std::rc::Rc;

use crate::{
    mswin::win_api::{query_dos_device, wmi_api::WmiAPI},
    common::{MigError, },
};
use log::debug;

use super::{Partition, QueryRes, NS_CVIM2};

const MODULE: &str = "mswin::wmi_utils::physical_drive";
const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";

#[derive(Debug)]
pub struct PhysicalDrive {
    name: String,
    device_id: String,
    size: u64,
    media_type: String,
    status: String,
    bytes_per_sector: i32,
    partitions: i32,
    compression_method: String,
    disk_index: u64,
    device: String,
}

impl<'a> PhysicalDrive {
    pub(crate) fn get_query_all() -> &'static str {
        QUERY_ALL
    }

    pub(crate) fn get_query_by_index(index: u64) -> String {
        format!("SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive WHERE Index={}",index)
    }

    pub(crate) fn new(res_map: QueryRes) -> Result<PhysicalDrive, MigError> {
        let disk_index = res_map.get_int_property("Index")? as u64;
        Ok(PhysicalDrive {
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
            media_type: String::from(res_map.get_string_property("MediaType")?), // TODO: parse this value fixed / removable
            size: res_map.get_uint_property("Size")?,
            status: String::from(res_map.get_string_property("Status")?),
            bytes_per_sector: res_map.get_int_property("BytesPerSector")?,
            partitions: res_map.get_int_property("Partitions")?,
            compression_method: String::from(res_map.get_string_property("CompressionMethod")?),
            disk_index,
            device: String::from(
                query_dos_device(Some(&format!("PhysicalDrive{}", disk_index)))?
                    .get(0)
                    .unwrap()
                    .as_ref(),
            ),
        })
    }

    pub fn query_partitions(&self) -> Result<Vec<Partition>, MigError> {
        let query = &format!("ASSOCIATORS OF {{Win32_DiskDrive.DeviceID='{}'}} WHERE AssocClass = Win32_DiskDriveToDiskPartition",self.device_id);
        debug!(
            "{}::query_partitions: performing WMI Query: '{}'",
            MODULE, query
        );

        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        let mut result: Vec<Partition> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(&res);
            result.push(Partition::new(self.disk_index, res_map)?);
        }
        Ok(result)
    }

    pub fn get_device_id(&'a self) -> &'a str {
        &self.device_id
    }

    pub fn get_device(&'a self) -> &'a str {
        &self.device
    }

    pub fn get_index(&self) -> u64 {
        self.disk_index
    }

    pub fn get_size(&self) -> u64 {
        self.size
    }

    pub fn get_partitions(&self) -> i32 {
        self.partitions
    }

    pub fn get_bytes_per_sector(&self) -> i32 {
        self.bytes_per_sector
    }

    pub fn get_status(&'a self) -> &'a str {
        &self.status
    }

    pub fn get_media_type(&'a self) -> &'a str {
        &self.media_type
    }

    pub fn get_compression_method(&'a self) -> &'a str {
        &self.compression_method
    }

    pub fn get_wmi_name(&'a self) -> &'a str {
        &self.name
    }
}
