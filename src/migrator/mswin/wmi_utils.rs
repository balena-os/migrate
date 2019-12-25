// extern crate wmi;

use log::info;
use std::collections::HashMap;
// use std::rc::Rc;

use failure::ResultExt;

use crate::{
    common::{os_release::OSRelease, MigErrCtx, MigError, MigErrorKind},
    defs::OSArch,
    mswin::win_api::wmi_api::{Variant, WmiAPI},
};

pub(crate) mod volume;
pub(crate) mod physical_drive;
pub(crate) use physical_drive::PhysicalDrive;
pub(crate) mod logical_drive;
pub(crate) use logical_drive::LogicalDrive;

pub(crate) mod partition;
pub(crate) use partition::Partition;

// TODO: fix this
//#![cfg(debug_assertions)]
//const VERBOSE: bool = false;
// const VERBOSE: bool = true;

const EMPTY_STR: &str = "";
pub(crate) const NS_CVIM2: &str = "ROOT\\CIMV2";
const NS_MSW_STORAGE: &str = r"ROOT\Microsoft\Windows\Storage";

pub(crate) const WMIQ_OS: &str = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem";
// pub const WMIQ_CSProd: &str = "SELECT * FROM Win32_ComputerSystemProduct";
// pub const WMIQ_BOOT_CONFIG: &str = "SELECT * FROM Win32_SystemBootConfiguration";
// pub const WMIQ_Disk: &str = "SELECT * FROM Win32_DiskDrive";
// pub const WMIQ_Disk: &str = "SELECT Caption,Partitions,Status,DeviceID,Size,BytesPerSector,MediaType,InterfaceType FROM Win32_DiskDrive";
// pub const WMIQ_Partition: &str = "SELECT * FROM Win32_DiskPartition";
// pub const WMIQ_Partition: &str = "SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition,DiskIndex,Index FROM Win32_DiskPartition";

#[derive(Debug, Clone)]
pub(crate) struct WMIOSInfo {
    pub os_name: String,
    pub os_release: OSRelease,
    pub os_arch: OSArch,
    pub mem_tot: u64,
    pub mem_avail: u64,
    pub boot_dev: String,
}

// TODO: make WmiAPI an Rc to make it shareble with dependant objects ?
pub(crate) struct WmiUtils {}

impl WmiUtils {
    /*    pub fn new(namespace:  &str) -> Result<WmiUtils, MigError> {
            debug!("new: entered");
            Ok(Self { wmi_api: Rc::new(WmiAPI::get_api(namespace)?), })
        }

        pub fn wmi_query(&self, query: &str) -> Result<Vec<HashMap<String, Variant>>, MigError> {
            debug!("wmi_query: entered with '{}'",  query);
            Ok(self.wmi_api.raw_query(query)?)
        }
    */

