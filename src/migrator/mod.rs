//#![feature(rustc_private)]
extern crate lazy_static;
extern crate regex;

extern crate clap;
extern crate failure;
extern crate log;
extern crate stderrlog;
#[cfg(target_os = "windows")]
extern crate winapi;

#[cfg(target_os = "linux")]
pub extern crate libc;

#[cfg(target_os = "windows")]
pub mod mswin;
// #[cfg(target_os = "windows")]
// use mswin::drive_info::PhysicalDriveInfo;

#[cfg(target_os = "linux")]
pub mod linux;
mod common;

pub use common::mig_error::{MigError, MigErrorKind, MigErrCtx};
pub use common::os_release::OSRelease;
pub use common::OSArch;
pub use common::config::{Config, YamlConfig};

pub trait Migrator {
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str, MigError>;
    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease, MigError>;
    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch, MigError>;
    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str, MigError>;
    fn get_mem_tot(&mut self) -> Result<u64, MigError>;
    fn get_mem_avail(&mut self) -> Result<u64, MigError>;
    fn is_admin(&mut self) -> Result<bool, MigError>;
    fn is_secure_boot(&mut self) -> Result<bool, MigError>;
    fn can_migrate(&mut self) -> Result<bool, MigError>;
    fn migrate(&mut self) -> Result<(), MigError>;
    fn is_uefi_boot(&mut self) -> Result<bool, MigError>;
}




#[cfg(target_os = "windows")]
pub fn get_migrator() -> Result<Box<Migrator>, MigError> {    
    use mswin::MSWMigrator;
    Ok(Box::new(MSWMigrator::try_init()?))
}

#[cfg(target_os = "linux")]
pub fn get_migrator(config: Config) -> Result<Box<Migrator>, MigError> {
    use linux::LinuxMigrator;
    Ok(Box::new(LinuxMigrator::try_init(config)?))
}
