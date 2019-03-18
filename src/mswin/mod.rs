mod powershell;
mod win_api;
mod wmi_utils;

use log::{info, trace, error};

use failure::{ResultExt};
use wmi_utils::{WmiUtils,WMIOSInfo};

use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};

use crate::{OSRelease, OSArch, Migrator};

use powershell::{PSInfo};

const MODULE: &str = "mswin";


pub fn get_migrator() -> Result<Box<Migrator>,MigError> {


    Ok(Box::new(MSWInfo::try_init()?))
    //Err(MigError::from(MigErrorKind::NotImpl))
}

struct MSWInfo {
    ps_info: PSInfo,
    wmi_utils: WmiUtils,
    os_info: Option<WMIOSInfo>,
}

impl MSWInfo {
    fn try_init() -> Result<MSWInfo, MigError> {
        let msw_info = MSWInfo {
            ps_info: PSInfo::try_init()?,
            wmi_utils: WmiUtils::new()?,
            os_info: None,
        };
        Ok(msw_info)
    }
}

impl Migrator for MSWInfo {
    fn can_migrate(&mut self) -> Result<bool,MigError> {


        Err(MigError::from(MigErrorKind::NotImpl))
    }
    
    fn migrate(&mut self) -> Result<(),MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_name),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_name)              
            },
        }
    }

    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_release),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_release)              
            },
        }
    }

    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_arch),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_arch)              
            },
        }
    }


    fn get_mem_tot(&mut self) -> Result<usize,MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_tot),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_tot)              
            },
        }
    }

    fn get_mem_avail(&mut self) -> Result<usize,MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_avail),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_avail)              
            },
        }
    }

    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str,MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.boot_dev),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().boot_dev)              
            },
        }
    }

    fn is_admin(&mut self) -> Result<bool,MigError> {
        Ok(self.ps_info.is_admin()?)
    }
    
    fn is_secure_boot(&mut self) -> Result<bool,MigError> {
        // TODO: implement
        Err(MigError::from(MigErrorKind::NotImpl))
    }

}

pub fn available() -> bool {
    trace!("called available()");
    return cfg!(windows);
}

pub fn process() -> Result<(), MigError> {
    let mut ps_info = powershell::PSInfo::try_init()?;
    // info!("process: os_type = {}", ps_info.get_os_name());
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_mswin() {
        let msw_info = MSWInfo::try_init().unwrap();
        assert!(!msw_info.get_os_name().is_empty());
        assert!(if let Some(_or) = msw_info.get_os_release() {
            true
        } else {
            false
        });
        assert!(!msw_info.get_mem_avail() > 0);
        assert!(!msw_info.get_mem_tot() > 0);
    }
}
