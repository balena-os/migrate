use failure::{Fail, ResultExt};
use log::{debug, error, info, trace};
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

// TODO: Require files to be in work_dir

use crate::{
    common::{
        boot_manager::BootManager,
        config::balena_config::ImageType,
        device::Device,
        dir_exists, format_size_with_unit,
        migrate_info::{balena_cfg_json::BalenaCfgJson, MigrateInfo},
        path_append,
        stage2_config::Stage2ConfigBuilder,
        Config, MigErrCtx, MigError, MigErrorKind, MigMode,
    },
    defs::{DeviceType, OSArch, MIN_DISK_SIZE, STAGE2_CFG_FILE},
    mswin::{mswin_api::MSWinApi, util::to_linux_path, wmi_utils::WmiUtils},
};

pub(crate) mod msw_defs;
// use defs::{STAGE2_CFG_FILE, STAGE2_CFG_DIR};

mod mswin_api;

mod powershell;
use powershell::{is_admin, is_secure_boot};

//pub(crate) mod win_api;
// pub mod drive_info;
mod win_api;
use win_api::is_efi_boot;

mod util;

mod device_impl;

pub(crate) mod drive_info;

pub(crate) mod wmi_utils;

mod boot_manager_impl;
use boot_manager_impl::efi_boot_manager::EfiBootManager;

//mod migrate_info;
//use migrate_info::MigrateInfo;

// mod boot_manager;
//use boot_manager::{BootManager, EfiBootManager};

pub struct MSWMigrator {
    config: Config,
    mig_info: MigrateInfo,
    device: Box<dyn Device>,
    stage2_config: Stage2ConfigBuilder,
}

impl<'a> MSWMigrator {
    pub fn migrate() -> Result<(), MigError> {
        let mut migrator = MSWMigrator::try_init(Config::new()?)?;
        match migrator.config.migrate.get_mig_mode() {
            MigMode::Immediate => migrator.do_migrate(),
            MigMode::Pretend => Ok(()),
            //MigMode::Agent => Err(MigError::from(MigErrorKind::NotImpl)),
        }
    }

    fn try_init(config: Config) -> Result<MSWMigrator, MigError> {
        trace!("MSWinMigrator::try_init: entered");

        // **********************************************************************
        // We need to be root to do this
        // note: fake admin is not honored in release mode

        if !is_admin()? {
            error!("Please run this program with adminstrator privileges");
            return Err(MigError::displayed());
        }

        let mswin_api = MSWinApi::new()?;

        let mig_info = match MigrateInfo::new(&config, &mswin_api) {
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
        let device = match device_impl::get_device(&mig_info, &config, &mut stage2_config) {
            Ok(device) => {
                let dev_type = device.get_device_type();
                let boot_type = device.get_boot_type();
                info!("Device Type is {:?}", device.get_device_type());
                info!("Boot mode is {:?}", boot_type);
                stage2_config.set_device_type(&dev_type);
                stage2_config.set_boot_type(&boot_type);
                device
            }
            Err(why) => {
                return match why.kind() {
                    MigErrorKind::Displayed => Err(why),
                    _ => {
                        error!("Failed to create Device: {:?}", why);
                        Err(MigError::from(
                            why.context(MigErrCtx::from(MigErrorKind::Displayed)),
                        ))
                    }
                };
            }
        };

        match mig_info
            .config_file
            .check(&config, device.get_device_slug())
        {
            Ok(_dummy) => info!(
                "The sanity check on '{}' passed",
                mig_info.config_file.get_rel_path().display()
            ),
            Err(why) => {
                let message = format!(
                    "The sanity check on '{}' failed: {:?}",
                    mig_info.config_file.get_rel_path().display(),
                    why
                );
                error!("{}", message);
                return Err(MigError::from(
                    why.context(MigErrCtx::from(MigErrorKind::Displayed)),
                ));
            }
        }

        debug!("Finished architecture dependant initialization");

        // **********************************************************************
        // Pick the current root device as flash device

        let boot_info = device.get_boot_device();
        let flash_device = &boot_info.device_info.drive;
        let flash_dev_size = boot_info.device_info.drive_size;

        info!(
            "The install drive is {}, size: {}",
            boot_info.device_info.drive,
            format_size_with_unit(flash_dev_size)
        );

        if let ImageType::FileSystems(ref fs_dump) = config.balena.get_image_path() {
            if fs_dump.device_slug != device.get_device_slug() {
                error!(
                    "The device-slug of the image dump configuration differs from the detect device slug '{}' != '{}'",
                    fs_dump.device_slug,
                    device.get_device_slug()
                );
                return Err(MigError::from(MigErrorKind::Displayed));
            }
        }

        // TODO: check available space for work files here if work is not on a distinct partition

        // **********************************************************************
        // Require a minimum disk device size for installation

        if flash_dev_size < MIN_DISK_SIZE {
            error!(
                "The size of the install drive '{}' = {} is too small to install balenaOS",
                flash_device,
                format_size_with_unit(flash_dev_size)
            );
            return Err(MigError::from(MigErrorKind::Displayed));
        }

        // TODO: Don't migrate if we do not have PARTUUIDS
        // TODO: maybe allow hints otherwise  ->

        Ok(MSWMigrator {
            config,
            mig_info,
            device,
            stage2_config,
        })
    }

    fn do_migrate(&mut self) -> Result<(), MigError> {
        // TODO: take care of backup
        /*
                self.stage2_config.set_has_backup(false);

                // *****************************************************************************************
                // Finish Stage2ConfigBuilder & create stage2 config file

                self.stage2_config
                    .set_failmode(self.config.migrate.get_fail_mode());

                self.stage2_config
                    .set_no_flash(self.config.debug.is_no_flash());

                let device =
                    match device_impl::get_device(&self.mig_info, &self.config, &mut self.stage2_config) {
                        Ok(device) => {
                            self.stage2_config.set_boot_type(&device.get_boot_type());
                            self.stage2_config
                                .set_device_type(&device.get_device_type());
                            device
                        }
                        Err(why) => {
                            return match why.kind() {
                                MigErrorKind::Displayed => Err(why),
                                _ => {
                                    error!("Failed to create Device: {:?}", why);
                                    Err(MigError::from(
                                        why.context(MigErrCtx::from(MigErrorKind::Displayed)),
                                    ))
                                }
                            };
                        }
                    };

                        self.stage2_config.set_boot_device(&PathBuf::from(
                            self.mig_info.drive_info.boot_path.get_linux_part(),
                        ));
                self.stage2_config.set_boot_fstype(&String::from(
                    self.mig_info.drive_info.boot_path.get_linux_fstype(),
                ));


                // later
                self.stage2_config
                    .set_balena_image(self.mig_info.image_file.clone());

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
                        &format!("Fauíled to create directory '{}'", stage2_cfg_dir.display()),
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
        */
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
