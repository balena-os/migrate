use std::rc::{Rc};

// use log::{debug};
use regex::{Regex};
use crate::migrator::{
    MigError, 
    MigErrorKind,
    mswin::{ 
        MSWMigrator, 
        win_api::wmi_api::{WmiAPI},
    },
};
use super::{QueryRes};

const MODULE: &str = "mswin::wmi_utils::logical_drive";
// const QUERY_ALL: &str = "SELECT Caption, Index, DeviceID, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive";        



#[derive(Debug)]
pub struct LogicalDrive {
    name: String,
    device_id: String,
}

impl<'a> LogicalDrive {
    pub(crate) fn new(res_map: QueryRes ) -> Result<LogicalDrive,MigError> {
        Ok(LogicalDrive {             
            name: String::from(res_map.get_string_property("Caption")?),
            device_id: String::from(res_map.get_string_property("DeviceID")?),
        })
    }

    pub fn get_device_id(&'a self) -> &'a str {
        &self.device_id
    }

    pub fn get_supported_sizes(&self, migrator: &mut MSWMigrator) -> Result<(u64,u64),MigError> {
        let regex = Regex::new("^([a-zA-Z]):$").unwrap();
        if let Some(cap) = regex.captures(&self.device_id) {
            Ok(migrator.get_ps_info().get_drive_supported_size(cap.get(1).unwrap().as_str())?)
        } else {
            Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_supported_sizes: invalid drive letter: '{}", MODULE, self.device_id)))
        }        
    }  
}
