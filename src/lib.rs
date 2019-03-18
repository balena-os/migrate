#[macro_use] extern crate failure;

#[cfg(target_os = "windows")]
pub mod mswin;

#[cfg(target_os = "linux")]
pub mod linux;
// pub mod darwin;
// pub mod common;
pub mod mig_error;

use std::fmt::{self,Display, Formatter};
use lazy_static::lazy_static;
use regex::Regex;
use failure::{ResultExt};

use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};

#[derive(Debug)]

pub enum OSArch {
    X86_64,
    I686,    
}

impl Display for OSArch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct OSRelease (u32,u32,u32);

impl OSRelease {    
    pub fn parse_from_str(os_release: &str) -> Result<OSRelease,MigError> {
    lazy_static! {
        static ref RE_OS_VER: Regex = Regex::new(r"^(\d+)\.(\d+)\.(\d+)$").unwrap();                    
    }

    let captures = match RE_OS_VER.captures(os_release) {
        Some(c) => c,
        None => return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("OSRelease::parse_from_str: parse regex failed to parse release string: '{}'",os_release))),
    };

    let parse_capture = |i: usize| -> Result<u32,MigError> {
        match captures.get(i) {
            Some(s) => 
                Ok(s.as_str().parse::<u32>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam, 
                    &format!("OSRelease::parse_from_str: failed to parse {} part {} to u32", os_release, i)))?),            
            None => return Err(MigError::from_remark(
                                MigErrorKind::InvParam,
                                &format!("OSRelease::parse_from_str: failed to get release part {} from: '{}'", i, os_release))),
        }
    };

    if let Ok(n0) = parse_capture(1) {
        if let Ok(n1) = parse_capture(2) {
            if let Ok(n2) = parse_capture(3) {
                return Ok(OSRelease(n0,n1,n2));
            }
        }
    } 
    Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("OSRelease::parse_from_str: failed to parse release string: '{}'",os_release)))
    }
}

impl Display for OSRelease {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

pub trait Migrator {
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError>;
    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError>;
    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch, MigError>;
    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str,MigError>;
    fn get_mem_tot(&mut self) -> Result<usize,MigError>;
    fn get_mem_avail(&mut self) -> Result<usize,MigError>;
    fn is_admin(&mut self) -> Result<bool,MigError>;
    fn is_secure_boot(&mut self) -> Result<bool,MigError>;
    fn can_migrate(&mut self) -> Result<bool,MigError>; 
    fn migrate(&mut self) -> Result<(),MigError>;  
}


#[cfg(target_os = "windows")]
pub fn get_migrator() -> Result<Box<Migrator>,MigError> {
    use mswin::MSWMigrator;
    Ok(Box::new(MSWMigrator::try_init()?))
}

#[cfg(target_os = "linux")]
pub fn get_migrator() -> Result<Box<Migrator>,MigError> {
    use linux::LinuxMigrator;
    Ok(Box::new(LinuxMigrator::new()))
}

