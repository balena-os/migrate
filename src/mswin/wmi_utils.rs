// extern crate wmi;

use log::{info, debug};
use std::collections::HashMap;
// pub use wmi::Variant;
// use wmi::{COMLibrary, WMIConnection};


use failure::{Fail, ResultExt};

use crate::mig_error::{MigErrCtx, MigError, MigErrorKind};
use crate::{OSArch, OSRelease};
use crate::mswin::win_api::wmi_api::{WmiAPI, Variant};

// TODO: fix this
//#![cfg(debug_assertions)]
//const VERBOSE: bool = false;
const VERBOSE: bool = true;

const MODULE: &str = "mswin::wmi_utils";

pub const WMIQ_OS: &str = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem";
//pub const WMIQ_CSProd: &str = "SELECT * FROM Win32_ComputerSystemProduct";
pub const WMIQ_BootConfig: &str = "SELECT * FROM Win32_SystemBootConfiguration";
// pub const WMIQ_Disk: &str = "SELECT * FROM Win32_DiskDrive";
// pub const WMIQ_Disk: &str = "SELECT Caption,Partitions,Status,DeviceID,Size,BytesPerSector,MediaType,InterfaceType FROM Win32_DiskDrive";
// pub const WMIQ_Partition: &str = "SELECT * FROM Win32_DiskPartition";
// pub const WMIQ_Partition: &str = "SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition,DiskIndex,Index FROM Win32_DiskPartition";

#[derive(Debug)]
pub(crate) struct WMIOSInfo {
    pub os_name: String,
    pub os_release: OSRelease,
    pub os_arch: OSArch,
    pub mem_tot: u64,
    pub mem_avail: u64,
    pub boot_dev: String,
}

#[derive(Debug)]
pub struct WmiPartitionInfo {
    pub name: String,
    pub bootable: bool,
    pub size: u64,
    pub number_of_blocks: u64,
    pub ptype: String,
    pub boot_partition: bool,
    pub disk_index: u64,
    pub partition_index: u64,
    pub start_offset: u64,
}

#[derive(Debug)]
pub struct WmiDriveInfo {
    pub name: String,
    pub size: u64,
    pub media_type: String,
    pub status: String,    
    pub bytes_per_sector: u64,
    pub partitions: u64,
    pub compression_method: String,
    pub disk_index: u64,
}



pub struct WmiUtils {
    wmi_api: WmiAPI,
}

impl WmiUtils {
    pub fn new() -> Result<WmiUtils, MigError> {
        debug!("{}::new: entered", MODULE);        
        Ok(Self { wmi_api: WmiAPI::get_api()?, })
    }

    pub fn wmi_query(&self, query: &str) -> Result<Vec<HashMap<String, Variant>>, MigError> {
        debug!("{}::wmi_query: entered with '{}'", MODULE, query);
        Ok(self.wmi_api.raw_query(query)?)            
    }

