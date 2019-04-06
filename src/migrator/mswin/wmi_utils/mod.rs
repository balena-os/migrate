// extern crate wmi;

use log::{info,debug};
use std::collections::HashMap;
// use std::rc::Rc;

use failure::{ResultExt};

use crate::migrator::{
    MigErrCtx, 
    MigError, 
    MigErrorKind,
    OSArch, 
    OSRelease,
    mswin::win_api::wmi_api::{WmiAPI, Variant}
};

mod physical_drive;
pub use physical_drive::PhysicalDrive;
mod logical_drive;
pub use logical_drive::LogicalDrive;

mod partition;
pub use partition::Partition;


// TODO: fix this
//#![cfg(debug_assertions)]
//const VERBOSE: bool = false;
// const VERBOSE: bool = true;

const MODULE: &str = "mswin::wmi_utils";

const EMPTY_STR: &str = "";
pub const NS_CVIM2: &str = "ROOT\\CIMV2";
pub const NS_MSW_STORAGE: &str = r"ROOT\Microsoft\Windows\Storage";



pub const WMIQ_OS: &str = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem";
// pub const WMIQ_CSProd: &str = "SELECT * FROM Win32_ComputerSystemProduct";
// pub const WMIQ_BOOT_CONFIG: &str = "SELECT * FROM Win32_SystemBootConfiguration";
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

// TODO: make WmiAPI an Rc to make it shareble with dependant objects ? 
pub struct WmiUtils {}

impl WmiUtils {
/*    pub fn new(namespace:  &str) -> Result<WmiUtils, MigError> {
        debug!("{}::new: entered", MODULE);        
        Ok(Self { wmi_api: Rc::new(WmiAPI::get_api(namespace)?), })
    }

    pub fn wmi_query(&self, query: &str) -> Result<Vec<HashMap<String, Variant>>, MigError> {
        debug!("{}::wmi_query: entered with '{}'", MODULE, query);
        Ok(self.wmi_api.raw_query(query)?)            
    }
*/

    pub(crate) fn init_os_info() -> Result<WMIOSInfo, MigError> {
        let wmi_api = WmiAPI::get_api(NS_CVIM2)?;
        let wmi_res = wmi_api.raw_query(WMIQ_OS)?;
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

/*  make this query_partitions (if required), individual partitions can be queried fron drive
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
                    device_id: String::new(),
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
*/ 
    pub fn query_drives() -> Result<Vec<PhysicalDrive>, MigError> {   
        let query = PhysicalDrive::get_query_all();     
        debug!("{}::get_drives: performing WMI Query: '{}'", MODULE, query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        let mut result: Vec<PhysicalDrive> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(res);
            result.push(PhysicalDrive::new(res_map)?);
        }
        Ok(result)
    }

    pub fn get_drive(disk_index: u64) -> Result<PhysicalDrive, MigError> {
        let query = PhysicalDrive::get_query_by_index(disk_index);             
        debug!("{}::get_drives: performing WMI Query: '{}'", MODULE, query);
        let mut q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        match q_res.len() {
            0 => Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_disk_info: the query returned an empty result set: '{}'", MODULE, query))), 
            1 => {
                let res_map = QueryRes::new(q_res.pop().unwrap());
                Ok(PhysicalDrive::new(res_map)?)
            },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_drive_info: invalid result cout for query, expected 1, got  {}",MODULE, q_res.len()))), 
        }
    } 


    pub fn test_get_drive(disk_index: u64) -> Result<(),MigError> {
        let query = format!("SELECT * FROM MSFT_Disk WHERE Number={}", disk_index);
        let mut q_res = WmiAPI::get_api(NS_MSW_STORAGE)?.raw_query(&query)?;

        match q_res.len() {
            0 => Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_disk_info: the query returned an empty result set: '{}'", MODULE, query))), 
            1 => {
                let res_map = q_res.pop().unwrap();
                for (key,value) in res_map.iter().enumerate() {
                    info!("{}::test_get_drive: {} -> {:?}", MODULE, key, value);
                }
                Ok(())
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

pub(crate) struct QueryRes {
    q_result: HashMap<String,Variant>,
}

impl<'a> QueryRes {
    fn new(result: HashMap<String,Variant>,) -> QueryRes {
        QueryRes{q_result: result}
    }

    fn get_string_property(&'a self, prop_name: &str) -> Result<&'a str, MigError> {    
        if let Some(ref variant) = self.q_result.get(prop_name) {
            match variant {
                Variant::STRING(val) => Ok(val.as_ref()),
                Variant::NULL() => Ok(EMPTY_STR),
                _ => {
                    Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_string_property: unexpected variant type, not STRING for key: '{}', value: {:?}", MODULE, prop_name, variant)))
                }
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
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_bool_property: unexpected variant type, not OOL for key: '{}' value: {:?}", MODULE, prop_name, variant)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_bool_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }

    fn get_int_property(&self, prop_name: &str) -> Result<i32, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {            
            if let Variant::I32(val) = variant {
                Ok(*val)
            } else {                                
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_int_property: unexpected variant type, not I32 for key: '{}' value: {:?}", MODULE, prop_name, variant)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_int_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }

    fn get_uint_property(&self, prop_name: &str) -> Result<u64, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {            
            if let Variant::STRING(val) = variant {
                Ok((*val).parse::<u64>().context(MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::get_uint_property: failed tp parse value from string '{}'", MODULE, val)))?)
            } else {                                
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_uint_property: unexpected variant type, not STRING for key: '{}', value: {:?}", MODULE, prop_name, variant)))
            }
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::get_uint_property: value not found for key: '{}", MODULE, prop_name)))
        }
     }
}
