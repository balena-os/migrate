pub mod mig_error;

use std::collections::HashMap;

#[derive(Debug)]
pub struct SysInfo {
    sys_info_map: HashMap<String, String>,
    os_type_key: String,
}

impl<'a> SysInfo {
    pub fn new(os_type_key: &str, map: HashMap<String, String>) -> SysInfo {        
        SysInfo {
            sys_info_map: map,
            os_type_key: String::from(os_type_key),
        }
    }

    pub fn get_os_type(&self) -> Option<&String> {
        self.sys_info_map.get(&self.os_type_key)
    }
}
