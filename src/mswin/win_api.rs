// extern crate winapi;
use std::io::Error;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::prelude::*;
use std::ptr::null_mut;
use log::{warn, info, trace};
use regex::Regex;
use std::rc::{Rc,Weak};
use std::cell::{RefCell};
use std::collections::hash_map::HashMap;
use std::fmt::{self,Debug};
use failure::{Fail,ResultExt};

use winapi::um::handleapi::{INVALID_HANDLE_VALUE};
use winapi::um::fileapi::{FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, QueryDosDeviceW};        
use winapi::um::winbase::{GetFirmwareEnvironmentVariableW};
use winapi::shared::winerror::{ERROR_INVALID_FUNCTION};

use super::drive_info::{StorageDevice, HarddiskPartitionInfo, HarddiskVolumeInfo, PhysicalDriveInfo, VolumeInfo, DriveLetterInfo};

use crate::mig_error::{MigError, MigErrorKind, MigErrCtx};

const MODULE:&str = "test_win_api";



/*
#[cfg(windows)]
fn print_message(msg: &str) -> Result<i32, Error> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::winuser::{MB_OK, MessageBoxW};
    let wide: Vec<u16> = OsStr::new(msg).encode_wide().chain(once(0)).collect();
    let ret = unsafe {
        MessageBoxW(null_mut(), wide.as_ptr(), wide.as_ptr(), MB_OK)
    };
    if ret == 0 { Err(Error::last_os_error()) }
    else { Ok(ret) }
}

fn to_c_string(os_str_buf: &[u8]) -> Result<CString,Box<std::error::Error>> {    
    match os_str_buf.iter().position(|&x| x == 0 ) {
        Some(i) => { 
            match CString::new(os_str_buf[0..i].to_vec()) {
                Ok(c) => Ok(c),
                Err(why) => Err(Box::new(why)),
            }            
        },
        None => return Err(Box::new(Error::from(ErrorKind::InvalidInput)))
    }
}
#[cfg(windows)]
fn to_os_string(os_str_buf: &[u16]) -> Result<OsString,Box<std::error::Error>> {            
    match os_str_buf.iter().position(|&x| x == 0 ) {        
        Some(i) => Ok(OsString::from_wide(&os_str_buf[0..i])),
        None => return Err(Box::new(Error::from(ErrorKind::InvalidInput)))
    }
}
*/


fn to_string(os_str_buf: &[u16]) -> Result<String,MigError> {            
    match os_str_buf.iter().position(|&x| x == 0 ) {        
        Some(i) => Ok(String::from_utf16_lossy(&os_str_buf[0..i])),
        None => return Err(MigError::from(MigErrorKind::InvParam)),
    }
}


fn to_string_list(os_str_buf: &[u16]) -> Result<Vec<String>,MigError> {            
    let mut str_list: Vec<String> = Vec::new();
    let mut start: usize = 0;
    for curr in os_str_buf.iter().enumerate() {
        if *curr.1 == 0 {
            if  start < curr.0 {
                let s = to_string(&os_str_buf[start .. curr.0 + 1]).context(MigErrCtx::from(MigErrorKind::InvParam))?;
                str_list.push(s);
                start = curr.0 + 1;
            } else {
                break;
            }            
        }
    }
    Ok(str_list)
}

fn clip<'a>(clip_str: &'a str, clip_start: Option<&str>, clip_end: Option<&str>) -> &'a str {            
    let mut work_str = clip_str;

    if let Some(s) = clip_start {
        if !s.is_empty() && work_str.starts_with(s) {        
            work_str = &work_str[s.len()..];
        }
    }

    if let Some(s) = clip_end {
        if !s.is_empty() && work_str.ends_with(s) {
            work_str = &work_str[0..(work_str.len()- s.len())];
        }
    }

    work_str
}

