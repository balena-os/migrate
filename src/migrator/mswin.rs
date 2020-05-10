use failure::{Fail, ResultExt};
use log::{debug, error, info, trace, warn};
use mod_logger::{LogDestination, Logger};
use std::fs::{copy, create_dir, read_dir};
use std::thread;
use std::time::Duration;

// TODO: Require files to be in work_dir

use crate::{
    common::{
        // backup,
        config::balena_config::ImageType,
        device::Device,
        dir_exists,
        file_size,
        format_size_with_unit,
        migrate_info::MigrateInfo,
        os_api::OSApi,
        path_append,
        stage2_config::{MountConfig, PathType, Stage2ConfigBuilder},
        Config,
        MigErrCtx,
        MigError,
        MigErrorKind,
        MigMode,
    },
    defs::{
        BACKUP_FILE, MIN_DISK_SIZE, STAGE1_MEM_THRESHOLD, STAGE2_CFG_FILE, SYSTEM_CONNECTIONS_DIR,
    },
};

pub(crate) mod msw_defs;
// use defs::{STAGE2_CFG_FILE, STAGE2_CFG_DIR};

pub(crate) mod mswin_api;

mod powershell;
use powershell::{is_admin, reboot};

//pub(crate) mod win_api;
// pub mod drive_info;
mod win_api;

mod util;

mod device_impl;

pub(crate) mod drive_info;

pub(crate) mod wmi_utils;

mod boot_manager_impl;
use crate::common::os_api::OSApiImpl;
use crate::common::stage2_config::LogDevice;

pub struct MSWMigrator {
    config: Config,
    mig_info: MigrateInfo,
    device: Box<dyn Device>,
    stage2_config: Stage2ConfigBuilder,
}

