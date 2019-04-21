pub(crate) mod powershell;
//pub(crate) mod win_api;
// pub mod drive_info;
pub mod win_api;
pub(crate) mod wmi_utils;

use log::{debug, warn, info};
// use std::collections::{HashMap};

use std::time::Instant;

pub use wmi_utils::{WmiUtils, Partition};
use wmi_utils::{WMIOSInfo};
use crate::migrator::{
    MigError, 
    MigErrorKind,
    Migrator, 
    OSArch, 
    OSRelease,
    Config,
    common::check_tcp_connect,
};
//use crate::mswin::drive_info::PhysicalDriveInfo;

use powershell::PSInfo;

const MODULE: &str = "migrator::mswin";

// const MODULE: &str = "mswin";

pub struct MSWMigrator {
    config: Config,
    ps_info: PSInfo,
    os_info: Option<WMIOSInfo>,
    uefi_boot: Option<bool>,
}

impl<'a> MSWMigrator {
    pub fn try_init(config: Config) -> Result<MSWMigrator, MigError> {
        let msw_info = MSWMigrator {
            config, 
            ps_info: PSInfo::try_init()?,            
            os_info: None,
            uefi_boot: None,
        };
        Ok(msw_info)
    }

    pub(crate) fn get_ps_info(&'a mut self) -> &'a mut  PSInfo {
        &mut self.ps_info
    }
}

impl Migrator for MSWMigrator {
    fn can_migrate(&mut self) -> Result<bool, MigError> {
        debug!("{}::can_migrate: entered", MODULE);

        let os_name = String::from(self.get_os_name()?); 
        let os_release = self.get_os_release()?;

        info!("{}::can_migrate: running on {} release: {}", MODULE, os_name, os_release);
        
        if  (os_release.get_mayor() < 6) || ((os_release.get_mayor() == 6) && (os_release.get_minor() < 3)) ||
            (os_release.get_mayor() > 10) || ((os_release.get_mayor() == 10) && (os_release.get_minor() > 0))
            {                    
            warn!("{}::can_migrate: Windows OS releases < 8.1 (version < 6.3) or > 10 (version > 10.0) are current not supported", MODULE);
            return Ok(false);
        }

        let os_arch = self.get_os_arch()?;
        match  os_arch {
            OSArch::AMD64 => {
                info!("{}::can_migrate: using ARCH: {}", MODULE, os_arch);
            },
            _ => {
                warn!("{}::can_migrate: the achitecture {} is not currently supported for this platform", MODULE, os_arch);
                return Ok(false);
            }
        }

        if ! self.is_admin()? {
            warn!("{}::can_migrate: you need to run this program as root", MODULE);
            return Ok(false);
        }


        if self.is_secure_boot()? {
            warn!("{}::can_migrate: secure boot appears to be enabled. Please disable secure boot in the firmaware settings.", MODULE);
            return Ok(false);
        }

        if let Some(ref balena) = self.config.balena {
            if balena.api_check == true {
                info!("{}::can_migrate: checking connection api backend at to {}:{}", MODULE, balena.api_host, balena.api_port );
                let now = Instant::now();
                if let Err(why) = check_tcp_connect(&balena.api_host, balena.api_port, balena.check_timeout) {
                    warn!("{}::can_migrate: connectivity check to {}:{} failed timeout {} seconds ", MODULE, balena.api_host, balena.api_port, balena.check_timeout );
                    warn!("{}::can_migrate: check_tcp_connect returned: {:?} ", MODULE, why );
                    return Ok(false);
                }
                info!("{}::can_migrate: successfully connected to api backend in {} ms", MODULE, now.elapsed().as_millis());
            }

            if balena.vpn_check == true {
                info!("{}::can_migrate: checking connection vpn backend at to {}:{}", MODULE, balena.vpn_host, balena.vpn_port);
                let now = Instant::now();
                if let Err(why) = check_tcp_connect(&balena.vpn_host, balena.vpn_port, balena.check_timeout) {
                    warn!("{}::can_migrate: connectivity check to {}:{} failed timeout {} seconds ", MODULE, balena.vpn_host, balena.vpn_port, balena.check_timeout );
                    warn!("{}::can_migrate: check_tcp_connect returned: {:?} ", MODULE, why );
                    return Ok(false);
                }
                info!("{}::can_migrate: successfully connected to vpn backend in {} ms", MODULE, now.elapsed().as_millis());
            }
        }    

        if self.is_uefi_boot()? == true {
            let drive_letter = mount_efi_partition();
        }    

        Ok(true)
    }

