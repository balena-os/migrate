use  crate::common::mig_error::{MigError,MigErrorCode};
use  crate::common::SysInfo;

const OS: &str = "windows";

pub fn available() -> bool {    
    return cfg!(windows);
}

pub fn sys_info() -> Result<SysInfo,MigError> {
    if cfg!(windows) {
        Ok(SysInfo::new("windows"))
    } else {
        Err(MigError::from_code(MigErrorCode::ErrInvOSType(String::from("invalid OS, not windows"))))
    }
}

pub fn process() -> Result<(),MigError> {
    Err(MigError::from_code(MigErrorCode::ErrNotImpl(format!("process is not yet implemented for {}",OS))))    
}