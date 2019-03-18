mod powershell;
mod win_api;
mod wmi_utils;

use log::{info, trace, error};

use lazy_static::lazy_static;
use regex::Regex;

use failure::{ResultExt};
use wmi_utils::{WmiUtils,Variant};

use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};

use crate::{SysInfo,OSRelease};

use powershell::{PSInfo};

#[cfg(debug_assertions)]
const VERBOSE: bool = true;
// TODO: fix this
//#![cfg(debug_assertions)]
//const VERBOSE: bool = false;


const MODULE: &str = "mswin";

pub struct MWIOSInfo {
    os_name: String,
    os_release: OSRelease,
    os_arch: String,
    mem_tot: usize,
    mem_avail: usize,
    boot_dev: String,
}

pub struct MSWInfo {
    ps_info: PSInfo,
    os_info: Option<MWIOSInfo>,
}

impl MSWInfo {
    pub fn try_init() -> Result<MSWInfo, MigError> {
        let mut msw_info = MSWInfo {
            ps_info: PSInfo::try_init()?,
            os_info: None,
        };
        Ok(msw_info)
    }

    fn init_os_info(&mut self) -> Result<MWIOSInfo, MigError> {
        let wmi_utils = WmiUtils::new().context(MigErrCtx::from_remark(MigErrorKind::Upstream,"Create WMI utils failed"))?;
        let wmi_res = wmi_utils.wmi_query(wmi_utils::WMIQ_OS)?;
        let wmi_row = match wmi_res.get(0) {
            Some(r) => r,
            None => return Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::init_sys_info: no rows in result from wmi query: '{}'", MODULE,wmi_utils::WMIQ_OS)))
        };
        
        if VERBOSE {
            info!("{}::init_sys_info: ****** QUERY: {}", MODULE, wmi_utils::WMIQ_BootConfig);
            info!("{}::init_sys_info: *** ROW START", MODULE);
            for (key,value) in wmi_row.iter() {
                info!("{}::init_sys_info:   {} -> {:?}", MODULE, key, value);        
            }            
        }

        let empty = Variant::Empty;
        
        let boot_dev = match wmi_row.get("BootDevice").unwrap_or(&empty) {
                Variant::String(s) => s.clone(),
                _ => return Err(MigError::from_remark(
                        MigErrorKind::InvParam, 
                        &format!("{}::init_os_info: invalid result type for 'BootDevice'",MODULE)))
        };
        
        let os_name = match wmi_row.get("Caption").unwrap_or(&empty) {
            Variant::String(s) => s.clone(), 
            _ => return Err(MigError::from_remark(
                        MigErrorKind::InvParam, 
                        &format!("{}::init_os_info: invalid result type for 'Caption'",MODULE)))
        };
        
        // parse si_os_release
        let os_release = match wmi_row.get("Version").unwrap_or(&empty) {
            Variant::String(os_rls) => parse_os_release(&os_rls)?,
            _ => return Err(MigError::from_remark(
                        MigErrorKind::InvParam, 
                        &format!("{}::init_os_info: invalid result type for 'Version'",MODULE))),
        };

        let os_arch = match wmi_row.get("OSArchitecture").unwrap_or(&empty) {
            Variant::String(s) => s.clone(), 
            _ => return Err(MigError::from_remark(
                        MigErrorKind::InvParam, 
                        &format!("{}::init_os_info: invalid result type for 'OSArchitecture'",MODULE))),
        };

        let mem_tot = match wmi_row.get("TotalVisibleMemorySize").unwrap_or(&empty) {
            Variant::String(s) => s.parse::<usize>().context(
                MigErrCtx::from_remark(MigErrorKind::InvParam, 
                &format!("{}::init_sys_info: failed to parse TotalVisibleMemorySize from  '{}'", MODULE,s)))?,
            _ => return Err(MigError::from_remark(
                        MigErrorKind::InvParam, 
                        &format!("{}::init_os_info: invalid result type for 'TotalVisibleMemorySize'",MODULE))),            
        };

        let mem_avail = match wmi_row.get("FreePhysicalMemory").unwrap_or(&empty) {
            Variant::String(s) => s.parse::<usize>().context(
                MigErrCtx::from_remark(MigErrorKind::InvParam, 
                &format!("{}::init_sys_info: failed to parse 'FreePhysicalMemory' from  '{}'", MODULE,s)))?,
            _ => return Err(MigError::from_remark(
                        MigErrorKind::InvParam, 
                        &format!("{}::init_os_info: invalid result type for 'FreePhysicalMemory'",MODULE))),            
        };

        Ok(MWIOSInfo{
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


impl SysInfo for MSWInfo {
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_name),
            None => {
                self.os_info = Some(self.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_name)              
            },
        }
    }

    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_release),
            None => {
                self.os_info = Some(self.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_release)              
            },
        }
    }

    fn get_mem_tot(&mut self) -> Result<usize,MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_tot),
            None => {
                self.os_info = Some(self.init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_tot)              
            },
        }
    }

    fn get_mem_avail(&mut self) -> Result<usize,MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_avail),
            None => {
                self.os_info = Some(self.init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_avail)              
            },
        }
    }

    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.boot_dev),
            None => {
                self.os_info = Some(self.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().boot_dev)              
            },
        }
    }

    fn is_admin(&mut self) -> Result<bool,MigError> {
        Ok(self.ps_info.is_admin()?)
    }
    
    fn is_secure_boot(&mut self) -> Result<bool,MigError> {
        // TODO: implement
        Err(MigError::from(MigErrorKind::NotImpl))
    }

}


fn parse_os_release(os_release: &str) -> Result<OSRelease,MigError> {
    lazy_static! {
        static ref RE_OS_VER: Regex = Regex::new(r"^(\d+)\.(\d+)\.(\d+)$").unwrap();                    
    }

    let captures = match RE_OS_VER.captures(os_release) {
        Some(c) => c,
        None => return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("{}::init_sys_info: parse regex failed to parse release string: '{}'",MODULE, os_release))),
    };

    let parse_capture = |i: usize| -> Result<u32,MigError> {
        match captures.get(i) {
            Some(s) => 
                Ok(s.as_str().parse::<u32>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam, 
                    &format!("{}::init_sys_info: failed to parse {} part {} to u32", MODULE, os_release, i)))?),            
            None => return Err(MigError::from_remark(
                                MigErrorKind::InvParam,
                                &format!("{}::init_sys_info: failed to get release part {} from: '{}'",MODULE, i, os_release))),
        }
    };

    if let Ok(n0) = parse_capture(1) {
        if let Ok(n1) = parse_capture(2) {
            if let Ok(n2) = parse_capture(3) {
                return Ok(OSRelease(n0,n1,n2));
            }
        }
    } 
    Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::init_sys_info: failed to parse release string: '{}'",MODULE, os_release)))
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