    fn migrate(&mut self) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn is_uefi_boot(&mut self) -> Result<bool, MigError> {
        match self.uefi_boot {
            Some(v) => Ok(v),
            None => {
                self.uefi_boot = Some(win_api::is_uefi_boot()?);
                Ok(self.uefi_boot.unwrap())
            }
        }
    }

    fn get_os_name<'a>(&'a mut self) -> Result<&'a str, MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_name),
            None => {
                self.os_info = Some(WmiUtils::init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_name)
            }
        }
    }

    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease, MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_release),
            None => {
                self.os_info = Some(WmiUtils::init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_release)
            }
        }
    }

    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch, MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.os_arch),
            None => {
                self.os_info = Some(WmiUtils::init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().os_arch)
            }
        }
    }

    fn get_mem_tot(&mut self) -> Result<u64, MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_tot),
            None => {
                self.os_info = Some(WmiUtils::init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_tot)
            }
        }
    }

    fn get_mem_avail(&mut self) -> Result<u64, MigError> {
        match self.os_info {
            Some(ref info) => Ok(info.mem_avail),
            None => {
                self.os_info = Some(WmiUtils::init_os_info()?);
                Ok(self.os_info.as_ref().unwrap().mem_avail)
            }
        }
    }

    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str, MigError> {
        match self.os_info {
            Some(ref info) => Ok(&info.boot_dev),
            None => {
                self.os_info = Some(WmiUtils::init_os_info()?);
                Ok(&self.os_info.as_ref().unwrap().boot_dev)
            }
        }
    }

#[cfg(debug_assertions)]
    fn is_admin(&mut self) -> Result<bool, MigError> {
        if self.config.debug.fake_admin == true {
            Ok(true)
        } else {
            Ok(self.ps_info.is_admin()?)
        }
    }

#[cfg(not(debug_assertions))]
    fn is_admin(&mut self) -> Result<bool, MigError> {
        Ok(self.ps_info.is_admin()?)
    }

    fn is_secure_boot(&mut self) -> Result<bool, MigError> {
        Ok(self.ps_info.is_secure_boot()?)
    }    
}

pub fn available() -> bool {
    debug!("called available()");
    return cfg!(windows);
}

pub fn process() -> Result<(), MigError> {
    let _ps_info = powershell::PSInfo::try_init()?;
    // TODO: implement
    // info!("process: os_type = {}", ps_info.get_os_name());
    Ok(())
}

fn mount_efi_partition() -> Result<String, MigError> {
    debug!("{}::mount_efi_partition: wmi query for boot partition", MODULE);
    let boot_partition = Partition::get_boot_partition()?;
    debug!("{}::mount_efi_partition: wmi query for boot partition returned {:?}", MODULE, boot_partition);
    if boot_partition.len() != 1 {
        return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::mount_efi_partition: encountered more than 1 boot partition", MODULE)));
    } 
        
    if let Some(drive) = boot_partition[0].query_logical_drive()? {
        return Ok(String::from(drive.get_name()));
    }

    // No logical drive mounted yet



/*
        let drive_letters = WmiUtils::query_drive_letters()?;                
        let mut efi_mount = 'B' as u8;
        let mut efi_drive_letter = format!("{}:",efi_mount as char);

        if drive_letters.len() > 0 {            
            if &drive_letters[0] <= &efi_drive_letter {
                for letter in drive_letters {
                    if letter == efi_drive_letter {
                        if efi_drive_letter == "Z:" {
                            warn!("{}::can_migrate: unable to find free drive letter for efi drive: ", MODULE, why );
                        }
                        efi_mount += 1;
                        if()
                        efi_drive_letter

                    } else {
                        if letter > efi_drive_letter {
                            break;
                        }
                    }
                }
            }
        }
        let efi_mount = efi_mount as char;


        for letter in drive_letters {
            debug!("{}::can_migrate: drive letter: '{}'",MODULE, letter);
        }

*/

    Err(MigError::from(MigErrorKind::NotImpl))
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

