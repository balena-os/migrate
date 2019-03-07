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
    let mut ps_info = powershell::PSInfo::try_init()?;
    // info!("process: os_type = {}", ps_info.get_os_name());
    Ok(())
}
