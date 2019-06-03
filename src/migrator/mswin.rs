mod powershell;
//pub(crate) mod win_api;
// pub mod drive_info;
mod win_api;

mod util;

use win_api::{is_efi_boot};
mod wmi_utils;

use log::{debug, error, info, trace, warn};
// use std::collections::{HashMap};

use crate::{
    common::{call, os_release::OSRelease, Config, MigError, MigErrorKind, MigMode},
    defs::OSArch,
    mswin::util::mount_efi,
};

use wmi_utils::WMIOSInfo;
pub(crate) use wmi_utils::{Partition, WmiUtils};
//use crate::mswin::drive_info::PhysicalDriveInfo;

use powershell::PSInfo;

const MODULE: &str = "migrator::mswin";

struct SysInfo {
    os_name: Option<String>,
    os_release: Option<OSRelease>,
    os_arch: Option<OSArch>,
    efi_boot: Option<bool>,
    secure_boot: Option<bool>,
    /*
        migrate_info: Option<DiskInfo>,
        image_info: Option<FileInfo>,
        kernel_info: Option<FileInfo>,
        initrd_info: Option<FileInfo>,
        device_slug: Option<String>,
    */
}

impl SysInfo {
    pub fn default() -> SysInfo {
        SysInfo {
            os_name: None,
            os_release: None,
            os_arch: None,
            efi_boot: None,
            secure_boot: None,
            /*
                        migrate_info: None,
                        image_info: None,
                        kernel_info: None,
                        initrd_info: None,
                        device_slug: None,
            */
        }
    }

    pub fn is_efi_boot(&self) -> bool {
        if let Some(efi_boot) = self.efi_boot {
            efi_boot
        } else {
            false
        }
    }
}

// const MODULE: &str = "mswin";

pub struct MSWMigrator {
    config: Config,
    ps_info: PSInfo,
    os_info: Option<WMIOSInfo>,
    uefi_boot: Option<bool>,
    sysinfo: SysInfo,
}

impl<'a> MSWMigrator {
    pub fn migrate() -> Result<(), MigError> {
        let migrator = MSWMigrator::try_init(Config::new()?)?;
        match migrator.config.migrate.get_mig_mode() {
            MigMode::IMMEDIATE => migrator.do_migrate(),
            MigMode::PRETEND => Ok(()),
            MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
        }
    }

    fn try_init(config: Config) -> Result<MSWMigrator, MigError> {
        trace!("MSWMigrator::try_int: entered");

        let mut migrator = MSWMigrator {
            config,
            ps_info: PSInfo::try_init()?,
            os_info: None,
            uefi_boot: None,
            sysinfo: SysInfo::default(),
        };

        // **********************************************************************
        // We need to be root to do this
        // note: fake admin is not honored in release mode

        if !migrator.is_admin()? {
            error!("Please run this program with adminstrator privileges");
            return Err(MigError::displayed());
        }

        migrator.os_info = Some(
            match WmiUtils::get_os_info() {
                Ok(os_info) => {
                    info!(
                        "OS Architecture is {}, OS Name is '{}', OS Release is '{}'",
                        os_info.os_arch, os_info.os_name, os_info.os_release );
                    debug!("Boot device: '{}'", os_info.boot_dev);    
                    os_info 
                    },
                Err(why) => {
                    error!("Failed to retrieve OS info: {:?}", why);
                    return Err(MigError::displayed());
                }    
            });

        let phys_drives = match WmiUtils::query_drives() {
            Ok(phys_drives) => {
                for drive in phys_drives {
                    debug!("found drive id {}, device {}", drive.get_device_id(), drive.get_device());  
                    let partitions = match drive.query_partitions() {
                        Ok(partitions) => {
                            for partition in partitions {
                                if partition.is_boot_device() {
                                    info!("Boot partition is: '{}' type: '{}' on drive '{}'", 
                                    partition.get_device(), 
                                    partition.get_ptype(),
                                    drive.get_device_id());
                                    let boot_drive = match partition.query_logical_drive() {
                                        Ok(boot_drive) => {
                                            if let Some(boot_drive) = boot_drive {
                                                info!("Boot partition is mounted on: '{}' ", boot_drive.get_name());
                                                boot_drive
                                            } else {
                                                // TODO: mount it
                                                debug!("Boot partition is not mounted",);
                                                if is_efi_boot()? {
                                                    info!("Device was booted in EFI mode, attempting to mount the EFI partition");
                                                    let efi_drive = mount_efi()?;
                                                    info!("The EFI partition was mounted on '{}'", efi_drive.get_name());
                                                    efi_drive
                                                } else {
                                                    error!("Failed to mount EFI partition for device");
                                                    return Err(MigError::displayed());
                                                }
                                            }
                                        },
                                        Err(why) => {
                                            error!("Failed to query logical drive for partition {}: {:?}", partition.get_device(), why);
                                            return Err(MigError::displayed());
                                        }
                                    };
                                } else {
                                    debug!("found partition: '{}'", partition.get_device());
                                }
                            }
                        }, 
                        Err(why) => {
                            error!("Failed to query partitions for drive {}: {:?}", drive.get_device_id(), why);
                            return Err(MigError::displayed());
                        }
                    };
                }
            }, 
            Err(why) => {
                error!("Failed to query drive info: {:?}", why);
                return Err(MigError::displayed());
            }    
        };

        if is_efi_boot()? {


            // call("mountvol", &[""])
        }

        Ok(migrator)
    }

    fn do_migrate(&self) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn get_ps_info(&'a mut self) -> &'a mut PSInfo {
        &mut self.ps_info
    }

    #[cfg(not(debug_assertions))]
    fn is_admin(&self) -> Result<bool, MigError> {
        trace!("LinuxMigrator::is_admin: entered");
        Ok(self.ps_info.is_admin()?)
    }

    #[cfg(debug_assertions)]
    fn is_admin(&self) -> Result<bool, MigError> {
        trace!("LinuxMigrator::is_admin: entered");
        Ok(self.ps_info.is_admin()? || self.config.debug.is_fake_admin())
    }

    /*
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
                warn!("{}::can_migrate: you need to run this program as admin", MODULE);
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
                let _drive_letter = mount_efi_partition();
            }

            Ok(true)
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

        fn get_os_release(&mut self) -> Result<OSRelease, MigError> {
            match self.os_info {
                Some(ref info) => Ok(info.os_release),
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
    */
}

/*
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
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_mswin() {
        // let mut msw_info = MSWMigrator::try_init().unwrap();
        // assert!(!msw_info.get_os_name().unwrap().is_empty());
        //msw_info.get_os_release().unwrap();
        //assert!(!msw_info.get_mem_avail().unwrap() > 0);
        //assert!(!msw_info.get_mem_tot().unwrap() > 0);
    }
}