    pub(crate) fn init_os_info(&self) -> Result<WMIOSInfo, MigError> {
        let wmi_res = self.wmi_api.raw_query(WMIQ_OS)?;
        let wmi_row = match wmi_res.get(0) {
            Some(r) => r,
            None => {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "{}::init_sys_info: no rows in result from wmi query: '{}'",
                        MODULE, WMIQ_OS
                    ),
                ))
            }
        };

        if VERBOSE {
            info!(
                "{}::init_sys_info: ****** QUERY: {}",
                MODULE, WMIQ_BootConfig
            );
            info!("{}::init_sys_info: *** ROW START", MODULE);
            for (key, value) in wmi_row.iter() {
                info!("{}::init_sys_info:   {} -> {:?}", MODULE, key, value);
            }
        }

        let empty = Variant::EMPTY();

        let boot_dev = match wmi_row.get("BootDevice").unwrap_or(&empty) {
            Variant::STRING(s) => s.clone(),
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::init_os_info: invalid result type for 'BootDevice'",
                        MODULE
                    ),
                ))
            }
        };

        let os_name = match wmi_row.get("Caption").unwrap_or(&empty) {
            Variant::STRING(s) => s.clone(),
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::init_os_info: invalid result type for 'Caption'",
                        MODULE
                    ),
                ))
            }
        };

        let os_release = match wmi_row.get("Version").unwrap_or(&empty) {
            Variant::STRING(os_rls) => OSRelease::parse_from_str(&os_rls)?,
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::init_os_info: invalid result type for 'Version'",
                        MODULE
                    ),
                ))
            }
        };

        let os_arch = match wmi_row.get("OSArchitecture").unwrap_or(&empty) {
            Variant::STRING(s) => {
                if s.to_lowercase() == "64-bit" {
                    OSArch::AMD64
                } else if s.to_lowercase() == "32-bit" {
                    OSArch::I386
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "{}::init_os_info: invalid result string for 'OSArchitecture': {}",
                            MODULE, s
                        ),
                    ));
                }
            }
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::init_os_info: invalid result type for 'OSArchitecture'",
                        MODULE
                    ),
                ))
            }
        };

        let mem_tot = match wmi_row.get("TotalVisibleMemorySize").unwrap_or(&empty) {
            Variant::STRING(s) => s.parse::<u64>().context(MigErrCtx::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::init_sys_info: failed to parse TotalVisibleMemorySize from  '{}'",
                    MODULE, s
                ),
            ))?,
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::init_os_info: invalid result type for 'TotalVisibleMemorySize'",
                        MODULE
                    ),
                ))
            }
        } as u64;

        let mem_avail = match wmi_row.get("FreePhysicalMemory").unwrap_or(&empty) {
            Variant::STRING(s) => s.parse::<u64>().context(MigErrCtx::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::init_sys_info: failed to parse 'FreePhysicalMemory' from  '{}'",
                    MODULE, s
                ),
            ))?,
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::init_os_info: invalid result type for 'FreePhysicalMemory'",
                        MODULE
                    ),
                ))
            }
        };

        Ok(WMIOSInfo {
            os_name,
            os_release,
            os_arch,
            mem_tot,
            mem_avail,
            boot_dev,
        })
    }

    pub fn get_partition_info(&self, disk_index: u64, partition_index: u64) -> Result<WmiPartitionInfo, MigError> {
        let query = format!("SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition,StartingOffset FROM Win32_DiskPartition where DiskIndex={} and Index={}", disk_index, partition_index);
        debug!("{}::get_partition_info: performing WMI Query: '{}'", MODULE, query);
        let mut q_res = self.wmi_api.raw_query(&query)?;
        match q_res.len() {
            0 => Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_partition_info: the query returned an empty result set: '{}'", MODULE, query))), 
            1 => {
                let res_map = QueryRes::new(q_res.pop().unwrap());
                Ok(WmiPartitionInfo{
                    name: String::from(res_map.get_string_property("Caption")?),
                    bootable: res_map.get_bool_property("Bootable")?, 
                    size: res_map.get_uint_property("Size")?,
                    number_of_blocks: res_map.get_uint_property("NumberOfBlocks")?,
                    ptype: String::from(res_map.get_string_property("Type")?), // TODO: parse this value GPT / System 
                    boot_partition: res_map.get_bool_property("BootPartition")?,
                    start_offset: res_map.get_uint_property("StartingOffset")?,
                    disk_index,
                    partition_index,
                })
            },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_partition_info: invalid result cout for query, expected 1, got  {}",MODULE, q_res.len()))), 
        }
    } 

    pub fn get_drive_info(&self, disk_index: u64) -> Result<WmiDriveInfo, MigError> {
        let query = format!("SELECT Name, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive WHERE Index={}", disk_index);        
        debug!("{}::get_partition_info: performing WMI Query: '{}'", MODULE, query);
        let mut q_res = self.wmi_api.raw_query(&query)?;
        match q_res.len() {
            0 => Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_disk_info: the query returned an empty result set: '{}'", MODULE, query))), 
            1 => {
                let res_map = QueryRes::new(q_res.pop().unwrap());
                Ok(WmiDriveInfo{
                    name: String::from(res_map.get_string_property("Name")?),
                    media_type: String::from(res_map.get_string_property("MediaType")?),  // TODO: parse this value fixed / removable
                    size: res_map.get_uint_property("Size")?,
                    status: String::from(res_map.get_string_property("Status")?),
                    bytes_per_sector: res_map.get_uint_property("BytesPerSector")?,
                    partitions: res_map.get_uint_property("Partitions")?,
                    compression_method: String::from(res_map.get_string_property("CompressionMethod")?),
                    disk_index,
                })
            },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_drive_info: invalid result cout for query, expected 1, got  {}",MODULE, q_res.len()))), 
        }
    } 

    /*
        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_BootConfig)?;

        info!("{}::init_sys_info: ****** QUERY: {}", MODULE, wmi_utils::WMIQ_BootConfig);
        for wmi_row in wmi_res.iter() {
            info!("{}::init_sys_info: *** ROW START", MODULE);
            for (key,value) in wmi_row.iter() {
                info!("{}::init_sys_info:   {} -> {:?}", MODULE, key, value);
            }
        }

        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_Disk)?;
        info!("{}::init_sys_info: ****** QUERY: {}", MODULE, wmi_utils::WMIQ_Disk);
        for wmi_row in wmi_res.iter() {
            info!("{}::init_sys_info:   *** ROW START", MODULE);
            for (key,value) in wmi_row.iter() {
                info!("{}::init_sys_info:   {} -> {:?}", MODULE, key, value);
            }
        }

        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_Partition)?;
        info!("{}::init_sys_info: ****** QUERY: {}", MODULE, wmi_utils::WMIQ_Partition);
        for wmi_row in wmi_res.iter() {
            info!("{}::init_sys_info:   *** ROW START", MODULE);
            for (key,value) in wmi_row.iter() {
                info!("{}::init_sys_info:   {} -> {:?}", MODULE, key, value);
            }
        }


        Ok(())
    }
    */
}

