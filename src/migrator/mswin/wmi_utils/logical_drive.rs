use log::debug;
use std::rc::Rc;
// use log::{debug};
use super::QueryRes;
use crate::{
    common::{MigError, MigErrorKind},
    mswin::{win_api::wmi_api::WmiAPI, MSWMigrator},
};
use regex::Regex;

use crate::{ mswin::wmi_utils::NS_CVIM2 };

const MODULE: &str = "mswin::wmi_utils::logical_drive";
// const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";
const QUERY_BASE: &str = "SELECT Caption, DeviceID, Compressed, FileSystem, MediaType, Size, FreeSpace, VolumeDirty, Status FROM Win32_LogicalDisk";

#[derive(Debug)]
pub enum MediaType {
    UNKNOWN,
    FIXED_MEDIA,
    REMOVABLE_MEDIA,
    REMOVABLE_FLOPPY,
}

impl MediaType {
    pub fn from_int(value: i32) -> MediaType {
        match value {
            0 => MediaType::UNKNOWN,
            11 => MediaType::REMOVABLE_MEDIA,
            12 => MediaType::FIXED_MEDIA,
            _ => {
                if ((value > 0) && (value < 11)) || (value > 12 && value < 22) {
                    MediaType::REMOVABLE_FLOPPY
                } else {
                    MediaType::UNKNOWN
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct LogicalDrive {
    name: String,
    device_id: String,
    status: String,
    media_type: MediaType,
    file_system: String,
    size: u64,
    free_space: u64,
    compressed: bool,
    dirty: bool,
}

impl<'a> LogicalDrive {
    pub(crate) fn query_drive_letters() -> Result<Vec<String>, MigError> {
        let mut result: Vec<String> = Vec::new();
        for log_drive in LogicalDrive::query_all()? {
            result.push(String::from(log_drive.get_name()));
        }
        Ok(result)
    }

    pub(crate) fn query_for_name(name: &str) -> Result<LogicalDrive, MigError> {
        let query = format!("{} where Name='{}'", QUERY_BASE, name);
        debug!(
            "query_drive_for_name: performing WMI Query: '{}'",
            query
        );

        let mut q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        if q_res.len() > 0 {
            Ok(LogicalDrive::new(QueryRes::new(&q_res[0]))?)
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound, &format!("received an empty result lookin for logical dribe: '{}'", name)))
        }
    }

    pub(crate) fn query_all() -> Result<Vec<LogicalDrive>, MigError> {
        let query = QUERY_BASE;
        debug!(
            "query_all: performing WMI Query: '{}'",
            query
        );

        let mut q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        let mut result: Vec<LogicalDrive> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(&res);
            result.push(LogicalDrive::new(res_map)?);
        }
        Ok(result)
    }

    pub(crate) fn new(res_map: QueryRes) -> Result<LogicalDrive, MigError> {
        debug!("{}::new: creating logical drive", MODULE);

        for key in res_map.q_result.keys() {
            debug!(
                "{}::new: res_map key: {}: {:?}",
                MODULE,
                key,
                res_map.q_result.get(key).unwrap()
            );
        }

        Ok(LogicalDrive {
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
            status: String::from(res_map.get_string_property("Status")?),
            media_type: MediaType::from_int(res_map.get_int_property("MediaType")?),
            file_system: String::from(res_map.get_string_property("FileSystem")?),
            size: res_map.get_uint_property("Size")?,
            free_space: res_map.get_uint_property("FreeSpace")?,
            compressed: res_map.get_bool_property("Compressed")?,
            dirty: res_map.get_bool_property("VolumeDirty")?,
        })
    }

    pub fn get_name(&'a self) -> &'a str {
        &self.name
    }

    pub fn get_device_id(&'a self) -> &'a str {
        &self.device_id
    }

    pub fn get_size(&self) -> u64 {
        self.size
    }

    pub fn get_free_space(&self) -> u64 {
        self.free_space
    }

    pub fn get_file_system(&'a self) -> &'a str {
        &self.file_system
    }

    pub fn get_media_type(&'a self) -> &'a MediaType {
        &self.media_type
    }

    pub fn get_status(&'a self) -> &'a str {
        &self.status
    }

    pub fn is_compressed(&self) -> bool {
        self.compressed
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn get_supported_sizes(&self, migrator: &mut MSWMigrator) -> Result<(u64, u64), MigError> {
        let regex = Regex::new("^([a-zA-Z]):$").unwrap();
        if let Some(cap) = regex.captures(&self.device_id) {
            Ok(migrator
                .get_ps_info()
                .get_drive_supported_size(cap.get(1).unwrap().as_str())?)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::get_supported_sizes: invalid drive letter: '{}",
                    MODULE, self.device_id
                ),
            ))
        }
    }
}
