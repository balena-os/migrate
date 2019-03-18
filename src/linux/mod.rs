use log::{info, trace, error};

use failure::{ResultExt};

use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};
use crate::{OSRelease, OSArch, Migrator};

pub fn get_migrator() -> Result<Box<Migrator>,MigError> {
    Ok(Box::new(LinuxMigrator::new()))
    //Err(MigError::from(MigErrorKind::NotImpl))
}

pub struct LinuxMigrator {}

impl LinuxMigrator {
    pub fn new() -> LinuxMigrator {
        LinuxMigrator{}
    } 
}

impl Migrator for LinuxMigrator {
    fn can_migrate(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_mem_tot(&mut self) -> Result<usize,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_mem_avail(&mut self) -> Result<usize,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn is_admin(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn is_secure_boot(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn migrate(&mut self) -> Result<(),MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }  
}