pub(crate) mod powershell;
//pub(crate) mod win_api;
pub mod win_api;
pub(crate) mod wmi_utils;
pub mod drive_info;

use log::{trace};


use wmi_utils::{WmiUtils,WMIOSInfo};

use crate::mig_error::{MigError,MigErrorKind};

use crate::{OSRelease, OSArch, Migrator};

use powershell::{PSInfo};

// const MODULE: &str = "mswin";

pub(crate) struct MSWMigrator {
    ps_info: PSInfo,
    wmi_utils: WmiUtils,
    os_info: Option<WMIOSInfo>,
    uefi_boot: Option<bool>,
}

impl MSWMigrator {
    pub fn try_init() -> Result<MSWMigrator, MigError> {
        let msw_info = MSWMigrator {
            ps_info: PSInfo::try_init()?,
            wmi_utils: WmiUtils::new()?,
            os_info: None,
            uefi_boot: None,
        };
        Ok(msw_info)
    }
}

impl Migrator for MSWMigrator {
    fn can_migrate(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn migrate(&mut self) -> Result<(),MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn is_uefi_boot(&mut self) -> Result<bool,MigError> {
        match self.uefi_boot {
            Some(v) => Ok(v),
            None => {
                self.uefi_boot = Some(win_api::is_uefi_boot()?);
                Ok(self.uefi_boot.unwrap())
            }
        }
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


    fn get_mem_tot(&mut self) -> Result<u64,MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_tot),
            None => {
                self.os_info = Some(self.wmi_utils.init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_tot)              
            },
        }
    }

    fn get_mem_avail(&mut self) -> Result<u64,MigError> {
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
        Ok(self.ps_info.is_secure_boot()?)
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
        let mut msw_info = MSWMigrator::try_init().unwrap();
        assert!(!msw_info.get_os_name().unwrap().is_empty());
        msw_info.get_os_release().unwrap();
        assert!(!msw_info.get_mem_avail().unwrap() > 0);
        assert!(!msw_info.get_mem_tot().unwrap() > 0);
    }
}
