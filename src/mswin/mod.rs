use crate::common::mig_error::MigError;
use crate::common::SysInfo;

use log::{info, trace};
 
// use std::error::Error;
// use std::io::prelude::*;

mod powershell;

// const OS: &str = "windows";

pub fn available() -> bool {
    trace!("called available()");
    return cfg!(windows);
}

pub fn get_sys_info() -> Result<Box<SysInfo>,MigError> {
    match powershell::PSInfo::try_init() {
        Ok(si) => Ok(Box::new(si)),
        Err(why) => Err(why)
    }
}

pub fn process() -> Result<(), MigError> {
    let mut ps_info = powershell::PSInfo::try_init()?;
    // info!("process: os_type = {}", ps_info.get_os_name());
    Ok(())
}