struct QueryRes {
    q_result: HashMap<String,Variant>,
}

impl<'a> QueryRes {
    fn new(result: HashMap<String,Variant>,) -> QueryRes {
        QueryRes{q_result: result}
    }

    fn get_string_property(&'a self, prop_name: &str) -> Result<&'a str, MigError> {    
        if let Some(ref variant) = self.q_result.get(prop_name) {
            if let Variant::STRING(val) = variant {
                Ok(val.as_ref())
            } else {
                debug!("{}::get_string_property: unexpected variant type, expected STRING for key: '{} -> {:?}", MODULE, prop_name, variant);
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_string_property: unexpected variant type, not STRING for key: '{}", MODULE, prop_name)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_string_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }

    fn get_bool_property(&self, prop_name: &str) -> Result<bool, MigError> {    
        if let Some(ref variant) = self.q_result.get(prop_name) {
            if let Variant::BOOL(val) = variant {
                Ok(*val)
            } else {
                debug!("{}::get_bool_property: unexpected variant type, expected  BOOL for key: '{} -> {:?}", MODULE, prop_name, variant);
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_bool_property: unexpected variant type, not OOL for key: '{}", MODULE, prop_name)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_bool_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }

    fn get_sint_property(&self, prop_name: &str) -> Result<i64, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {            
            if let Variant::I32(val) = variant {
                Ok(*val as i64)
            } else {                
                debug!("{}::get_bool_property: unexpected variant type, expected STRING or I32 for key: '{} -> {:?}", MODULE, prop_name, variant);
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_bool_property: unexpected variant type, not BOOL for key: '{}", MODULE, prop_name)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_bool_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }

    fn get_uint_property(&self, prop_name: &str) -> Result<u64, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {            
            if let Variant::STRING(val) = variant {
                Ok((*val).parse::<u64>().context(MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::get_uint_property: failed tp parse value from string '{}'", MODULE, val)))?)
            } else {                
                debug!("{}::get_uint_property: unexpected variant type, expected STRING for key: '{} -> {:?}", MODULE, prop_name, variant);
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_bool_property: unexpected variant type, not OOL for key: '{}", MODULE, prop_name)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_bool_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }
}
