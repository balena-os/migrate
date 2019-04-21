//#![feature(rustc_private)]
extern crate lazy_static;
extern crate regex;

extern crate chrono;
extern crate clap;
extern crate colored;
extern crate failure;
extern crate log;
extern crate serde_json;
extern crate stderrlog;

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

pub use common::config::{Config, YamlConfig};
pub use common::mig_error::{MigErrCtx, MigError, MigErrorKind};
pub use common::os_release::OSRelease;
pub use common::OSArch;

#[cfg(target_os = "windows")]
pub fn migrate() -> Result<(), MigError> {
    Ok(mswin::MSWMigrator::migrate()?)
}

#[cfg(target_os = "linux")]
pub fn migrate() -> Result<(), MigError> {
    Ok(linux::LinuxMigrator::migrate()?)
}
