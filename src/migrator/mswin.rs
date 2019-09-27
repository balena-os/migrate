use failure::{Fail, ResultExt};
use log::{error, info, trace};
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

// TODO: Require files to be in work_dir

use crate::{
    common::{
        dir_exists, path_append, stage2_config::Stage2ConfigBuilder, Config, MigErrCtx, MigError,
        MigErrorKind, MigMode,
        migrate_info::{MigrateInfo, balena_cfg_json::BalenaCfgJson},
        boot_manager::BootManager,
        device::Device,
    },
    defs::{DeviceType, OSArch, STAGE2_CFG_FILE},
    mswin::util::to_linux_path,
    mswin::wmi_utils::WmiUtils,
};

pub(crate) mod msw_defs;
// use defs::{STAGE2_CFG_FILE, STAGE2_CFG_DIR};

mod mswin_api;

mod powershell;
use powershell::PSInfo;

//pub(crate) mod win_api;
// pub mod drive_info;
mod win_api;

mod util;

mod wmi_utils;

//mod migrate_info;
//use migrate_info::MigrateInfo;

// mod boot_manager;
//use boot_manager::{BootManager, EfiBootManager};

pub struct MSWMigrator {
    config: Config,
    mig_info: MigrateInfo,
    ps_info: PSInfo,
    stage2_config: Stage2ConfigBuilder,
    boot_manager: Box<BootManager>,
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
            MigMode::Immediate => migrator.do_migrate(),
            MigMode::Pretend => Ok(()),
            MigMode::Agent => Err(MigError::from(MigErrorKind::NotImpl)),
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

        let wmi_info = WmiUtils::get_os_info()?;


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
                error!(
                    "The {:?} OS architecture is not currently supported on windows devices",
                    mig_info.os_arch
                );
                return Err(MigError::displayed());
            }
        }

        let mut boot_manager = if mig_info.efi_boot {
            Box::new(EfiBootManager::new())
        } else {
            panic!("no choices for non efi boot manager");
        };

        if !boot_manager.can_migrate(&mig_info, &config, &mut stage2_config)? {
            error!("Cannot migrate this device.");
            return Err(MigError::displayed());
        }

        /* likely to be wrong, let stage2 figure it out from /root device
        stage2_config.set_flash_device(&PathBuf::from(
            mig_info.drive_info.boot_path.get_linux_drive(),
        ));
        */

        stage2_config.set_boot_type(&boot_manager.get_boot_type());

        // TODO: Don't migrate if we do not have PARTUUIDS
        // TODO: maybe allow hints otherwise  ->

        Ok(MSWMigrator {
            ps_info,
            config,
            mig_info,
            stage2_config,
            boot_manager,
        })
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

        /* No boot device on windows
        self.stage2_config.set_boot_device(&PathBuf::from(
            self.mig_info.drive_info.boot_path.get_linux_part(),
        ));
        self.stage2_config.set_boot_fstype(&String::from(
            self.mig_info.drive_info.boot_path.get_linux_fstype(),
        ));
        */

        // later
        self.stage2_config
            .set_balena_image(PathBuf::from(&to_linux_path(
                &self.mig_info.image_file.path,
            )));
        self.stage2_config
            .set_balena_config(PathBuf::from(&to_linux_path(
                &self.mig_info.config_file.get_path(),
            )));

        self.stage2_config.set_work_dir(&PathBuf::from(
            self.mig_info.drive_info.work_path.get_linux_path(),
        ));

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

        match self
            .boot_manager
            .setup(&self.mig_info, &self.config, &mut self.stage2_config)
        {
            Ok(_s) => info!("The system is set up to boot into the migration environment"),
            Err(why) => {
                error!(
                    "Failed to set up the boot configuration for the migration environment: {:?}",
                    why
                );
                return Err(MigError::displayed());
            }
        }

        trace!("write stage 2 config");

        let stage2_cfg_path = path_append(
            self.mig_info.drive_info.boot_path.get_path(),
            STAGE2_CFG_FILE,
        );
        let stage2_cfg_dir = stage2_cfg_path.parent().unwrap();
        if !dir_exists(&stage2_cfg_dir)? {
            create_dir_all(&stage2_cfg_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Fauíled to create directory '{}'",
                    stage2_cfg_dir.display()
                ),
            ))?;
        }

        self.stage2_config.write_stage2_cfg_to(&stage2_cfg_path)?;

        if let Some(delay) = self.config.migrate.get_reboot() {
            let message = format!(
                "Migration stage 1 was successfull, rebooting system in {} seconds",
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
}

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
