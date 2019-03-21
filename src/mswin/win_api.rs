// extern crate winapi;
use std::io::Error;
use std::io::ErrorKind;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::prelude::*;
use std::ptr::null_mut;
use log::{warn, info, trace};
use lazy_static::lazy_static;
use regex::Regex;
use std::rc::{Rc,Weak};
use std::cell::{RefCell};
use std::collections::hash_map::HashMap;
use std::fmt::{self,Debug};
use failure::{Fail,ResultExt, Context};

use winapi::um::handleapi::{INVALID_HANDLE_VALUE};
use winapi::um::fileapi::{FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, QueryDosDeviceW};        
use winapi::um::winbase::{GetFirmwareEnvironmentVariableW};
use winapi::shared::winerror::{ERROR_INVALID_FUNCTION};


use crate::mig_error::{MigError, MigErrorKind, MigErrCtx};

const MODULE:&str = "test_win_api";

#[derive(Debug)]
pub enum StorageDevice {
    PhysicalDrive(Rc<PhysicalDriveInfo>),
    HarddiskVolume(Rc<RefCell<HarddiskVolumeInfo>>),
    HarddiskPartition(Rc<RefCell<HarddiskPartitionInfo>>),
    Volume(Rc<RefCell<VolumeInfo>>),
    DriveLetter(Rc<RefCell<DriveLetterInfo>>),    
}

#[derive(Debug)]
pub struct PhysicalDriveInfo {
    dev_name: String,
    index: u64,    
    device: String,
}

// #[derive(Debug)]
pub struct HarddiskVolumeInfo {
    dev_name: String,
    index: u64,
    device: String,    
    hdpart: Option<Weak<RefCell<HarddiskPartitionInfo>>>
}

// need this to break infinite cycle introduced by weak backref to hdpart
impl Debug for HarddiskVolumeInfo {
 fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut dep_dev = String::from("None");
        if let Some(hdp) = &self.hdpart {
            if let Some(hdp) = hdp.upgrade() {
                dep_dev = format!("HarddiskPartition({},{})",hdp.as_ref().borrow().hd_index,hdp.as_ref().borrow().part_index)
            } else {
                // consider error
                dep_dev = String::from("invalid");
            }
        }

        write!( f, "HarddiskVolumeInfo {{ dev_name: {}, index: {}, device: {}, hdpart: {} }}", self.dev_name, self.index, self.device, dep_dev)
    }
}


#[derive(Debug)]
pub struct HarddiskPartitionInfo {
    dev_name: String,
    hd_index: u64,
    part_index: u64,
    device: String,
    phys_disk: Option<Rc<PhysicalDriveInfo>>,
    hd_vol: Option<Rc<RefCell<HarddiskVolumeInfo>>>
}

#[derive(Debug)]
pub struct VolumeInfo {
    dev_name: String,
    uuid: String,    
    device: String,
    hd_vol: Option<Rc<RefCell<HarddiskVolumeInfo>>>
}

#[derive(Debug)]
pub struct DriveLetterInfo {
    dev_name: String,
    device: String,
    hd_vol: Option<Rc<RefCell<HarddiskVolumeInfo>>>
}


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


