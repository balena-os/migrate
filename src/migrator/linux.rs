use failure::{Fail, ResultExt};
use log::{debug, error, info, trace, warn};
use std::fs::{copy, create_dir};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

// TODO: Require files to be in work_dir

use crate::{
    common::{
        backup, dir_exists, format_size_with_unit, path_append, Config, MigErrCtx, MigError,
        MigErrorKind, MigMode,
        stage2_config::{PathType, MountConfig, Stage2ConfigBuilder, Stage2LogConfig, },
    },
    defs::{BACKUP_FILE, MIN_DISK_SIZE, SYSTEM_CONNECTIONS_DIR},
};

pub(crate) mod linux_defs;
use linux_defs::{BOOT_PATH};

pub(crate) mod device;
pub(crate) use device::Device;

pub(crate) mod boot_manager;

pub(crate) mod stage2;

pub(crate) mod ensured_cmds;
pub(crate) use ensured_cmds::{
    EnsuredCmds, CHMOD_CMD, DD_CMD, DF_CMD, FDISK_CMD, FILE_CMD, GRUB_REBOOT_CMD, GRUB_UPDT_CMD,
    GZIP_CMD, LSBLK_CMD, MKTEMP_CMD, MOKUTIL_CMD, MOUNT_CMD, PARTED_CMD, PARTPROBE_CMD, REBOOT_CMD,
    UNAME_CMD,
};

pub(crate) mod migrate_info;
pub(crate) use migrate_info::MigrateInfo;

pub(crate) mod linux_common;
pub(crate) use linux_common::is_admin;


const REQUIRED_CMDS: &'static [&'static str] = &[
    DF_CMD, LSBLK_CMD, FILE_CMD, UNAME_CMD, MOUNT_CMD, REBOOT_CMD, CHMOD_CMD, MKTEMP_CMD,
];

pub(crate) struct LinuxMigrator {
    cmds: EnsuredCmds,
    mig_info: MigrateInfo,
    config: Config,
    stage2_config: Stage2ConfigBuilder,
    device: Box<Device>,
}

