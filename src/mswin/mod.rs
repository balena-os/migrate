mod powershell;
mod win_api;
mod wmi_utils;

use crate::common::mig_error;
use crate::common::{SysInfo,OSRelease};

use std::process::{Command};
use log::{info, warn, trace, error};
use csv;
use lazy_static::lazy_static;
use regex::Regex;

use std::ffi::OsString;
use std::os::windows::prelude::*;

use mig_error::{MigError, MigErrorCode};
use powershell::PSInfo;
use wmi_utils::{WmiUtils,Variant};

#[cfg(debug_assertions)]
const VERBOSE: bool = true;
// TODO: fix this
//#![cfg(debug_assertions)]
//const VERBOSE: bool = false;


const MODULE: &str = "mswin";

pub struct MSWInfo {
    ps_info: Option<PSInfo>,
    si_os_name: String,
    si_os_release: Option<OSRelease>,
    si_os_arch: String,
    si_mem_tot: usize,
    si_mem_avail: usize,
    si_boot_dev: String,
}

impl MSWInfo {
    pub fn try_init() -> Result<MSWInfo, MigError> {
        let mut msw_info = MSWInfo {
            ps_info: None,
            si_os_name: String::new(),
            si_os_release: None,
            si_os_arch: String::new(),
            si_mem_tot: 0,
            si_mem_avail: 0,
            si_boot_dev: String::new(),
        };

        match msw_info.init_sys_info() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        Ok(msw_info)
    }

    fn init_sys_info(&mut self) -> Result<(), MigError> {
        let wmi_utils = WmiUtils::new()?;
        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_OS)?;
        let wmi_row = match wmi_res.get(0) {
            Some(r) => r,
            None => return Err(MigError::from_code(
                MigErrorCode::ErrNotFound, 
                &format!("{}::init_sys_info: no rows in result from wmi query: '{}'", MODULE,wmi_utils::WMIQ_OS), 
                None))
        };
        
        if VERBOSE {
            info!("{}::init_sys_info: ****** QUERY: {}", MODULE, wmi_utils::WMIQ_BootConfig);
            info!("{}::init_sys_info: *** ROW START", MODULE);
            for (key,value) in wmi_row.iter() {
                info!("{}::init_sys_info:   {} -> {:?}", MODULE, key, value);        
            }            
        }

        let empty = Variant::Empty;

        if let Variant::String(s) = wmi_row.get("BootDevice").unwrap_or(&empty) {
            self.si_boot_dev = s.clone();
        }

        if let Variant::String(s) = wmi_row.get("Caption").unwrap_or(&empty) {
            self.si_os_name = s.clone();
        }

        // parse si_os_release
        if let Variant::String(os_release) = wmi_row.get("Version").unwrap_or(&empty) {
            self.si_os_release = match parse_os_release(&os_release) {
                Ok(r) => Some(r),
                Err(why) => { 
                    error!("{}::init_sys_info: failed to parse {:?}", MODULE, why);
                    None
                },
            };
        }

        if let Variant::String(s) = wmi_row.get("OSArchitecture").unwrap_or(&empty) {
            self.si_os_arch = match s {
                _ => s.clone()
            };
        }

        if let Variant::String(s) = wmi_row.get("TotalVisibleMemorySize").unwrap_or(&empty) {
            self.si_mem_tot = match s.parse::<usize>() {
                Ok(num) => num,
                Err(why) => return Err(
                    MigError::from_code(
                        MigErrorCode::ErrInvParam, 
                        &format!("{}::init_sys_info: failed to parse TotalVisibleMemorySize from  '{}'", MODULE,s) , 
                        Some(Box::new(why)))),
            };
        }

        if let Variant::String(s) = wmi_row.get("FreePhysicalMemory").unwrap_or(&empty) {
            self.si_mem_avail = match s.parse::<usize>() {
                Ok(num) => num,
                Err(why) => return Err(
                    MigError::from_code(
                        MigErrorCode::ErrInvParam, 
                        &format!("{}::init_sys_info: failed to parse FreePhysicalMemory from  '{}'", MODULE,s) , 
                        Some(Box::new(why)))),
            };
        }

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
}


impl SysInfo for MSWInfo {
    fn get_os_name(&self) -> String {
        self.si_os_name.clone()
    }

    fn get_os_release(&self) -> Option<OSRelease> {
        self.si_os_release
    }

    fn get_mem_tot(&self) -> usize {
        self.si_mem_tot
    }

    fn get_mem_avail(&self) -> usize {
        self.si_mem_avail
    }

    fn get_boot_dev(&self) -> String {
        self.si_boot_dev.clone()
    }
}


fn parse_os_release(os_release: &str) -> Result<OSRelease,MigError> {
    lazy_static! {
        static ref RE_OS_VER: Regex = Regex::new(r"^(\d+)\.(\d+)\.(\d+)$").unwrap();                    
    }

    let captures = match RE_OS_VER.captures(os_release) {
        Some(c) => c,
        None => return Err(MigError::from_code(
            MigErrorCode::ErrInvParam,
            &format!("{}::init_sys_info: parse regex failed to parse release string: '{}'",MODULE, os_release),
            None)),
    };

    let parse_capture = |i: usize| -> Result<u32,MigError> {
        match captures.get(i) {
            Some(s) => {
                match s.as_str().parse::<u32>() {
                    Ok(n) => Ok(n),
                    Err(_why) => 
                        return Err(MigError::from_code(
                                MigErrorCode::ErrInvParam,
                                &format!("{}::init_sys_info: failed to parse {} part {} to u32", MODULE, os_release, i),
                                None)),
                }
            },
            None => return Err(MigError::from_code(
                                MigErrorCode::ErrInvParam,
                                &format!("{}::init_sys_info: failed to get release part {} from: '{}'",MODULE, i, os_release),
                                None)),
        }
    };

    if let Ok(n0) = parse_capture(1) {
        if let Ok(n1) = parse_capture(2) {
            if let Ok(n2) = parse_capture(3) {
                return Ok((n0,n1,n2));
            }
        }
    } 
    Err(MigError::from_code(
                MigErrorCode::ErrInvParam,
                &format!("{}::init_sys_info: failed to parse release string: '{}'",MODULE, os_release),
                None))
}

pub fn available() -> bool {
    trace!("called available()");
    return cfg!(windows);
}

pub fn process() -> Result<(), MigError> {
    let mut ps_info = powershell::PSInfo::try_init()?;
    // info!("process: os_type = {}", ps_info.get_os_name());
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_mswin() {
        let msw_info = MSWInfo::try_init().unwrap();
        assert!(!msw_info.get_os_name().is_empty());
        assert!(if let Some(_or) = msw_info.get_os_release() {
            true
        } else {
            false
        });
        assert!(!msw_info.get_mem_avail() > 0);
        assert!(!msw_info.get_mem_tot() > 0);
    }
}