    pub fn get_os_info() -> Result<WMIOSInfo, MigError> {
        let wmi_api = WmiAPI::get_api(NS_CVIM2)?;
        let wmi_res = wmi_api.raw_query(WMIQ_OS)?;
        let wmi_row = match wmi_res.get(0) {
            Some(r) => r,
            None => {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "init_sys_info: no rows in result from wmi query: '{}'",
                        WMIQ_OS
                    ),
                ));
            }
        };

        let empty = Variant::EMPTY();

        let boot_dev = match wmi_row.get("BootDevice").unwrap_or(&empty) {
            Variant::STRING(s) => s.clone(),
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "init_os_info: invalid result type for 'BootDevice'",
                ));
            }
        };

        let os_name = match wmi_row.get("Caption").unwrap_or(&empty) {
            Variant::STRING(s) => s.clone(),
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "init_os_info: invalid result type for 'Caption'",
                ));
            }
        };

        let os_release = match wmi_row.get("Version").unwrap_or(&empty) {
            Variant::STRING(os_rls) => OSRelease::parse_from_str(&os_rls)?,
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "init_os_info: invalid result type for 'Version'",
                ));
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
                            "init_os_info: invalid result string for 'OSArchitecture': {}",
                            s
                        ),
                    ));
                }
            }
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "init_os_info: invalid result type for 'OSArchitecture'",
                ));
            }
        };

        let mem_tot = match wmi_row.get("TotalVisibleMemorySize").unwrap_or(&empty) {
            Variant::STRING(s) => {
                s.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "init_sys_info: failed to parse TotalVisibleMemorySize from  '{}'",
                        s
                    ),
                ))? * 1024
            }
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "init_os_info: invalid result type for 'TotalVisibleMemorySize'",
                ));
            }
        } as u64;

        let mem_avail = match wmi_row.get("FreePhysicalMemory").unwrap_or(&empty) {
            Variant::STRING(s) => {
                s.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "init_sys_info: failed to parse 'FreePhysicalMemory' from  '{}'",
                        s
                    ),
                ))? * 1024
            }
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "init_os_info: invalid result type for 'FreePhysicalMemory'",
                ));
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
            debug!("get_partition_info: performing WMI Query: '{}'", query);
            let mut q_res = self.wmi_api.raw_query(&query)?;
            match q_res.len() {
                0 => Err(MigError::from_remark(MigErrorKind::NotFound,&format!("get_partition_info: the query returned an empty result set: '{}'", query))),
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
                _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("get_partition_info: invalid result cout for query, expected 1, got  {}", q_res.len()))),
            }
        }
    */

    pub fn query_drive_letters() -> Result<Vec<String>, MigError> {
        const QUERY: &str = "SELECT DeviceID FROM Win32_LogicalDisk";
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(QUERY)?;
        let mut result: Vec<String> = Vec::new();
        for res in q_res {
            /*for key in res.keys() {
                debug!("query_drive_letters: key: {}, value: {:?}", key, res.get(key).unwrap());
            }*/
            result.push(String::from(
                QueryRes::new(&res).get_string_property("DeviceID")?,
            ));
        }
        result.sort();
        Ok(result)
    }

    #[allow(dead_code)]
    pub fn test_get_drive(disk_index: u64) -> Result<(), MigError> {
        let query = format!("SELECT * FROM MSFT_Disk WHERE Number={}", disk_index);
        let mut q_res = WmiAPI::get_api(NS_MSW_STORAGE)?.raw_query(&query)?;

        match q_res.len() {
            0 => Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "get_disk_info: the query returned an empty result set: '{}'",
                    query
                ),
            )),
            1 => {
                let res_map = q_res.pop().unwrap();
                for (key, value) in res_map.iter().enumerate() {
                    info!("test_get_drive: {} -> {:?}", key, value);
                }
                Ok(())
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

    /*
        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_BootConfig)?;

        info!("init_sys_info: ****** QUERY: {}", wmi_utils::WMIQ_BootConfig);
        for wmi_row in wmi_res.iter() {
            info!("init_sys_info: *** ROW START");
            for (key,value) in wmi_row.iter() {
                info!("init_sys_info:   {} -> {:?}", key, value);
            }
        }

        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_Disk)?;
        info!("init_sys_info: ****** QUERY: {}", wmi_utils::WMIQ_Disk);
        for wmi_row in wmi_res.iter() {
            info!("init_sys_info:   *** ROW START");
            for (key,value) in wmi_row.iter() {
                info!("init_sys_info:   {} -> {:?}",  key, value);
            }
        }

        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_Partition)?;
        info!("init_sys_info: ****** QUERY: {}",  wmi_utils::WMIQ_Partition);
        for wmi_row in wmi_res.iter() {
            info!("init_sys_info:   *** ROW START",);
            for (key,value) in wmi_row.iter() {
                info!("init_sys_info:   {} -> {:?}", key, value);
            }
        }


        Ok(())
    }
    */
}

pub(crate) struct QueryRes<'a> {
    q_result: &'a HashMap<String, Variant>,
}

impl<'a> QueryRes<'a> {
    fn new(result: &HashMap<String, Variant>) -> QueryRes {
        QueryRes { q_result: result }
    }

