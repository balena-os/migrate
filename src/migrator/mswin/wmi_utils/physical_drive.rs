use crate::{
    common::{MigError, MigErrorKind},
    mswin::win_api::{query_dos_device, wmi_api::WmiAPI},
};
use log::debug;

use super::{Partition, QueryRes, NS_CVIM2};

const MODULE: &str = "mswin::wmi_utils::physical_drive";
const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod, InterfaceType FROM Win32_DiskDrive";

#[derive(Debug, Clone)]
pub(crate) enum DriveType {
    Scsi,
    Ide,
    Other,  // TODO: find out what that looks like & define it
}

impl DriveType {
    pub fn from_str(val: &str) -> DriveType {
        match val.to_uppercase().as_ref() {
            "SCSI" => DriveType::Scsi,
            "IDE" => DriveType::Ide,
            _ => DriveType::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PhysicalDrive {
    name: String,
    device_id: String,
    size: u64,
    media_type: String,
    status: String,
    bytes_per_sector: i32,
    partitions: i32,
    compression_method: String,
    disk_index: usize,
    device: String,
    drive_type: DriveType,
}

impl<'a> PhysicalDrive {
    pub fn query_all() -> Result<Vec<PhysicalDrive>, MigError> {
        let query = QUERY_ALL;
        debug!("query_drives: performing WMI Query: '{}'", query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        let mut result: Vec<PhysicalDrive> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(&res);
            result.push(PhysicalDrive::new(res_map)?);
        }
        Ok(result)
    }

    pub fn by_index(disk_index: usize) -> Result<PhysicalDrive, MigError> {
        let query = format!("{} WHERE Index={}", QUERY_ALL, disk_index);
        debug!("get_drive: performing WMI Query: '{}'", query);
        let mut q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        match q_res.len() {
            0 => Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "get_drive: the query returned an empty result set: '{}'",
                    query
                ),
            )),
            1 => {
                let res = q_res.pop().unwrap();
                let res_map = QueryRes::new(&res);
                Ok(PhysicalDrive::new(res_map)?)
            }
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "get_drive_info: invalid result cout for query, expected 1, got  {}",
                    q_res.len()
                ),
            )),
        }
    }

    fn new(res_map: QueryRes) -> Result<PhysicalDrive, MigError> {
        let disk_index = res_map.get_int_property("Index")? as usize;
        Ok(PhysicalDrive {
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
            media_type: String::from(res_map.get_string_property("MediaType")?), // TODO: parse this value fixed / removable
            size: res_map.get_uint_property("Size")?,
            status: String::from(res_map.get_string_property("Status")?),
            bytes_per_sector: res_map.get_int_property("BytesPerSector")?,
            partitions: res_map.get_int_property("Partitions")?,
            compression_method: String::from(res_map.get_string_property("CompressionMethod")?),
            drive_type: DriveType::from_str(res_map.get_string_property("InterfaceType")?),
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

    pub fn get_index(&self) -> usize {
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

    pub fn get_drive_type(&'a self) -> &'a DriveType {
        &self.drive_type
    }
}
