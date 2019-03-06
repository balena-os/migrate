pub mod mig_error;

use std::collections::HashMap;
use mig_error::{MigError,MigErrorCode};

#[derive(Debug)]
pub struct SysInfo {
    sys_info_map: HashMap<String, String>,
    os_name: String,
    os_release: String,        
}

impl<'a> SysInfo {
    pub fn new(map: HashMap<String, String>) -> SysInfo {        
        SysInfo {
            sys_info_map: map,
            os_name: String::from(""),
            os_release: String::from(""),
        }
    }

    fn get_str_value(&self,key_name: &str )  -> Result<&String,MigError> {
        match self.sys_info_map.get(key_name) {
            Some(s) => Ok(s),
            None => Err(MigError::from_code(MigErrorCode::ErrNotFound, &format!("SysInfo::set_str_value(): no value found for key {}", key_name) ,None))
            }
    }

    pub fn set_os_name(&mut self, key_name: &str) -> Result<(),MigError> {
        self.os_name = match self.get_str_value(key_name) {
            Ok(s) => s.clone(),
            Err(why) => return Err(why)
        };
        Ok(())
    }

    pub fn get_os_name(&self) -> &String {
        &self.os_name
    }
}
