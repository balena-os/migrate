use crate::common::mig_error::MigError;

use log::{info, trace};
// use std::error::Error;
// use std::io::prelude::*;

mod powershell;

// const OS: &str = "windows";

pub fn available() -> bool {
    trace!("called available()");
    return cfg!(windows);
}

pub fn process() -> Result<(), MigError> {
    let s_info = powershell::sys_info()?;
    info!("process: os_type = {}", s_info.get_os_name());
    Ok(())
}
