use failure::ResultExt;
use log::{debug, error, info, trace, warn, Level};
use mod_logger::{LogDestination, Logger, NO_STREAM};
use nix::unistd::sync;

use std::fs::{copy, create_dir, read_dir, read_to_string};

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    common::{
        call, dir_exists,
        file_digest::check_digest,
        file_exists,
        file_info::RelFileInfo,
        file_size, format_size_with_unit, path_append,
        stage2_config::{CheckedImageType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{FailMode, BACKUP_FILE, SYSTEM_CONNECTIONS_DIR, VERSION},
    linux::{
        device_impl,
        linux_common::{get_mem_info, whereis},
        linux_defs::{KERNEL_OSRELEASE_PATH, REBOOT_CMD},
        linux_defs::{MIGRATE_LOG_FILE, STAGE2_MEM_THRESHOLD},
    },
};

// for starters just restore old boot config, only required command is mount

// later ensure all other required commands

mod stage2_defs;
pub(crate) use stage2_defs::*;

mod fs_writer;

mod flasher;

pub(crate) mod mounts;
use mounts::Mounts;

use std::cell::RefCell;

const REBOOT_DELAY: u64 = 3;
const S2_REV: u32 = 5;

// TODO: set this to Info once mature
const INIT_LOG_LEVEL: Level = Level::Trace;
const DEBUG_CONSOLE_DELAY: u64 = 5;
const MIGRATE_TEMP_DIR: &str = "/migrate_tmp";

// TODO: replace removed command checks ?
//const MIG_REQUIRED_CMDS: &[&str] = &[REBOOT_CMD, UDEVADM_CMD, FAT_CHK_CMD];

const BALENA_IMAGE_FILE: &str = "balenaOS.img.gz";
const BALENA_CONFIG_FILE: &str = "config.json";

const BALENA_BOOT_FS_FILE: &str = "resin-boot.tgz";
const BALENA_ROOTA_FS_FILE: &str = "resin-rootA.tgz";
const BALENA_ROOTB_FS_FILE: &str = "resin-rootB.tgz";
const BALENA_STATE_FS_FILE: &str = "resin-state.tgz";
const BALENA_DATA_FS_FILE: &str = "resin-data.tgz";

const LOG_STDERR: bool = true; // mute / unmute the start until config is read

pub(crate) enum FlashResult {
    Ok,
    FailRecoverable,
    FailNonRecoverable,
}

pub(crate) struct Stage2 {
    pub mounts: RefCell<Mounts>,
    config: Stage2Config,
    pub recoverable_state: bool,
}

impl<'a> Stage2 {
    // try to mount former root device and /boot if it is on a separate partition and
    // load the stage2 config

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    pub fn try_init() -> Result<Stage2, MigError> {
        Logger::set_default_level(&INIT_LOG_LEVEL);

        // make not logging to console at all configurable
        let log_dest = if LOG_STDERR {
            LogDestination::BufferStderr
        } else {
            LogDestination::Buffer
        };

        match Logger::set_log_dest(&log_dest, NO_STREAM) {
            Ok(_s) => {
                info!(
                    "Balena Migrate Stage 2 version {} rev {} initializing",
                    VERSION, S2_REV
                );
            }
            Err(_why) => {
                println!("failed to initalize logger");
                println!("Balena Migrate Stage 2 rev {} initializing", S2_REV);
            }
        }

        if let Ok(krelease) = read_to_string(KERNEL_OSRELEASE_PATH) {
            info!("Running stage2 on kernel version: '{}'", krelease.trim());
        } else {
            warn!("Failed to retrieve kernel release");
        }

        // TODO: create replacement for ensured commands

        // mount boot device containing BALENA_STAGE2_CFG for starters
        let mut mounts = match Mounts::new() {
            Ok(mounts) => {
                debug!(
                    "Successfully mounted boot file system: '{}' on '{:?}'",
                    mounts.get_flash_device().display(),
                    mounts.get_balena_boot_mountpoint()
                );
                mounts
            }
            Err(why) => {
                error!("Failed to mount boot file system, giving up: {:?}", why);
                thread::sleep(Duration::new(2 * DEBUG_CONSOLE_DELAY, 0));
                return Err(MigError::displayed());
            }
        };

        debug!("got mounts: {:?}", mounts);

        let stage2_cfg_file = mounts.get_stage2_config();
        let stage2_cfg = match Stage2Config::from_config(&stage2_cfg_file) {
            Ok(s2_cfg) => {
                info!(
                    "Successfully read stage 2 config file from {}",
                    stage2_cfg_file.display()
                );
                s2_cfg
            }
            Err(why) => {
                error!(
                    "Failed to read stage 2 config file from file '{}' with error: {:?}",
                    stage2_cfg_file.display(),
                    why
                );
                thread::sleep(Duration::new(2 * DEBUG_CONSOLE_DELAY, 0));
                // TODO: could try to restore former boot config anyway
                return Err(MigError::displayed());
            }
        };

        if let Some(device) = stage2_cfg.get_force_flash_device() {
            if device != mounts.get_flash_device() {
                warn!("Forcibly setting flash device to '{}'", device.display());
                mounts.set_force_flash_device(device);
            }
        }

        info!("Setting log level to {:?}", stage2_cfg.get_log_level());
        Logger::set_default_level(&stage2_cfg.get_log_level());

        // Mount all remaining drives - work and log
        match mounts.mount_from_config(&stage2_cfg) {
            Ok(_) => {
                info!("mounted all configured drives");
            }
            Err(why) => {
                warn!("mount_all returned an error: {:?}", why);
            }
        }

        //thread::sleep(Duration::new(10, 0));

        // try switch logging to a persistent log
        let log_path = if let Some(log_path) = mounts.get_log_path() {
            Some(path_append(log_path, MIGRATE_LOG_FILE))
        } else if stage2_cfg.is_no_flash() || mounts.is_work_no_copy() {
            if let Some(work_path) = mounts.get_work_path() {
                Some(path_append(work_path, MIGRATE_LOG_FILE))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ref log_path) = log_path {
            match Logger::set_log_file(&LogDestination::StreamStderr, &log_path, false) {
                Ok(_) => {
                    info!("Set log file to '{}'", log_path.display());
                }
                Err(why) => {
                    warn!(
                        "Failed to set log file to '{}', error: {:?}",
                        log_path.display(),
                        why
                    );
                }
            }
        } else {
            // stop logging to memory buffer
            let _res = Logger::set_log_dest(&LogDestination::BufferStderr, NO_STREAM);
        }

        Ok(Stage2 {
            mounts: RefCell::new(mounts),
            config: stage2_cfg,
            recoverable_state: false,
        })
    }

    // Do the actual work once drives are mounted and config is read

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    pub fn migrate(&mut self) -> Result<(), MigError> {
        debug!("migrate: entered");

        let device_type = *self.config.get_device_type();
        let boot_type = *self.config.get_boot_type();

        // TODO: this will not work for grub, boot once
        let migrate_delay = self.config.get_migrate_delay();
        if migrate_delay > 0 {
            let start_time = Instant::now();
            let max_wait = Duration::from_secs(migrate_delay);
            info!("Taking a break for {} seconds", migrate_delay);

            let mut elapsed = start_time.elapsed();
            while elapsed < max_wait {
                thread::sleep(Duration::from_secs(1));
                elapsed = start_time.elapsed();
                debug!("still sleeping, time elapsed: {}", elapsed.as_secs());
            }

            info!("Done waiting, continuing now");
        }

        thread::sleep(Duration::from_secs(DEBUG_CONSOLE_DELAY));

        let device = device_impl::from_config(device_type, boot_type)?;

        if device.restore_boot(&self.mounts.borrow(), &self.config) {
            info!("Boot configuration was restored sucessfully");
            // boot config restored can reboot
            self.recoverable_state = true;
        } else {
            warn!("Failed to restore boot configuration - trying to migrate anyway.",);
        }

        sync();
        // TODO: debug, remove this
        thread::sleep(Duration::from_secs(DEBUG_CONSOLE_DELAY));

        info!("migrating {:?} boot type: {:?}", device_type, &boot_type);

        let work_path = if let Some(work_path) = self.mounts.borrow().get_work_path() {
            work_path.to_path_buf()
        } else {
            error!("The working directory was not mounted - aborting migration");
            return Err(MigError::displayed());
        };

        let mig_tmp_dir = if !self.mounts.borrow().is_work_no_copy() {
            // check if we have enough space to copy files to initramfs
            let mig_tmp_dir = match get_mem_info() {
                Ok((mem_tot, mem_avail)) => {
                    info!(
                        "Memory available is {} of {}",
                        format_size_with_unit(mem_avail),
                        format_size_with_unit(mem_tot)
                    );

                    let mut required_size = self.config.get_balena_image().get_required_space();

                    required_size +=
                        file_size(path_append(&work_path, &self.config.get_balena_config()))?;

                    if self.config.has_backup() {
                        required_size += file_size(path_append(&work_path, BACKUP_FILE))?;
                    }

                    let src_nwmgr_dir = path_append(&work_path, SYSTEM_CONNECTIONS_DIR);
                    if dir_exists(&src_nwmgr_dir)? {
                        let paths = read_dir(&src_nwmgr_dir).context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("Failed to list directory '{}'", src_nwmgr_dir.display()),
                        ))?;

                        for path in paths {
                            if let Ok(path) = path {
                                required_size += file_size(path.path())?;
                            }
                        }
                    }

                    info!(
                        "Memory required for copying files is {}",
                        format_size_with_unit(required_size)
                    );

                    if mem_avail > required_size + STAGE2_MEM_THRESHOLD {
                        Path::new(MIGRATE_TEMP_DIR)
                    } else {
                        // TODO: create temporary storage on disk instead by resizing
                        error!("Not enough memory available for copying files");
                        return Err(MigError::from_remark(
                            MigErrorKind::InvState,
                            "Not enough memory available for copying files",
                        ));
                    }
                }
                Err(why) => {
                    warn!("Failed to retrieve mem info, error: {:?}", why);
                    return Err(MigError::displayed());
                }
            };

            if !dir_exists(mig_tmp_dir)? {
                create_dir(mig_tmp_dir).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "failed to create migrate temp directory {}",
                        MIGRATE_TEMP_DIR
                    ),
                ))?;
            }

            match self.config.get_balena_image() {
                CheckedImageType::Flasher(ref image_file) => {
                    let src = path_append(&work_path, &image_file.rel_path);
                    let tgt = path_append(mig_tmp_dir, BALENA_IMAGE_FILE);
                    copy(&src, &tgt).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "failed to copy balena image to migrate temp directory, '{}' -> '{}'",
                            src.display(),
                            tgt.display()
                        ),
                    ))?;
                    info!("Checking digest on copied file '{}'", tgt.display());
                    if !check_digest(&tgt, &image_file.hash_info)? {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                                "Failed to check digest on copied file: '{}', {:?} ",
                                tgt.display(),
                                image_file.hash_info
                            ),
                        ));
                    }

                    info!("copied balena OS image to '{}'", tgt.display());
                    // check digest
                }
                CheckedImageType::FileSystems(ref fs_dump) => {
                    self.copy_and_check(
                        &work_path,
                        &fs_dump.boot.archive,
                        mig_tmp_dir,
                        "boot",
                        BALENA_BOOT_FS_FILE,
                    )?;
                    self.copy_and_check(
                        &work_path,
                        &fs_dump.root_a.archive,
                        mig_tmp_dir,
                        "rootA",
                        BALENA_ROOTA_FS_FILE,
                    )?;
                    self.copy_and_check(
                        &work_path,
                        &fs_dump.root_b.archive,
                        mig_tmp_dir,
                        "rootB",
                        BALENA_ROOTB_FS_FILE,
                    )?;
                    self.copy_and_check(
                        &work_path,
                        &fs_dump.state.archive,
                        mig_tmp_dir,
                        "state",
                        BALENA_STATE_FS_FILE,
                    )?;
                    self.copy_and_check(
                        &work_path,
                        &fs_dump.data.archive,
                        mig_tmp_dir,
                        "data",
                        BALENA_DATA_FS_FILE,
                    )?;
                }
            };

            let src = path_append(&work_path, self.config.get_balena_config());
            let tgt = path_append(mig_tmp_dir, BALENA_CONFIG_FILE);
            copy(&src, &tgt).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to copy balena config to migrate temp directory, '{}' -> '{}'",
                    src.display(),
                    tgt.display()
                ),
            ))?;

            info!("copied balena OS config to '{}'", tgt.display());

            let src_nwmgr_dir = path_append(&work_path, SYSTEM_CONNECTIONS_DIR);

            let tgt_nwmgr_dir = path_append(mig_tmp_dir, SYSTEM_CONNECTIONS_DIR);
            if dir_exists(&src_nwmgr_dir)? {
                if !dir_exists(&tgt_nwmgr_dir)? {
                    create_dir(&tgt_nwmgr_dir).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "failed to create systm-connections in migrate temp directory: '{}'",
                            tgt_nwmgr_dir.display()
                        ),
                    ))?;
                }

                let paths = read_dir(&src_nwmgr_dir).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to list directory '{}'", src_nwmgr_dir.display()),
                ))?;

                for path in paths {
                    if let Ok(path) = path {
                        let src_path = path.path();
                        if src_path.metadata().unwrap().is_file() {
                            let tgt_path =
                                path_append(&tgt_nwmgr_dir, &src_path.file_name().unwrap());
                            copy(&src_path, &tgt_path)
                                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed copy network manager file to migrate temp directory '{}' -> '{}'", src_path.display(), tgt_path.display())))?;
                            info!("copied network manager config  to '{}'", tgt_path.display());
                        }
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Error reading entry from directory '{}'",
                                src_nwmgr_dir.display()
                            ),
                        ));
                    }
                }
            }

            if self.config.has_backup() {
                // TODO: check available memory / disk space
                let target_path = path_append(mig_tmp_dir, BACKUP_FILE);
                let source_path = path_append(&work_path, BACKUP_FILE);

                copy(&source_path, &target_path).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed copy backup file to migrate temp directory '{}' -> '{}'",
                        source_path.display(),
                        target_path.display()
                    ),
                ))?;
                info!("copied backup  to '{}'", target_path.display());
            }

            info!("Files copied to RAMFS");
            mig_tmp_dir
        } else {
            info!("Files were not copied, work dir is on a separate drive");
            // TODO: adapt path for no copy mode
            // TODO: check digest anyway ?
            &work_path
        };

        // Write our buffered log to workdir before unmounting if we are not flashing anyway

        if self.config.is_no_flash() {
            // TODO: check recoverable flag, but what to ?
            info!("Not flashing due to config parameter no_flash");
            Logger::flush();
            sync();
            let _res = Logger::set_log_dest(&LogDestination::Stderr, NO_STREAM);
        }

        // TODO: check this
        self.mounts.borrow_mut().unmount_boot_devs()?;

        info!("Unmounted file systems");

        // ************************************************************************************
        // * write the gzipped image to disk
        // * from migrate:
        // * gzip -d -c "${MIGRATE_TMP}/${IMAGE_FILE}" | dd of=${BOOT_DEV} bs=4194304 || fail  "failed with gzip -d -c ${MIGRATE_TMP}/${IMAGE_FILE} | dd of=${BOOT_DEV} bs=4194304"

        let target_path = self.mounts.borrow().get_flash_device().to_path_buf();

        if !file_exists(&target_path) {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "Could not locate target device: '{}'",
                    target_path.display()
                ),
            ));
        }

        if self.config.is_no_flash() {
            Stage2::exit(&FailMode::Reboot)?;
        }

        // TODO: debug fs_writer crash
        // Exit in Rescue Shell  Mode to call external script
        // Call external script

        match self.config.get_balena_image() {
            CheckedImageType::Flasher(ref image_file) => {
                // TODO: move some, if not most of this into flasher

                let image_path = if self.mounts.borrow().is_work_no_copy() {
                    if let Some(work_dir) = self.mounts.borrow().get_work_path() {
                        path_append(work_dir, &image_file.rel_path)
                    } else {
                        warn!("Work path not found in no_copy mode, trying mig temp");
                        path_append(mig_tmp_dir, BALENA_IMAGE_FILE)
                    }
                } else {
                    path_append(mig_tmp_dir, BALENA_IMAGE_FILE)
                };

                info!(
                    "attempting to flash '{}' to '{}'",
                    image_path.display(),
                    target_path.display()
                );

                if !file_exists(&image_path) {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!("Could not locate OS image: '{}'", image_path.display()),
                    ));
                }

                match flasher::flash_balena_os(
                    &target_path,
                    &mut self.mounts.borrow_mut(),
                    &self.config,
                    &image_path,
                ) {
                    FlashResult::Ok => {}
                    FlashResult::FailRecoverable => {
                        error!("Failed to flash balena OS image");
                        // Logger::flush();
                        self.recoverable_state = true;
                        return Err(MigError::displayed());
                    }
                    FlashResult::FailNonRecoverable => {
                        error!("Failed to flash balena OS image");
                        // Logger::flush();
                        self.recoverable_state = false;
                        return Err(MigError::displayed());
                    }
                }

                // Logger::flush();
            }
            CheckedImageType::FileSystems(ref _fs_dump) => {
                let base_path = if self.mounts.borrow().is_work_no_copy() {
                    if let Some(work_dir) = self.mounts.borrow().get_work_path() {
                        work_dir.to_path_buf()
                    } else {
                        warn!("Work path not found in no_copy mode, trying mig temp");
                        mig_tmp_dir.to_path_buf()
                    }
                } else {
                    mig_tmp_dir.to_path_buf()
                };

                match fs_writer::write_balena_os(
                    &target_path,
                    &mut self.mounts.borrow_mut(),
                    &self.config,
                    &base_path,
                ) {
                    FlashResult::Ok => (),
                    FlashResult::FailNonRecoverable => {
                        self.recoverable_state = false;
                        error!("Failed to write balena os image");
                        return Err(MigError::displayed());
                    }
                    FlashResult::FailRecoverable => {
                        self.recoverable_state = true;
                        error!("Failed to write balena os image");
                        return Err(MigError::displayed());
                    }
                }
            }
        }

        info!("Mounting balena file systems");
        sync();

        // TODO: check fingerprints ?

        let boot_mountpoint =
            if let Some(mountpoint) = self.mounts.borrow().get_balena_boot_mountpoint() {
                mountpoint.to_path_buf()
            } else {
                self.recoverable_state = false;
                error!("Unable to retrieve balena boot mountpoint");
                return Err(MigError::displayed());
            };

        let src = path_append(mig_tmp_dir, BALENA_CONFIG_FILE);
        let tgt = path_append(&boot_mountpoint, BALENA_CONFIG_FILE);

        copy(&src, &tgt).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy balena config to boot mount dir, '{}' -> '{}'",
                src.display(),
                tgt.display()
            ),
        ))?;

        info!("copied balena OS config to '{}'", tgt.display());

        // copy system connections
        let nwmgr_dir = path_append(mig_tmp_dir, SYSTEM_CONNECTIONS_DIR);
        if dir_exists(&nwmgr_dir)? {
            let tgt_path = path_append(&boot_mountpoint, SYSTEM_CONNECTIONS_DIR);
            for path in read_dir(&nwmgr_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to read directory: '{}'", nwmgr_dir.display()),
            ))? {
                if let Ok(ref path) = path {
                    let tgt = path_append(&tgt_path, path.file_name());
                    copy(path.path(), &tgt).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to copy '{}' to '{}'",
                            path.path().display(),
                            tgt.display()
                        ),
                    ))?;
                    info!("copied '{}' to '{}'", path.path().display(), tgt.display());
                } else {
                    error!("failed to read path element: {:?}", path);
                }
            }
        } else {
            warn!("No network manager configurations were copied");
        }

        // we can hope to successfully reboot again after writing config.json and system-connections
        sync();
        self.recoverable_state = true;

        if let Some(data_mountpoint) = self.mounts.borrow().get_balena_data_mountpoint() {
            // TODO: copy log, backup to data_path
            if self.config.has_backup() {
                // TODO: check available disk space
                let source_path = path_append(&mig_tmp_dir, BACKUP_FILE);
                let target_path = path_append(&data_mountpoint, BACKUP_FILE);

                copy(&source_path, &target_path).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed copy backup file to data partition '{}' -> '{}'",
                        source_path.display(),
                        target_path.display()
                    ),
                ))?;
                info!("copied backup  to '{}'", target_path.display());
            }

            if Logger::get_log_dest().is_buffer_dest() {
                let log_path = path_append(&data_mountpoint, MIGRATE_LOG_FILE);
                match Logger::set_log_file(&LogDestination::Stderr, &log_path, false) {
                    Ok(_) => {
                        info!("Set log file to '{}'", log_path.display());
                        //Logger::flush();
                    }
                    Err(why) => {
                        warn!(
                            "Failed to set log file to '{}', error: {:?}",
                            log_path.display(),
                            why
                        );
                    }
                }
            }
        }

        let _res = self.mounts.borrow_mut().unmount_balena();

        info!(
            "Migration stage 2 was successful, rebooting in {} seconds!",
            REBOOT_DELAY
        );

        let _res = self.mounts.borrow_mut().unmount_log();

        thread::sleep(Duration::new(REBOOT_DELAY, 0));

        Logger::flush(); // superfluous
        sync();

        Stage2::exit(&FailMode::Reboot)?;

        Ok(())
    }

    fn exit(fail_mode: &FailMode) -> Result<(), MigError> {
        trace!("exit: entered with {:?}", fail_mode);

        Logger::flush();
        sync();

        thread::sleep(Duration::from_secs(1));

        match fail_mode {
            FailMode::Reboot => {
                let reboot_cmd = whereis(REBOOT_CMD)?;
                let cmd_res = call(&reboot_cmd, &["-f"], true)?;
                if !cmd_res.status.success() {
                    error!("Command failed: {}, : '{}'", REBOOT_CMD, cmd_res.stderr);
                    return Err(MigError::displayed());
                }
            }
            FailMode::RescueShell => {
                std::process::exit(1);
            }
        }
        Ok(())
    }

    /*
        pub(crate) fn is_recoverable(&self) -> bool {
            self.recoverable_state
        }
    */
    pub(crate) fn default_exit() -> Result<(), MigError> {
        trace!("default_exit: entered ");
        Stage2::exit(FailMode::get_default())
    }

    pub(crate) fn error_exit(&self) -> Result<(), MigError> {
        trace!("error_exit: entered");
        if self.recoverable_state {
            Stage2::exit(self.config.get_fail_mode())
        } else {
            Stage2::exit(&FailMode::RescueShell)
        }
    }

    fn copy_and_check(
        &self,
        source_dir: &Path,
        archive: &RelFileInfo,
        target_dir: &Path,
        tag: &str,
        target_name: &str,
    ) -> Result<(), MigError> {
        let src = path_append(&source_dir, &archive.rel_path);
        let tgt = path_append(target_dir, target_name);
        copy(&src, &tgt).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy balena fs archive to migrate temp directory, '{}' -> '{}'",
                src.display(),
                tgt.display()
            ),
        ))?;

        info!(
            "copied balena {} archive to '{}' -> '{}'",
            tag,
            src.display(),
            tgt.display()
        );

        info!(
            "Checking digest on copied file '{}' - {:?}",
            tgt.display(),
            archive.hash_info
        );
        if !check_digest(&tgt, &archive.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Digest mismatch on file '{}', {:?}",
                    archive.rel_path.display(),
                    archive.hash_info
                ),
            ));
        }
        Ok(())
    }
}