fn query_dos_device(dev_name: Option<&str>) -> Result<Vec<String>,MigError> {
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

pub fn is_uefi_boot() -> Result<bool, MigError> {
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

pub fn enumerate_drives() -> Result<HashMap<String,StorageDevice>,MigError> {    
    trace!("{}::enumerate_drives: entered" , MODULE);

    let re_dl = Regex::new(r"^([A-Z]:)$").unwrap();
    let re_hdv = Regex::new(r"^HarddiskVolume([0-9]+)$").unwrap();
    let re_pd = Regex::new(r"^PhysicalDrive([0-9]+)$").unwrap();
    let re_vol = Regex::new(r"^Volume\{([0-9a-z\-]+)\}$").unwrap();
    let re_hdpart = Regex::new(r"^Harddisk([0-9]+)Partition([0-9]+)$").unwrap();
    // let re_devname = Regex::new(r"^/device/(.*)$").unwrap();

    let mut hdp_list: Vec<Rc<RefCell<HarddiskPartitionInfo>>> = Vec::new();
    let mut hdv_list: Vec<Rc<RefCell<HarddiskVolumeInfo>>> = Vec::new();
    // let mut pd_list: Vec<Rc<RefCell<PhysicalDriveInfo>>> = Vec::new();
    let mut dl_list: Vec<Rc<RefCell<DriveLetterInfo>>> = Vec::new();
    let mut vol_list: Vec<Rc<RefCell<VolumeInfo>>> = Vec::new();

    let mut dev_map: HashMap<String,StorageDevice> = HashMap::new();

    match query_dos_device(None) { 
        Ok(dl) => {            
            for device in dl {
                trace!("{}::enumerate_drives: got device name: {}",MODULE, device);
                loop {  
                    if let Some(c) = re_hdpart.captures(&device) {
                        hdp_list.push(
                            Rc::new(
                                RefCell::new(
                                    HarddiskPartitionInfo{
                                        dev_name: device.clone(),
                                        hd_index: c.get(1).unwrap().as_str().parse::<u64>().unwrap(),
                                        part_index: c.get(2).unwrap().as_str().parse::<u64>().unwrap(),
                                        device: query_dos_device(Some(&device))?.get(0).unwrap().clone(),
                                        phys_disk: None,
                                        hd_vol: None,})));
                                                                
                        break;
                    } 

                    if let Some(c) = re_hdv.captures(&device) {                        
                        hdv_list.push(
                            Rc::new(
                                RefCell::new(                        
                                    HarddiskVolumeInfo{
                                        dev_name: device.clone(),
                                        index: c.get(1).unwrap().as_str().parse::<u64>().unwrap(),
                                        device: query_dos_device(Some(&device))?.get(0).unwrap().clone(), 
                                        hdpart: None,                                   
                                    })));
                        break;
                    } 

                    if re_dl.is_match(&device) {
                        dl_list.push(
                            Rc::new(
                                RefCell::new(
                                    DriveLetterInfo{
                                        dev_name: device.clone(),                                    
                                        device: query_dos_device(Some(&device))?.get(0).unwrap().clone(),
                                        hd_vol: None
                                    })));
                        break;
                    }


                    if let Some(c) = re_pd.captures(&device) {                    
                        dev_map.entry(device.clone()).or_insert(                            
                            StorageDevice::PhysicalDrive(
                                Rc::new(
                                    PhysicalDriveInfo{
                                        dev_name: device.clone(),
                                        index: c.get(1).unwrap().as_str().parse::<u64>().unwrap(),
                                        device: query_dos_device(Some(&device))?.get(0).unwrap().clone(),
                                        })));
                        break;
                    } 

                    if let Some(c) = re_vol.captures(&device) {                    
                        vol_list.push(
                            Rc::new(
                                RefCell::new(
                                    VolumeInfo{
                                        dev_name: device.clone(),
                                        uuid: String::from(c.get(1).unwrap().as_str()),
                                        device: query_dos_device(Some(&device))?.get(0).unwrap().clone(),
                                        hd_vol: None
                                    })));
                        break;
                    } 

                    break;
                }
            }            
            
            loop {
                match hdp_list.pop() {
                    Some(hdp) => {
                        let mut hdpart = hdp.as_ref().borrow_mut();
                        info!("{}::enumerate_drives: looking at: {:?}",MODULE, hdpart);
                        let findstr = format!("PhysicalDrive{}",hdpart.hd_index);
                        if let Some(pd) = dev_map.get(&findstr) {
                            if let StorageDevice::PhysicalDrive(pd) = pd {
                                hdpart.phys_disk = Some(pd.clone());
                            }  else {
                                panic!("{}::enumerate_drives: invalid type (not PhysicalDrive) {} in dev_map",MODULE, &findstr); 
                            }                   
                        } else {
                            return Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::enumerate_drives: could not find {} in dev_map",MODULE, &findstr)));
                        }
                        
                        for hdv in &hdv_list {
                            if hdv.as_ref().borrow().device == hdpart.device {
                                info!("{}::enumerate_drives: partition {} found matching hdv {:?}",MODULE, &hdpart.dev_name, hdv);
                                // TODO: modify hd_vol here                                
                                hdpart.hd_vol = Some(hdv.clone());
                                hdv.as_ref().borrow_mut().hdpart = Some(Rc::downgrade(&hdp));
                                break;
                            }
                        }                        
                        dev_map.entry(hdpart.dev_name.clone()).or_insert(StorageDevice::HarddiskPartition(hdp.clone()));                        
                    },
                    None => { break; }                   
                }
            }    

            loop {
                match vol_list .pop() {                    
                    Some(vol) => {
                        let mut volume = vol.as_ref().borrow_mut();
                        for hdv in &hdv_list {
                            if hdv.as_ref().borrow().device == volume.device {
                                info!("{}::enumerate_drives: volume {} found matching hdv {:?}",MODULE, &volume.dev_name, hdv);
                                // TODO: modify hd_vol here                                
                                volume.hd_vol = Some(hdv.clone());                                
                                break;
                            }
                        }                        
                        dev_map.entry(volume.dev_name.clone()).or_insert(StorageDevice::Volume(vol.clone()));                        
                        
                    },
                    None => { break; },
                }
            }

            loop {
                match dl_list .pop() {                    
                    Some(dl) => {
                        let mut driveletter = dl.as_ref().borrow_mut();
                        for hdv in &hdv_list {
                            if hdv.as_ref().borrow().device == driveletter.device {
                                info!("{}::enumerate_drives: driveletter {} found matching hdv {:?}",MODULE, &driveletter.dev_name, hdv);
                                // TODO: modify hd_vol here                                
                                driveletter.hd_vol = Some(hdv.clone());                                
                                break;
                            }
                        }                        
                        dev_map.entry(driveletter.dev_name.clone()).or_insert(StorageDevice::DriveLetter(dl.clone()));                        
                        
                    },
                    None => { break; },
                }

            }
        },
        Err(why) => {
            println!("query_dos_device retured error: {:?}", why);
        }
    };

    Ok(dev_map)
}    