fn get_volumes() -> Result<Vec<String>,MigError> {
    trace!("{}::get_volumes: entered", MODULE);
    const BUFFER_SIZE: usize = 2048;
    let mut buffer: [u16;BUFFER_SIZE] = [0; BUFFER_SIZE];
    let mut vol_list: Vec<String> = Vec::new();

    let h_search = unsafe {
        FindFirstVolumeW(buffer.as_mut_ptr(), BUFFER_SIZE as u32)
    };
    
    if h_search == INVALID_HANDLE_VALUE {    
        return Err(MigError::from(Error::last_os_error().context(MigErrCtx::from(MigErrorKind::WinApi))));
    }

    vol_list.push(to_string(&buffer)?);

    loop {
        let ret = unsafe { FindNextVolumeW(h_search, buffer.as_mut_ptr(), BUFFER_SIZE as u32) };
        if ret == 0 {
            unsafe { FindVolumeClose(h_search) };
            return Ok(vol_list);
        }
        vol_list.push(to_string(&buffer)?);
    }
}


pub(crate) fn query_dos_device(dev_name: Option<&str>) -> Result<Vec<String>,MigError> {
    trace!("{}::query_dos_device: entered with {:?}" , MODULE, dev_name);  
    match dev_name {
        Some(s) => {
            const BUFFER_SIZE: usize = 8192;
            let mut buffer: [u16;BUFFER_SIZE] = [0; BUFFER_SIZE];
            let dev_path: Vec<u16> = OsStr::new(&s).encode_wide().chain(once(0)).collect();
            let num_tchar = unsafe { QueryDosDeviceW(dev_path.as_ptr(),buffer.as_mut_ptr(),BUFFER_SIZE as u32) };
            if num_tchar > 0 {
                trace!("{}::query_dos_device: success", MODULE);
                Ok(to_string_list(&buffer)?)
            } else {
                let os_err = Error::last_os_error();
                warn!("{}::query_dos_device: returned {}, last os error: {:?} ", MODULE, num_tchar, os_err);       
                return Err(MigError::from(os_err.context(MigErrCtx::from(MigErrorKind::WinApi))));
            }            
        },
        None => {
            const BUFFER_SIZE: usize = 32768;
            let mut buffer: [u16;BUFFER_SIZE] = [0; BUFFER_SIZE];
            let num_tchar = unsafe { QueryDosDeviceW(null_mut(),buffer.as_mut_ptr(),BUFFER_SIZE as u32) };
            if num_tchar > 0 {
                trace!("{}::query_dos_device: success", MODULE);
                Ok(to_string_list(&buffer)?)
            } else {
                let os_err = Error::last_os_error();
                warn!("{}::query_dos_device: returned {}, last os error: {:?} ", MODULE, num_tchar, os_err);       
                return Err(MigError::from(os_err.context(MigErrCtx::from(MigErrorKind::WinApi))));
            }                        
        },
    }
}

pub(crate) fn is_uefi_boot() -> Result<bool, MigError> {
    let dummy: Vec<u16> = OsStr::new("").encode_wide().chain(once(0)).collect();
    let guid: Vec<u16> = OsStr::new("{00000000-0000-0000-0000-000000000000}").encode_wide().chain(once(0)).collect();
    let res = unsafe { GetFirmwareEnvironmentVariableW(dummy.as_ptr(), guid.as_ptr(), null_mut(), 0) };
    if res != 0 {
        return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::is_uefi_boot: no error where an error was expected", MODULE)));
    }
    let os_err = Error::last_os_error();

    match os_err.raw_os_error() {
        Some(err) => { 
            if err == ERROR_INVALID_FUNCTION as i32 {
                Ok(false)
            } else {
                trace!("{}::is_uefi_boot: error value: {}",MODULE,err);
                Ok(true)
            }
        },
        None => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::is_uefi_boot: no error where an error was expeted", MODULE))),
    }
}


pub fn enumerate_volumes() -> Result<i32, MigError> {        

    match query_dos_device(None) { 
        Ok(sl) => {
            for device in sl {
                println!("got device name: {}",device);
            }
        },
        Err(why) => {
            println!("query_dos_device retured error: {:?}", why);
        }
    };

    
    for vol_name in get_volumes()? {
        let dev_name = clip(&vol_name,Some("\\\\?\\"), Some("\\"));

        println!("got dev_name: {}",dev_name);

        for device in query_dos_device(Some(dev_name))? {
            println!("  got dev_name: {}",device);
        }
    }    
    
    Ok(0)
}