impl<'a> MSWMigrator {
    pub fn migrate() -> Result<(), MigError> {
        // **********************************************************************
        // We need to be root to do this
        if !is_admin()? {
            error!("Please run this program with adminstrator privileges");
            return Err(MigError::displayed());
        }

        let mut migrator = MSWMigrator::try_init(Config::new()?)?;
        match migrator.config.migrate.get_mig_mode() {
            MigMode::Immediate => migrator.do_migrate(),
            MigMode::Pretend => {
                Logger::flush();
                Ok(())
            }
        }
    }

    fn try_init(config: Config) -> Result<MSWMigrator, MigError> {
        trace!("MSWinMigrator::try_init: entered");

        let log_file = path_append(config.migrate.get_work_dir(), "stage1.log");

        Logger::set_log_file(&LogDestination::Stderr, &log_file, true).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to set logging to '{}'", log_file.display()),
            ),
        )?;

        // **********************************************************************
        // We need to be root to do this
        // note: fake admin is not honored in release mode

        let mig_info = match MigrateInfo::new(&config) {
            Ok(mig_info) => {
                info!(
                    "OS Architecture is {}, OS Name is '{}'",
                    mig_info.os_arch, mig_info.os_name
                );
                mig_info
            }
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
                stage2_config.set_device_type(dev_type);
                stage2_config.set_boot_type(boot_type);
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

        debug!("try_init: success");

        Ok(MSWMigrator {
            config,
            mig_info,
            device,
            stage2_config,
        })
    }

    fn do_migrate(&mut self) -> Result<(), MigError> {
        debug!("Entered do_migrate");

        let work_dir = &self.mig_info.work_path.path;
        let boot_device = self.device.get_boot_device();

        if &self.mig_info.work_path.device_info.device == &boot_device.device_info.device {
            self.stage2_config
                .set_work_path(&PathType::Path(OSApiImpl::new()?.to_linux_path(work_dir)?));
        } else {
            // in windows the mount path is usually something like [a-z]:/ which is stripped by to_linux_path
            //let work_dir = OSApi::new()?.to_linux_path(work_dir)?;
            let work_device = &self.mig_info.work_path.device_info;
            let stripped_path = OSApiImpl::new()?.to_linux_path(
                work_dir
                    .strip_prefix(&self.mig_info.work_path.mountpoint)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        "failed to create relative work path",
                    ))?,
            )?;

            self.stage2_config
                .set_work_path(&PathType::Mount(MountConfig::new(
                    &work_device.get_alt_path(),
                    work_device.fs_type.as_str(),
                    &stripped_path,
                )));
        }

        let backup_path = path_append(work_dir, BACKUP_FILE);
        if !self.config.migrate.get_backup_volumes().is_empty() {
            // TODO: enable backup in windows - howto create backup.tgz ?
            warn!("The backup feature is not currently supported in windows - the defined backup will not be saved/restored");
        }

        self.stage2_config.set_has_backup(false);

        // TODO: this might not be a smart place to put things, everything in system-connections
        // will end up in /mnt/boot/system-connections
        trace!("nwmgr_files");
        let nwmgr_path = path_append(work_dir, SYSTEM_CONNECTIONS_DIR);

        if self.mig_info.nwmgr_files.len() > 0
            || self.mig_info.wifis.len() > 0 && !dir_exists(&nwmgr_path)?
        {
            if !dir_exists(&nwmgr_path)? {
                create_dir(&nwmgr_path).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to create directory '{}'", nwmgr_path.display()),
                ))?;
            }
        }

        if dir_exists(&nwmgr_path)? {
            for file in &self.mig_info.nwmgr_files {
                if let Some(file_name) = file.path.file_name() {
                    let tgt = path_append(&nwmgr_path, file_name);
                    copy(&file.path, &tgt).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to copy '{}' to '{}'",
                            file.path.display(),
                            tgt.display()
                        ),
                    ))?;
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::Upstream,
                        &format!("unable to processs path: '{}'", file.path.display()),
                    ));
                }
            }
        }

        trace!("do_migrate: found wifis: {}", self.mig_info.wifis.len());

        if self.mig_info.wifis.len() > 0 {
            let mut index = 0;
            for wifi in &self.mig_info.wifis {
                index = wifi.create_nwmgr_file(&nwmgr_path, index)?;
            }
        }

        let (mem_tot, mem_avail) = OSApiImpl::new()?.get_mem_info()?;
        info!(
            "Memory available is {} of {}",
            format_size_with_unit(mem_avail),
            format_size_with_unit(mem_tot),
        );

        let mut required_size: u64 = self.mig_info.image_file.get_required_space();

        required_size += self.mig_info.config_file.get_size();

        if self.stage2_config.get_has_backup() {
            required_size += file_size(&backup_path)?;
        }

        if dir_exists(&nwmgr_path)? {
            let read_dir = read_dir(&nwmgr_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to read directory '{}'", nwmgr_path.display()),
            ))?;

            for entry in read_dir {
                required_size += file_size(
                    &entry
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            "Failed to read directory entry",
                        ))?
                        .path(),
                )?;
            }
        }

        info!(
            "Memory required for copying files is {}",
            format_size_with_unit(required_size)
        );

        if mem_tot - required_size < STAGE1_MEM_THRESHOLD {
            warn!("The memory used to copy files to initramfs might not be available.");
        }

        debug!("device setup");

        // We need this before s2 config as it might still modify migrate_info
        // TODO: make setup take no s2_cfg or immutable s2_cfg and return boot_backup instead
        // TODO: make setup undoable in case something bad happens later on

        self.device
            .setup(&mut self.mig_info, &self.config, &mut self.stage2_config)?;

        debug!("stage2 config");

        // dbg!("setting up stage2_cfg");
        // *****************************************************************************************
        // Finish Stage2ConfigBuilder & create stage2 config file

        if let Some(device) = self.config.debug.get_force_flash_device() {
            warn!("Forcing flash device to '{}'", device.display());
            self.stage2_config
                .set_force_flash_device(device.to_path_buf());
        }

        self.stage2_config
            .set_failmode(self.config.migrate.get_fail_mode());

        self.stage2_config
            .set_no_flash(self.config.debug.is_no_flash());

        self.stage2_config
            .set_migrate_delay(self.config.migrate.get_delay());

        if let Some(hacks) = self.config.debug.get_hacks() {
            self.stage2_config.set_hacks(hacks)
        }

        self.stage2_config
            .set_balena_image(self.mig_info.image_file.clone());

        self.stage2_config
            .set_balena_config(self.mig_info.config_file.get_rel_path().clone());

        // TODO: setpath if on / mount else set mount

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        self.stage2_config
            .set_log_console(self.config.migrate.get_log_console());

        self.stage2_config
            .set_log_level(String::from(self.config.migrate.get_log_level()));

        if let Some(ref log_path) = self.mig_info.log_path {
            if log_path.drive != boot_device.device_info.drive {
                info!(
                    "Set up log device as '{}'",
                    log_path.get_alt_path().display()
                );

                self.stage2_config.set_log_to(LogDevice {
                    device: log_path.get_alt_path(),
                    fs_type: log_path.fs_type.clone(),
                });
            } else {
                warn!("Log partition '{}' is not on a distinct drive from flash drive: '{}' - ignoring", log_path.drive, boot_device.device_info.drive);
            }
        }

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        trace!("write stage 2 config");
        let s2_path = path_append(&boot_device.mountpoint, STAGE2_CFG_FILE);
        self.stage2_config.write_stage2_cfg_to(&s2_path)?;

        if let Some(delay) = self.config.migrate.get_reboot() {
            println!(
                "Migration stage 1 was successfull, rebooting the system in {} seconds",
                *delay
            );

            Logger::flush();

            let delay = Duration::new(*delay, 0);
            thread::sleep(delay);
            println!("Rebooting now..");
            Logger::flush();

            if let Err(why) = reboot() {
                error!("Failed to reboot device: error {:?}", why);
            }
        } else {
            println!(
                "Migration stage 1 was successful, please reboot the system to finalize migration"
            );
            Logger::flush();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn init_mswin() {
        // let mut msw_info = MSWMigrator::try_init().unwrap();
        // assert!(!msw_info.get_os_name().unwrap().is_empty());
        //msw_info.get_os_release().unwrap();
        //assert!(!msw_info.get_mem_avail().unwrap() > 0);
        //assert!(!msw_info.get_mem_tot().unwrap() > 0);
    }
}
