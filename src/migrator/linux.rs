use failure::{Fail, ResultExt};
use log::{debug, error, info, trace, warn};
use std::fs::{copy, create_dir};
use std::thread;
use std::time::Duration;

// TODO: Require files to be in work_dir: balena-image, balena-config, system-connections

use crate::{
    common::{
        backup,
        config::balena_config::{FSDump, ImageType},
        dir_exists, format_size_with_unit, path_append,
        stage2_config::{PathType, Stage2ConfigBuilder, Stage2LogConfig},
        Config, MigErrCtx, MigError, MigErrorKind, MigMode,
    },
    defs::{BACKUP_FILE, MIN_DISK_SIZE, SYSTEM_CONNECTIONS_DIR},
};

pub(crate) mod linux_defs;

pub(crate) mod device;
pub(crate) use device::Device;

pub(crate) mod boot_manager;

mod extract;
use extract::Extractor;

pub(crate) mod stage2;

pub(crate) mod ensured_cmds;
pub(crate) use ensured_cmds::{
    EnsuredCmds, CHMOD_CMD, DF_CMD, FDISK_CMD, FILE_CMD, GRUB_REBOOT_CMD, GRUB_UPDT_CMD, LSBLK_CMD,
    MKTEMP_CMD, MOKUTIL_CMD, MOUNT_CMD, PARTED_CMD, REBOOT_CMD, UNAME_CMD,
};

pub(crate) mod migrate_info;
pub(crate) use migrate_info::MigrateInfo;

pub(crate) mod linux_common;
use crate::common::stage2_config::MountConfig;
use crate::defs::STAGE2_CFG_FILE;
pub(crate) use linux_common::is_admin;
use mod_logger::{LogDestination, Logger};

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
        let config = Config::new()?;

        match config.migrate.get_mig_mode() {
            MigMode::EXTRACT => {
                let mut extractor = Extractor::new(config)?;
                extractor.extract(None)?;
                Ok(())
            }
            _ => {
                let mut migrator = LinuxMigrator::try_init(config)?;
                let res = match migrator.config.migrate.get_mig_mode() {
                    MigMode::IMMEDIATE => migrator.do_migrate(),
                    MigMode::PRETEND => Ok(()),
                    MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
                    MigMode::EXTRACT => panic!("impossible MigMode here"),
                };
                Logger::flush();
                res
            }
        }
    }

    // **********************************************************************
    // ** Initialise migrator
    // **********************************************************************

    pub fn try_init(config: Config) -> Result<LinuxMigrator, MigError> {
        trace!("LinuxMigrator::try_init: entered");

        info!("migrate mode: {:?}", config.migrate.get_mig_mode());

        let mut cmds = EnsuredCmds::new();

        if let Err(why) = cmds.ensure_cmds(REQUIRED_CMDS) {
            error!("Failed to ensure required commands: {:?}", why);
            return Err(MigError::displayed());
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
            Ok(_dummy) => info!(
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

        let boot_info = device.get_boot_device();
        let flash_device = &boot_info.drive;
        let flash_dev_size = boot_info.drive_size;

        info!(
            "The install drive is {}, size: {}",
            boot_info.drive.display(),
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

        /*
        if let Some(flash_device) = config.migrate.get_flash_device() {
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
        */

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
        let log_file = path_append(work_dir, "stage1.log");

        Logger::set_log_file(&LogDestination::Stderr, &log_file, true).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to set logging to '{}'", log_file.display()),
            ),
        )?;

        let boot_device = self.device.get_boot_device();

        if &self.mig_info.work_path.device == &boot_device.device {
            self.stage2_config
                .set_work_path(&PathType::Path(self.mig_info.work_path.path.clone()));
        } else {
            let (_lsblk_device, lsblk_part) = self.mig_info.lsblk_info.get_path_info(&work_dir)?;
            self.stage2_config
                .set_work_path(&PathType::Mount(MountConfig::new(
                    &lsblk_part.get_path(),
                    lsblk_part.fstype.as_ref().unwrap(),
                    work_dir
                        .strip_prefix(lsblk_part.mountpoint.as_ref().unwrap())
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            "failed to create relative work path",
                        ))?,
                )));
        }

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
            if !dir_exists(&nwmgr_path)? {
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

        // dbg!("setting up stage2_cfg");
        // *****************************************************************************************
        // Finish Stage2ConfigBuilder & create stage2 config file

        self.stage2_config
            .set_failmode(self.config.migrate.get_fail_mode());

        self.stage2_config
            .set_no_flash(self.config.debug.is_no_flash());

        /*
                self.stage2_config
                    .set_skip_flash(self.config.debug.is_skip_flash());
        */

        self.stage2_config
            .set_balena_image(self.mig_info.image_file.clone());

        self.stage2_config
            .set_balena_config(self.mig_info.config_file.get_rel_path().unwrap().clone());

        // TODO: setpath if on / mount else set mount

        self.stage2_config
            .set_gzip_internal(self.config.migrate.is_gzip_internal());

        self.stage2_config
            .set_log_level(String::from(self.config.migrate.get_log_level()));

        if let Some((ref log_path, ref log_drive, ref log_part)) = self.mig_info.log_path {
            if log_drive.get_path() != boot_device.drive {
                if let Some(ref fstype) = log_part.fstype {
                    self.stage2_config.set_log_to(Stage2LogConfig {
                        device: log_path.clone(),
                        fstype: fstype.clone(),
                    });

                    info!(
                        "Set up log device as '{}' with file system type '{}'",
                        log_path.display(),
                        fstype
                    );
                } else {
                    warn!(
                        "Could not determine file system type for log partition '{}'  - ignoring",
                        log_path.display()
                    );
                }
            } else {
                warn!("Log partition '{}' is not on a distinct drive from flash drive: '{}' - ignoring", log_path.display(), boot_device.drive.display());
            }
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
        let s2_path = path_append(&boot_device.mountpoint, STAGE2_CFG_FILE);
        self.stage2_config.write_stage2_cfg_to(&s2_path)?;

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
