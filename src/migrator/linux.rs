use failure::{Fail, ResultExt};
use log::{debug, error, info, trace, warn};
use nix::unistd::sync;
use std::fs::{copy, create_dir, read_dir};
use std::thread;
use std::time::Duration;

// TODO: Require files to be in work_dir: balena-image, balena-config, system-connections

use crate::{
    common::{
        assets::Assets,
        backup, call,
        config::{ImageSource, ImageType},
        device::Device,
        dir_exists, file_size, format_size_with_unit,
        image_retrieval::download_image,
        migrate_info::MigrateInfo,
        path_append,
        stage2_config::{MountConfig, PathType, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind, MigMode,
    },
    defs::{
        BACKUP_FILE, MIN_DISK_SIZE, STAGE1_MEM_THRESHOLD, STAGE2_CFG_FILE, SYSTEM_CONNECTIONS_DIR,
    },
};

pub(crate) mod linux_defs;
use linux_defs::{
    CHMOD_CMD, DF_CMD, FILE_CMD, LSBLK_CMD, MKTEMP_CMD, MOUNT_CMD, REBOOT_CMD, TAR_CMD, UNAME_CMD,
};

pub(crate) mod device_impl;

pub(crate) mod boot_manager_impl;

pub(crate) mod stage2;

pub(crate) mod linux_api;

pub(crate) mod lsblk_info;
//pub(crate) use lsblk_info::LsblkInfo;

pub(crate) mod disk_util;

pub(crate) mod linux_common;
use crate::common::stage2_config::LogDevice;
use crate::defs::VERSION;
use crate::linux::linux_common::{get_mem_info, whereis};
pub(crate) use linux_common::is_admin;
use mod_logger::{LogDestination, Logger};

const REQUIRED_CMDS: &[&str] = &[
    // TODO: check this
    DF_CMD, LSBLK_CMD, FILE_CMD, UNAME_CMD, MOUNT_CMD, REBOOT_CMD, CHMOD_CMD, MKTEMP_CMD, TAR_CMD,
];

pub(crate) struct LinuxMigrator {
    mig_info: MigrateInfo,
    config: Config,
    stage2_config: Stage2ConfigBuilder,
    device: Box<dyn Device>,
}

impl<'a> LinuxMigrator {
    pub fn migrate(assets: Assets) -> Result<(), MigError> {
        // **********************************************************************
        // We need to be root to do this
        let config = Config::new()?;
        info!("balena-migrate {}", VERSION);

        if !is_admin()? {
            error!("please run this program as root");
            return Err(MigError::from(MigErrorKind::Displayed));
        }

        let mut migrator = LinuxMigrator::try_init(config, assets)?;

        match migrator.config.get_mig_mode() {
            MigMode::Immediate => migrator.do_migrate(),
            MigMode::Pretend => {
                Logger::flush();
                sync();
                Ok(())
            }
        }
    }

    // **********************************************************************
    // ** Initialise migrator
    // **********************************************************************

    pub fn try_init(config: Config, assets: Assets) -> Result<LinuxMigrator, MigError> {
        trace!("LinuxMigrator::try_init: entered");

        info!("migrate mode: {:?}", config.get_mig_mode());

        let log_file = path_append(config.get_work_dir(), "stage1.log");

        Logger::set_log_file(&LogDestination::Stderr, &log_file, true).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to set logging to '{}'", log_file.display()),
            ),
        )?;

        // A simple replacement for ensured commands
        for command in REQUIRED_CMDS {
            match whereis(command) {
                Ok(_cmd_path) => (),
                Err(why) => {
                    error!(
                        "Could not find required command: '{}': error: {:?}",
                        command, why
                    );
                    return Err(MigError::displayed());
                }
            }
        }

        let asset_ver = assets.get_version()?;
        info!("balena-migrator for device type: {}", asset_ver.device);
        info!("balena-migrator kernel version: {}", asset_ver.kernel);
        info!("balena-migrator balena version: {}", asset_ver.balena);

        // **********************************************************************
        // Get os architecture & name & disk properties, check required paths
        // find wifis etc..

        let mut mig_info = match MigrateInfo::new(&config, assets) {
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

        // **********************************************************************
        // Run the architecture dependent part of initialization
        // Add further architectures / functons in device_impl.rs

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

        if asset_ver.device != device.get_device_slug() {
            error!(
                "Asset device mismatch, assets for '{}', device type: '{}' ",
                asset_ver.device,
                device.get_device_slug()
            );
            return Err(MigError::displayed());
        }

        let os_image = config.get_image_path();
        match &os_image {
            ImageType::Flasher(flasher_img) => match flasher_img {
                ImageSource::Version(img_ver) => {
                    let img_file =
                        download_image(&mut mig_info, device.get_device_slug(), &img_ver)?;
                    mig_info.set_os_image(&ImageType::Flasher(ImageSource::File(img_file)))?;
                }
                ImageSource::File(_) => {
                    mig_info.set_os_image(&os_image)?;
                }
            },
            ImageType::FileSystems(fs_dump) => {
                if fs_dump.device_slug != device.get_device_slug() {
                    error!(
                        "The device-slug of the image dump configuration differs from the detect device slug '{}' != '{}'",
                        fs_dump.device_slug,
                        device.get_device_slug()
                    );
                    return Err(MigError::from(MigErrorKind::Displayed));
                }
                mig_info.set_os_image(&os_image)?;
            }
        }

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
            flash_device,
            format_size_with_unit(flash_dev_size)
        );

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

        Ok(LinuxMigrator {
            mig_info,
            config,
            device,
            stage2_config,
        })
    }

    // **********************************************************************
    // ** Start the actual migration
    // **********************************************************************

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    fn do_migrate(&mut self) -> Result<(), MigError> {
        trace!("Entered do_migrate");
        let work_dir = &self.mig_info.work_path.path;
        let boot_device = self.device.get_boot_device();

        //if &self.mig_info.work_path.device_info.device == &boot_device.device {
        if self.mig_info.work_path.device_info.device == boot_device.device_info.device {
            self.stage2_config
                .set_work_path(&PathType::Path(self.mig_info.work_path.path.clone()));
        } else {
            //let (_lsblk_device, lsblk_part) = os_api.get_lsblk_info()?.get_path_devs(&work_dir)?;
            let work_device = &self.mig_info.work_path.device_info;
            self.stage2_config
                .set_work_path(&PathType::Mount(MountConfig::new(
                    &work_device.get_alt_path(),
                    work_device.fs_type.as_str(),
                    work_dir
                        .strip_prefix(&self.mig_info.work_path.mountpoint)
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "failed to create relative work path from '{}'",
                                work_dir.display()
                            ),
                        ))?,
                )));
        }

        trace!("backup");

        let backup_path = path_append(work_dir, BACKUP_FILE);

        let has_backup = self
            .stage2_config
            .set_has_backup(if self.config.is_tar_internal() {
                backup::create(&backup_path, self.config.get_backup_volumes())?
            } else {
                backup::create_ext(&backup_path, self.config.get_backup_volumes())?
            });

        // TODO: this might not be a smart place to put things, everything in system-connections
        // will end up in /mnt/boot/system-connections
        trace!("nwmgr_files");
        let nwmgr_path = path_append(work_dir, SYSTEM_CONNECTIONS_DIR);

        if (!self.mig_info.nwmgr_files.is_empty()
            || !self.mig_info.wifis.is_empty() && !dir_exists(&nwmgr_path)?)
            && !dir_exists(&nwmgr_path)?
        {
            create_dir(&nwmgr_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to create directory '{}'", nwmgr_path.display()),
            ))?;
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

        if !self.mig_info.wifis.is_empty() {
            let mut index = 0;
            for wifi in &self.mig_info.wifis {
                index = wifi.create_nwmgr_file(&nwmgr_path, index)?;
            }
        }

        let (mem_tot, mem_avail) = get_mem_info()?;
        info!(
            "Memory available is {} of {}",
            format_size_with_unit(mem_avail),
            format_size_with_unit(mem_tot),
        );

        let mut required_size: u64 = self.mig_info.get_os_image().get_required_space();

        required_size += self.mig_info.config_file.get_size();

        if has_backup {
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

        trace!("device setup");

        // We need this before s2 config as it might still modify migrate_info
        // TODO: make setup take no s2_cfg or immutable s2_cfg and return boot_backup instead
        // TODO: make setup undoable in case something bad happens later on

        self.device
            .setup(&mut self.mig_info, &self.config, &mut self.stage2_config)?;

        trace!("stage2 config");

        // dbg!("setting up stage2_cfg");
        // *****************************************************************************************
        // Finish Stage2ConfigBuilder & create stage2 config file

        if let Some(device) = self.config.get_force_flash_device() {
            warn!("Forcing flash device to '{}'", device.display());
            self.stage2_config
                .set_force_flash_device(device.to_path_buf());
        }

        self.stage2_config.set_failmode(self.config.get_fail_mode());

        self.stage2_config.set_no_flash(self.config.is_no_flash());

        self.stage2_config
            .set_migrate_delay(self.config.get_delay());

        if let Some(hacks) = self.config.get_hacks() {
            self.stage2_config.set_hacks(hacks)
        }

        self.stage2_config
            .set_balena_image(self.mig_info.get_os_image());

        self.stage2_config
            .set_balena_config(self.mig_info.config_file.get_rel_path().clone());

        // TODO: setpath if on / mount else set mount

        self.stage2_config
            .set_gzip_internal(self.config.is_gzip_internal());

        self.stage2_config
            .set_log_level(String::from(self.config.get_log_level()));

        if let Some(ref log_path) = self.mig_info.log_path {
            if log_path.device != boot_device.device_info.device {
                info!(
                    "Set up log device as '{}' with fs type: {}",
                    log_path.get_alt_path().display(),
                    log_path.fs_type
                );

                self.stage2_config.set_log_to(LogDevice {
                    device: log_path.get_alt_path().clone(),
                    fs_type: log_path.fs_type.clone(),
                });
            } else {
                warn!("Log partition '{}' is not on a distinct drive from flash drive: '{}' - ignoring", log_path.device, boot_device.device_info.drive);
            }
        }

        trace!("write stage 2 config");
        let s2_path = path_append(&boot_device.mountpoint, STAGE2_CFG_FILE);
        self.stage2_config.write_stage2_cfg_to(&s2_path)?;

        if let Some(delay) = self.config.get_reboot() {
            println!(
                "Migration stage 1 was successfull, rebooting system in {} seconds",
                *delay
            );
            Logger::flush();
            sync();
            let delay = Duration::new(*delay, 0);
            thread::sleep(delay);
            println!("Rebooting now..");
            call(REBOOT_CMD, &["-f"], false)?;
        } else {
            println!(
                "Migration stage 1 was successful, please reboot system to finalize migration"
            );
        }

        trace!("done");
        Ok(())
    }
}
