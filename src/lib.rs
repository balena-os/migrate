#[macro_use] extern crate failure;
#[cfg(windows)]
pub mod mswin;
// pub mod linux;
// pub mod darwin;
// pub mod common;
pub mod mig_error;

use std::fmt::{self,Display, Formatter};

use crate::mig_error::MigError;

pub struct OSRelease (u32,u32,u32);

impl Display for OSRelease {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

pub trait SysInfo {
    // TODO: modify to allow string references returned
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError>;
    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError>;
    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str,MigError>;
    fn get_mem_tot(&mut self) -> Result<usize,MigError>;
    fn get_mem_avail(&mut self) -> Result<usize,MigError>;
    fn is_admin(&mut self) -> Result<bool,MigError>;
    fn is_secure_boot(&mut self) -> Result<bool,MigError>;
}

pub trait Migrator {
    fn can_migrate(&self) -> bool; 
    fn migrate(&self) -> Result<(),MigError>;  
}
