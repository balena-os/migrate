pub mod mig_error;

use std::collections::HashMap;

#[derive(Debug)]
pub struct SysInfo {
    pub sys_info_map: HashMap<String, String>,
    pub os_type: String,
}

impl SysInfo {
    pub fn new(os_type: &str, map: HashMap<String, String>) -> SysInfo {
        SysInfo {
            sys_info_map: map,
            os_type: String::from(os_type),
        }
    }
}
