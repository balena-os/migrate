// extern crate wmi;

use log::{info, trace};
use std::collections::HashMap;
pub use wmi::Variant;
use wmi::{COMLibrary, WMIConnection};

use failure::{Fail, ResultExt};

use crate::mig_error::{MigErrCtx, MigError, MigErrorKind};
use crate::{OSArch, OSRelease};

// TODO: fix this
//#![cfg(debug_assertions)]
//const VERBOSE: bool = false;
const VERBOSE: bool = true;

const MODULE: &str = "mswin::wmi_utils";

pub const WMIQ_OS: &str = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem";
pub const WMIQ_CSProd: &str = "SELECT * FROM Win32_ComputerSystemProduct";
pub const WMIQ_BootConfig: &str = "SELECT * FROM Win32_SystemBootConfiguration";
// pub const WMIQ_Disk: &str = "SELECT * FROM Win32_DiskDrive";
pub const WMIQ_Disk: &str = "SELECT Caption,Partitions,Status,DeviceID,Size,BytesPerSector,MediaType,InterfaceType FROM Win32_DiskDrive";
// pub const WMIQ_Partition: &str = "SELECT * FROM Win32_DiskPartition";
pub const WMIQ_Partition: &str = "SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition,DiskIndex,Index FROM Win32_DiskPartition";

pub(crate) struct WMIOSInfo {
    pub os_name: String,
    pub os_release: OSRelease,
    pub os_arch: OSArch,
    pub mem_tot: u64,
    pub mem_avail: u64,
    pub boot_dev: String,
}

pub struct WmiUtils {
    wmi_con: WMIConnection,
}

impl WmiUtils {
    pub fn new() -> Result<WmiUtils, MigError> {
        trace!("{}::new: entered", MODULE);
        let com_con = COMLibrary::new().context(MigErrCtx::from(MigErrorKind::WmiInit))?;
        Ok(Self {
            wmi_con: WMIConnection::new(com_con.into())
                .context(MigErrCtx::from(MigErrorKind::WmiInit))?,
        })
    }

    fn wmi_query(&self, query: &str) -> Result<Vec<HashMap<String, Variant>>, MigError> {
        trace!("{}::wmi_query: entered with '{}'", MODULE, query);
        Ok(self
            .wmi_con
            .raw_query(query)
            .context(MigErrCtx::from(MigErrorKind::WmiQueryFailed))?)
    }

    pub(crate) fn init_os_info(&self) -> Result<WMIOSInfo, MigError> {
        let wmi_res = self.wmi_query(WMIQ_OS)?;
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

        let empty = Variant::Empty;

        let boot_dev = match wmi_row.get("BootDevice").unwrap_or(&empty) {
            Variant::String(s) => s.clone(),
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
            Variant::String(s) => s.clone(),
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
            Variant::String(os_rls) => OSRelease::parse_from_str(&os_rls)?,
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
            Variant::String(s) => {
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
            Variant::String(s) => s.parse::<u64>().context(MigErrCtx::from_remark(
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
            Variant::String(s) => s.parse::<u64>().context(MigErrCtx::from_remark(
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
