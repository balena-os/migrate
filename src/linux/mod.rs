use log::{info, trace, error};
use failure::{ResultExt};
use std::io::Read;
use std::fs::File;

// use std::os::linux::{};
use lazy_static::lazy_static;
use regex::Regex;

mod util;



use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};
use crate::{OSRelease, OSArch, Migrator};

const MODULE: &str = "Linux";
const OS_NAME_RE: &str = r#"^PRETTY_NAME="([^"]+)"$"#;
const OS_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";


pub fn get_migrator() -> Result<Box<Migrator>,MigError> {
    Ok(Box::new(LinuxMigrator::new()))
    //Err(MigError::from(MigErrorKind::NotImpl))
}

pub(crate) struct LinuxMigrator {
    os_name: Option<String>,
    os_release: Option<OSRelease>,
}

impl LinuxMigrator {
    pub fn new() -> LinuxMigrator {
        LinuxMigrator{
            os_name: None,
            os_release: None,
        }
    } 
}



impl Migrator for LinuxMigrator {
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError> {
        // TODO: ensure availabilty of method
        match self.os_name {
            Some(ref s) => Ok(s),
            None => {
                lazy_static! {
                    static ref RE: Regex = Regex::new(OS_NAME_RE).unwrap();                    
                    // static ref RE: Regex = Regex::new("^PRETTY_NAME=\"([^\"]+)$").unwrap();                    
                }

                self.os_name = Some(util::parse_file("/etc/os-release", &RE)?);
                Ok(self.os_name.as_ref().unwrap())
            }
        }
    }

    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError> {
        match self.os_release {
            Some(ref s) => Ok(s),
            None => {
                let mut os_info = String::new();

                File::open(OS_RELEASE_FILE).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("File open '{}'",OS_RELEASE_FILE)))?
                    .read_to_string(&mut os_info).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("File read '{}'",OS_RELEASE_FILE)))?;

                self.os_release = Some(OSRelease::parse_from_str(&os_info.trim())?);

                Ok(self.os_release.as_ref().unwrap())
            }
        }
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
    fn can_migrate(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn migrate(&mut self) -> Result<(),MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }  
}