    fn get_string_property(&'a self, prop_name: &str) -> Result<&'a str, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {
            match variant {
                Variant::STRING(val) => Ok(val.as_ref()),
                Variant::NULL() => Ok(EMPTY_STR),
                _ => {
                    Err(MigError::from_remark(MigErrorKind::InvParam,&format!("get_string_property: unexpected variant type, not STRING for key: '{}', value: {:?}", prop_name, variant)))
                }
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "get_string_property: value not found for key: '{}",
                    prop_name
                ),
            ))
        }
    }

    fn get_bool_property_with_def(&self, prop_name: &str, default: bool) -> Result<bool, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {
            match variant {
                Variant::BOOL(val) => {
                    Ok(*val)
                }, 
                Variant::STRING(val) => {                
                    Ok(val.eq_ignore_ascii_case("true"))
                },
                Variant::NULL() => {               
                    Ok(default)
                }
                _=> {
                    Err(MigError::from_remark(MigErrorKind::InvParam,&format!("get_bool_property: unexpected variant type, not BOOL for key: '{}' value: {:?}", prop_name, variant)))
                }
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_bool_property: value not found for key: '{}", prop_name),
            ))
        }
    }

    fn get_bool_property(&self, prop_name: &str) -> Result<bool, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {
            match variant {
                Variant::BOOL(val) => {
                    Ok(*val)
                }, 
                Variant::STRING(val) => {                
                    Ok(val.eq_ignore_ascii_case("true"))
                },
                _ => {                
                    Err(MigError::from_remark(MigErrorKind::InvParam,&format!("get_bool_property: unexpected variant type, not BOOL for key: '{}' value: {:?}", prop_name, variant)))
                }
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_bool_property: value not found for key: '{}", prop_name),
            ))
        }
    }

    fn get_opt_int_property(&self, prop_name: &str) -> Result<Option<i32>, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {
            if let Variant::I32(val) = variant {
                Ok(Some(*val))
            } else {
                if let Variant::NULL() = variant {
                    Ok(None)
                } else {
                    Err(MigError::from_remark(MigErrorKind::InvParam, &format!("get_int_property: unexpected variant type, not I32 for key: '{}' value: {:?}", prop_name, variant)))
                }
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_int_property: value not found for key: '{}", prop_name),
            ))
        }
    }

    fn get_int_property(&self, prop_name: &str) -> Result<i32, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {
            if let Variant::I32(val) = variant {
                Ok(*val)
            } else {
                Err(MigError::from_remark(MigErrorKind::InvParam,&format!("get_int_property: unexpected variant type, not I32 for key: '{}' value: {:?}", prop_name, variant)))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_int_property: value not found for key: '{}", prop_name),
            ))
        }
    }

    fn get_optional_uint_property(&self, prop_name: &str) -> Result<Option<u64>, MigError> {
        if let Some(ref variant) = self.q_result.get(prop_name) {
            match variant {
                Variant::STRING(val) => {
                    Ok(Some((*val).parse::<u64>().context(MigErrCtx::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "get_uint_property: failed tp parse value from string '{}'",
                            val
                        ),
                    ))?))
                },
                Variant::I32(val) => {
                    if *val < 0 {
                        Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!("get_uint_property: Found negative value: '{}' value: {}",
                                     prop_name, val)))
                    } else {
                        Ok(Some(*val as u64))
                    }
                },
                Variant::NULL() => {
                    Ok(None)
                },
                _ => {
                    Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("get_uint_property: unexpected variant type, not U32 or STRING for key: '{}' value: {:?}",
                                 prop_name, variant)))
                }
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_uint_property: value not found for key: '{}", prop_name),
            ))
        }
    }

    fn get_uint_property(&self, prop_name: &str) -> Result<u64, MigError> {
        if let Some(val) = self.get_optional_uint_property(prop_name)? {
            Ok(val)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "get_uint_property: unexpected variant type, not U32 or STRING for key: '{}'",
                    prop_name
                ),
            ))
        }
    }
}
