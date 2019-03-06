pub mod mig_error;

use std::collections::HashMap;
use mig_error::{MigError,MigErrorCode};
use regex::Regex;
use log::trace;

use lazy_static::lazy_static;

#[derive(Debug)]
pub struct SysInfo {
    sys_info_map: HashMap<String, String>,
    os_name: String,
    os_version: String,
    host_name: String,        
    boot_device: String,
    tot_memory: usize,
    avail_memory: usize,
}

impl<'a> SysInfo {
    pub fn new(map: HashMap<String, String>) -> SysInfo {        
        SysInfo {
            sys_info_map: map,
            os_name: String::from(""),
            os_version: String::from(""),
            host_name: String::from(""),
            boot_device: String::from(""),
            tot_memory: 0,
            avail_memory: 0,
        }
    }

    fn get_str_value(&self,key_name: &str )  -> Result<&String,MigError> {
        match self.sys_info_map.get(key_name) {
            Some(s) => Ok(s),
            None => Err(MigError::from_code(MigErrorCode::ErrNotFound, &format!("SysInfo::set_str_value(): no value found for key {}", key_name) ,None))
            }
    }

    fn get_mem_value(&self,key_name: &str )  -> Result<usize,MigError> {
        trace!("get_mem_value: entered with {}", key_name); 
        
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^((\d{1,3}(,\d{3})*)|(\d+))\s*(MB|KB|GB)?$").unwrap();
            //static ref RE: Regex = Regex::new(r"^(\d+)\s*(MB|KB|GB)?$").unwrap();
        }

        match self.sys_info_map.get(key_name) {
            Some(s) => {                             
                trace!("get_mem_value: got key_value {}", s); 
                let captures = match RE.captures(s) {
                    Some(c) => c,
                    None => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("SysInfo::set_mem_value(): failed to parse memory field '{}' from '{}' - no match on regex", key_name, &s) ,None))
                };                

                let digits = match captures.get(1) {
                    Some(m) => m.as_str(),
                    None => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("SysInfo::set_mem_value(): failed to parse memory field '{}' from '{}' - no digits captured ", key_name, &s) ,None))
                };

                trace!("get_mem_value: got digits {}", digits); 

                let mem: usize  = match digits.parse() {
                    Ok(num) => num,
                    Err(why) => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("SysInfo::set_mem_value(): failed to parse memory field '{}' from '{}' - failed to parse digits '{}'", key_name, &s, &digits) , Some(Box::new(why))))
                };
            
                Ok(match captures.get(2) .map_or("", |m| m.as_str()) {
                    "MB" => mem * 1024 * 1024,
                    "KB" => mem * 1024,
                    _ => mem,
                })
            },
            None => Err(MigError::from_code(MigErrorCode::ErrNotFound, &format!("SysInfo::set_str_value(): no value found for key {}", key_name) ,None)),
        }
    }


    pub fn set_os_name(&mut self, key_name: &str) -> Result<(),MigError> {
        self.os_name = match self.get_str_value(key_name) {
            Ok(s) => s.clone(),
            Err(why) => return Err(why)
        };
        Ok(())
    }

    pub fn set_os_version(&mut self, key_name: &str) -> Result<(),MigError> {
        self.os_version = match self.get_str_value(key_name) {
            Ok(s) => s.clone(),
            Err(why) => return Err(why)
        };
        Ok(())
    }

    pub fn set_host_name(&mut self, key_name: &str) -> Result<(),MigError> {
        self.host_name = match self.get_str_value(key_name) {
            Ok(s) => s.clone(),
            Err(why) => return Err(why)
        };
        Ok(())
    }

    pub fn set_boot_device(&mut self, key_name: &str) -> Result<(),MigError> {
        self.boot_device = match self.get_str_value(key_name) {
            Ok(s) => s.clone(),
            Err(why) => return Err(why)
        };
        Ok(())
    }

    pub fn set_tot_mem(&mut self, key_name: &str) -> Result<(),MigError> {
        self.tot_memory = match self.get_mem_value(key_name) {
            Ok(num) => num,
            Err(why) => return Err(why)
        };
        Ok(())
    }

    pub fn get_os_name(&self) -> &String {
        &self.os_name
    }
}
