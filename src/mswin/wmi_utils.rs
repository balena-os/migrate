// extern crate wmi;

use log::{error, warn, info, trace};
use wmi::{COMLibrary, WMIConnection};
use std::collections::HashMap;
pub use wmi::Variant;

use crate::common::mig_error::{MigError,MigErrorCode};

const MODULE: &str = "mswin::wmi_utils";

pub const WMIQ_OS: &str = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem";
pub const WMIQ_CSProd: &str = "SELECT * FROM Win32_ComputerSystemProduct";
pub const WMIQ_BootConfig: &str = "SELECT * FROM Win32_SystemBootConfiguration";
// pub const WMIQ_Disk: &str = "SELECT * FROM Win32_DiskDrive"; 
pub const WMIQ_Disk: &str = "SELECT Caption,Partitions,Status,DeviceID,Size,BytesPerSector,MediaType,InterfaceType FROM Win32_DiskDrive";
// pub const WMIQ_Partition: &str = "SELECT * FROM Win32_DiskPartition";
pub const WMIQ_Partition: &str = "SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition,DiskIndex,Index FROM Win32_DiskPartition";
pub struct WmiUtils {
    wmi_con: WMIConnection,
}

impl WmiUtils {
    pub fn new() -> Result<WmiUtils,MigError> {
        trace!("{}::new: entered", MODULE);
        let com_con = match COMLibrary::new() {
            Ok(com_con) => com_con,
            Err(_why) => return Err(
                MigError::from_code(
                    MigErrorCode::ErrComInit, 
                    &format!("{}::new: failed to initialize COM interface",MODULE),
                    None)), //Some(Box::new(why))),
            };

        Ok(Self {
            wmi_con: match WMIConnection::new(com_con.into()) {
                Ok(c) => c,
                Err(_why) => return Err(
                    MigError::from_code(
                        MigErrorCode::ErrWmiInit, 
                        &format!("{}::new: failed to initialize WMI interface",MODULE),
                        None)), //Some(Box::new(why))),

            },
        })
    }
    
    pub fn wmi_query(&self,query: &str) -> Result<Vec<HashMap<String, Variant>>, MigError> {    
        trace!("{}::wmi_query: entered with '{}'", MODULE, query);
        match self.wmi_con.raw_query(query) {
            Ok(res) => Ok(res),
            Err(why) => { 
                error!("{}::wmi_query_system: failed on query {} : {:?}",MODULE, query,why);
                return Err(                    
                    MigError::from_code(
                        MigErrorCode::ErrWmiQueryFailed, 
                        &format!("{}::wmi_query_system: failed on query {}",MODULE, query),
                        None)); //Some(Box::new(why))),
                },
        }
    }       
}