impl<'a> LinuxMigrator {
    pub fn migrate() -> Result<(), MigError> {
        let mut migrator = LinuxMigrator::try_init(Config::new()?)?;
        match migrator.config.migrate.get_mig_mode() {
            MigMode::IMMEDIATE => migrator.do_migrate(),
            MigMode::PRETEND => Ok(()),
            MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
        }
    }

    // **********************************************************************
    // ** Initialise migrator
    // **********************************************************************

    pub fn try_init(config: Config) -> Result<LinuxMigrator, MigError> {
        trace!("LinuxMigrator::try_init: entered");

        info!("migrate mode: {:?}", config.migrate.get_mig_mode());

        let mut cmds = match EnsuredCmds::new(REQUIRED_CMDS) {
            Ok(cmds) => cmds,
            Err(why) => {
                let message = format!("Failed to ensure required commands: {:?}", why);
                error!("{}", message);
                return Err(MigError::from(
                    why.context(MigErrCtx::from_remark(MigErrorKind::Upstream, &message)),
                ));
            }
        };

        // **********************************************************************
        // We need to be root to do this
        // note: fake admin is not honored in release mode

        if !is_admin(&config)? {
            error!("please run this program as root");
            return Err(MigError::from(MigErrorKind::Displayed));
        }

        // **********************************************************************
        // Ensure some more vital commands
        let parted_found = match cmds.ensure(PARTED_CMD) {
            Ok(_s) => true,
            Err(_why) => false,
        };

        let fdisk_found = match cmds.ensure(FDISK_CMD) {
            Ok(_s) => true,
            Err(_why) => false,
        };

        if !(fdisk_found || parted_found) {
            let message = format!(
                "Missing partitioning commands, please make sure either {} or {} is available",
                PARTED_CMD, FDISK_CMD
            );
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }

        // **********************************************************************
        // Get os architecture & name & disk properties, check required paths
        // find wifis etc..

        let mig_info = match MigrateInfo::new(&config, &mut cmds) {
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
        // Add further architectures / functons in device.rs

        let mut stage2_config = Stage2ConfigBuilder::default();

        let device = match device::get_device(&mut cmds, &mig_info, &config, &mut stage2_config) {
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
            Ok(_bla) => info!(
                "The sanity check on '{}' passed",
                mig_info.config_file.get_path().display()
            ),
            Err(why) => {
                let message = format!(
                    "The sanity check on '{}' failed: {:?}",
                    mig_info.config_file.get_path().display(),
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

        let flash_device = &mig_info.root_path.drive;
        let flash_dev_size = mig_info.root_path.drive_size;

        info!(
            "The install drive is {}, size: {}",
            flash_device.display(),
            format_size_with_unit(flash_dev_size)
        );

        // **********************************************************************
        // Require a minimum disk device size for installation

        if flash_dev_size < MIN_DISK_SIZE {
            error!(
                "The size of the install drive '{}' = {} is too small to install balenaOS",
                flash_device.display(),
                format_size_with_unit(flash_dev_size)
            );
            return Err(MigError::from(MigErrorKind::Displayed));
        }

        if let Some(force_flash_device) = config.debug.get_force_flash_device() {
            // force annother device to be flashed, strictly debug !!!
            // forced flash device currently  goes unchecked for existence and size
            warn!(
                "Overriding chosen flash device with: '{}'",
                force_flash_device.display()
            );
            stage2_config.set_flash_device(&PathBuf::from(force_flash_device));
        } else {
            stage2_config.set_flash_device(flash_device);
        }

        if mig_info.boot_path.device != mig_info.root_path.device {
            // /boot on separate partition
            info!(
                "Found boot device '{}', fs type: {}, free space: {}",
                mig_info.boot_path.device.display(),
                mig_info.boot_path.fs_type,
                format_size_with_unit(mig_info.boot_path.fs_free)
            );
        }

        Ok(LinuxMigrator {
            cmds,
            mig_info,
            config,
            device,
            stage2_config,
        })
    }

    // **********************************************************************
    // ** Start the actual migration
    // **********************************************************************

    fn do_migrate(&mut self) -> Result<(), MigError> {
        // TODO: prepare logging

        let work_dir = &self.mig_info.work_path.path;
        let backup_path = path_append(work_dir, BACKUP_FILE);

        self.stage2_config.set_has_backup(backup::create(
            &backup_path,
            self.config.migrate.get_backup_volumes(),
        )?);

        // TODO: compare total transfer size (kernel, initramfs, backup, configs )  to memory size (needs to fit in ramfs)

        // TODO: this might not be a smart place to put things, everything in system-connections
        // will end up in /mnt/boot/system-connections
        trace!("nwmgr_files");
        let nwmgr_path = path_append(work_dir, SYSTEM_CONNECTIONS_DIR);

        if self.mig_info.nwmgr_files.len() > 0
            || self.mig_info.wifis.len() > 0 && !dir_exists(&nwmgr_path)?
        {
            if ! dir_exists(&nwmgr_path)? {
                create_dir(&nwmgr_path).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to create directory '{}'", nwmgr_path.display()),
                ))?;
            }
        }

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

        trace!("do_migrate: found wifis: {}", self.mig_info.wifis.len());

        if self.mig_info.wifis.len() > 0 {
            let mut index = 0;
            for wifi in &self.mig_info.wifis {
                index = wifi.create_nwmgr_file(&nwmgr_path, index)?;
            }
        }

        trace!("stage2 config");

        // *****************************************************************************************
        // Finish Stage2ConfigBuilder & create stage2 config file

        self.stage2_config
            .set_failmode(self.config.migrate.get_fail_mode());

        self.stage2_config
            .set_no_flash(self.config.debug.is_no_flash());

        self.stage2_config
            .set_skip_flash(self.config.debug.is_skip_flash());


        if self.mig_info.boot_path.device != self.mig_info.root_path.device {
            self.stage2_config
                .set_boot_mount(&MountConfig::new(
                    self.mig_info.boot_path.device,
                    self.mig_info.boot_path.fs_type,
                    PathBuf::from(BOOT_PATH),
                ));
        }
        // later
        self.stage2_config
            .set_balena_image(PathBuf::from(&self.mig_info.image_file.path));
        self.stage2_config
            .set_balena_config(PathBuf::from(self.mig_info.config_file.get_path()));

        // TODO: setpath if on / mount else set mount

        self.stage2_config
            .set_work_path(&PathType::Path(self.mig_info.work_path.path));

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        self.stage2_config
            .set_log_level(String::from(self.config.migrate.get_log_level()));

        if let Some((ref device, ref fstype)) = self.mig_info.log_path {
            self.stage2_config.set_log_to(Stage2LogConfig {
                device: device.clone(),
                fstype: fstype.clone(),
            });
        }

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        trace!("device setup");

        self.device.setup(
            &self.cmds,
            &mut self.mig_info,
            &self.config,
            &mut self.stage2_config,
        )?;

        trace!("write stage 2 config");
        self.stage2_config.write_stage2_cfg()?;

        if let Some(delay) = self.config.migrate.get_reboot() {
            println!(
                "Migration stage 1 was successfull, rebooting system in {} seconds",
                *delay
            );
            let delay = Duration::new(*delay, 0);
            thread::sleep(delay);
            println!("Rebooting now..");
            self.cmds.call(REBOOT_CMD, &["-f"], false)?;
        }

        trace!("done");
        Ok(())
    }
}
