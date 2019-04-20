//#![feature(rustc_private)]
extern crate lazy_static;
extern crate regex;

extern crate clap;
extern crate failure;
extern crate log;
extern crate chrono;
extern crate stderrlog;
extern crate colored;
extern crate serde_json;

#[cfg(target_os = "windows")]
extern crate winapi;

#[cfg(target_os = "linux")]
pub extern crate libc;

#[cfg(target_os = "windows")]
mod mswin;
// #[cfg(target_os = "windows")]
// use mswin::drive_info::PhysicalDriveInfo;

#[cfg(target_os = "linux")]
mod linux;

mod common;

pub use common::mig_error::{MigError, MigErrorKind, MigErrCtx};
pub use common::os_release::OSRelease;
pub use common::OSArch;
pub use common::config::{Config, YamlConfig};

#[cfg(target_os = "windows")]
pub fn migrate() -> Result<(),MigError> { 
    Ok(mswin::MSWMigrator::migrate()?)
}

#[cfg(target_os = "linux")]
pub fn migrate() -> Result<(),MigError> {            
    Ok(linux::LinuxMigrator::migrate()?)
}
