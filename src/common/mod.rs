pub mod mig_error;

#[derive(Debug)]
pub struct SysInfo {
    pub os_type: String
}

impl SysInfo {
    pub fn new(os_type: &str) -> SysInfo {
        SysInfo{ os_type: String::from(os_type)}
    }
}