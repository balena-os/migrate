use failure::{Fail, ResultExt};
use log::{error, trace};
use std::time::{Duration};
use std::thread;
use std::path::{PathBuf};
use std::fs::{create_dir_all};

use crate::{
    defs::{DeviceType, OSArch, STAGE2_CFG_FILE},    
    common::{Config, MigErrCtx, MigError, MigErrorKind, MigMode, stage2_config::{Stage2ConfigBuilder}, path_append, dir_exists}, };


pub(crate) mod msw_defs;
// use defs::{STAGE2_CFG_FILE, STAGE2_CFG_DIR};

mod powershell;
//pub(crate) mod win_api;
// pub mod drive_info;
mod win_api;

mod util;

mod wmi_utils;

mod migrate_info;
use migrate_info::MigrateInfo;

use powershell::PSInfo;

pub struct MSWMigrator {
    config: Config,
    mig_info: MigrateInfo,
    ps_info: PSInfo,
    stage2_config: Stage2ConfigBuilder,
    /*    
        os_info: Option<WMIOSInfo>,
        efi_boot: Option<bool>,
        sysinfo: SysInfo,
    */
}

impl<'a> MSWMigrator {
    pub fn migrate() -> Result<(), MigError> {
        let mut migrator = MSWMigrator::try_init(Config::new()?)?;
        match migrator.config.migrate.get_mig_mode() {
            MigMode::IMMEDIATE => migrator.do_migrate(),
            MigMode::PRETEND => Ok(()),
            MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
        }
    }

    fn try_init(config: Config) -> Result<MSWMigrator, MigError> {
        trace!("try_int: entered");

        let mut ps_info = PSInfo::try_init()?;

        trace!("PSInfo initialised");
        // **********************************************************************
        // We need to be root to do this
        // note: fake admin is not honored in release mode

        if !ps_info.is_admin()? {
            error!("Please run this program with adminstrator privileges");
            return Err(MigError::displayed());
        }

        let mig_info = match MigrateInfo::new(&config, &mut ps_info) {
            Ok(mig_info) => mig_info,
            Err(why) => {
                return match why.kind() {
                    MigErrorKind::Displayed => Err(why),
                    _ => {
                        error!("Failed to create MigrateInfo: {:?}", why);
                        Err(MigError::from(
                            why.context(MigErrCtx::from(MigErrorKind::Displayed)),
                        ))
                    }
                };
            }
        };

        let mut stage2_config = Stage2ConfigBuilder::default();
        match mig_info.os_arch {
            OSArch::AMD64 => stage2_config.set_device_type(&DeviceType::IntelNuc),
            _ => {
                    error!("The {:?} OS architecture is not currently supported on windows devices", mig_info.os_arch);
                    return Err(MigError::displayed());
            },
        }
        
        stage2_config.set_flash_device(&PathBuf::from(mig_info.drive_info.boot_path.get_linux_drive()));
        stage2_config.set_boot_type(&mig_info.boot_type);

        Ok(MSWMigrator { ps_info, config, mig_info, stage2_config})
    }

    fn do_migrate(&mut self) -> Result<(), MigError> {
        

        // TODO: take care of backup
        self.stage2_config.set_has_backup(false);


        // *****************************************************************************************
        // Finish Stage2ConfigBuilder & create stage2 config file

        self.stage2_config
            .set_failmode(self.config.migrate.get_fail_mode());

        self.stage2_config
            .set_no_flash(self.config.debug.is_no_flash());

        self.stage2_config
            .set_skip_flash(self.config.debug.is_skip_flash());

        self.stage2_config
            .set_boot_device(&PathBuf::from(self.mig_info.drive_info.boot_path.get_linux_part()));
        self.stage2_config
            .set_boot_fstype(&String::from(self.mig_info.drive_info.boot_path.get_linux_fstype()));

        // later
        self.stage2_config
            .set_balena_image(PathBuf::from(&self.mig_info.image_file.path));
        self.stage2_config
            .set_balena_config(PathBuf::from(self.mig_info.config_file.get_path()));

        self.stage2_config
            .set_work_dir(&PathBuf::from(self.mig_info.drive_info.work_path.get_path()));

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        self.stage2_config
            .set_log_level(String::from(self.config.migrate.get_log_level()));

/*        if let Some((ref device, ref fstype)) = self.mig_info.log_path {
            self.stage2_config.set_log_to(Stage2LogConfig {
                device: device.clone(),
                fstype: fstype.clone(),
            });
        }
*/            

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        trace!("device setup");

/*        self.device.setup(
            &self.cmds,
            &mut self.mig_info,
            &self.config,
            &mut self.stage2_config,
        )?;
*/
  
        trace!("write stage 2 config");

        let stage2_cfg_path = path_append(self.mig_info.drive_info.boot_path.get_path(),STAGE2_CFG_FILE);
        let stage2_cfg_dir = stage2_cfg_path.parent().unwrap();
        if !dir_exists(&stage2_cfg_dir)? {
            create_dir_all(&stage2_cfg_dir).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Fauíled to create directory '{}'", stage2_cfg_dir.display())))?;
        } 
        self.stage2_config.write_stage2_cfg_to(&stage2_cfg_path)?;
      
        if let Some(delay) = self.config.migrate.get_reboot() {
            let message = format!("Migration stage 1 was successfull, rebooting system in {} seconds",
                *delay
            );
            println!("{}", &message);
            
            let delay = Duration::new(*delay, 0);
            thread::sleep(delay);
            println!("Rebooting now..");            

            let _res = self.ps_info.reboot();
        }

        Ok(())
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
