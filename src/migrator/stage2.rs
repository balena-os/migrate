use failure::ResultExt;
use flate2::read::GzDecoder;
use log::{debug, error, info, trace, warn, Level};
use mod_logger::{LogDestination, Logger, NO_STREAM};
use nix::{
    mount::{mount, umount, MsFlags},
    sys::reboot::{reboot, RebootMode},
    unistd::sync,
};

use std::fs::{copy, create_dir, read_dir, read_link, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    common::{
        dir_exists, file_exists, file_size, format_size_with_unit, path_append, FailMode, MigErrCtx, MigError,
        MigErrorKind,
    },
    defs::{
        BACKUP_FILE, BALENA_BOOT_FSTYPE, BALENA_BOOT_PART, BALENA_DATA_FSTYPE, BALENA_DATA_PART,
        BALENA_ROOTA_PART, BALENA_ROOTB_PART, BALENA_STATE_PART, BOOT_PATH, DISK_BY_LABEL_PATH,
        STAGE2_CFG_FILE, SYSTEM_CONNECTIONS_DIR,MIG_KERNEL_NAME, MIG_INITRD_NAME, STAGE2_MEM_THRESHOLD
    },
    device,
    linux_common::{
        call_cmd, ensure_cmds, get_cmd, get_root_info, get_mem_info, DD_CMD, GZIP_CMD, PARTPROBE_CMD, REBOOT_CMD,
    },
};

pub(crate) mod stage2_config;
pub(crate) use stage2_config::Stage2Config;


// for starters just restore old boot config, only required command is mount

// later ensure all other required commands

const REBOOT_DELAY: u64 = 3;

const INIT_LOG_LEVEL: Level = Level::Info;
const ROOTFS_DIR: &str = "/tmp_root";
const LOG_MOUNT_DIR: &str = "/migrate_log";
const LOG_FILE_NAME: &str = "migrate.log";

const MIGRATE_TEMP_DIR: &str = "/migrate_tmp";
const BOOT_MNT_DIR: &str = "mnt_boot";
const DATA_MNT_DIR: &str = "mnt_data";

const DD_BLOCK_SIZE: usize = 4194304;

const MIG_REQUIRED_CMDS: &'static [&'static str] = &[DD_CMD, PARTPROBE_CMD, GZIP_CMD, REBOOT_CMD];
const MIG_OPTIONAL_CMDS: &'static [&'static str] = &[];

const BALENA_IMAGE_FILE: &str = "balenaOS.img.gz";
const BALENA_CONFIG_FILE: &str = "config.json";

const NIX_NONE: Option<&'static [u8]> = None;
const PRE_PARTPROBE_WAIT_SECS: u64 = 5;
const POST_PARTPROBE_WAIT_SECS: u64 = 5;
const PARTPROBE_WAIT_NANOS: u32 = 0;

pub(crate) struct Stage2 {
    config: Stage2Config,
    boot_mounted: bool,
    recoverable_state: bool,
    root_fs_path: PathBuf,
}

impl Stage2 {

    // try to mount former root device and /boot if it is on a separate partition and
    // load the stage2 config

    pub fn try_init() -> Result<Stage2, MigError> {
        // TODO: wait a couple of seconds for more devices to show up ?

        match Logger::initialise_v2(Some(&INIT_LOG_LEVEL), Some(&LogDestination::BufferStderr), NO_STREAM) {
            Ok(_s) =>  {
                info!("Balena Migrate Stage 2 initializing");
            },
            Err(_why) => {
                println!("Balena Migrate Stage 2 initializing");
                println!("failed to initalize logger");
            }
        }

        let root_fs_dir = PathBuf::from(ROOTFS_DIR);

        // TODO: beaglebone version - make device_slug dependant

        let (root_device, root_fs_type) = get_root_info()?;

        info!(
            "Using root device '{}' with fs-type: '{:?}'",
            root_device.display(),
            root_fs_type
        );

        if !dir_exists(&root_fs_dir)? {
            create_dir(&root_fs_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to create mountpoint for roofs in {}", &root_fs_dir.display()),
            ))?;
        } else {
            warn!("root mount directory {} exists", &root_fs_dir.display());
        }

        // TODO: add options to make this more reliable)

        mount(
            Some(&root_device),
            &root_fs_dir,
            if let Some(ref fs_type) = root_fs_type {
                Some(fs_type.as_bytes())
            } else {
                NIX_NONE
            },
            MsFlags::empty(),
            NIX_NONE,
        )
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to mount previous root device '{}' to '{}' with type: {:?}",
                &root_device.display(),
                &root_fs_dir.display(),
                root_fs_type
            ),
        ))?;

        let stage2_cfg_file = path_append(&root_fs_dir, STAGE2_CFG_FILE);

        if !file_exists(&stage2_cfg_file) {
            let message = format!(
                "failed to locate stage2 config in {}",
                stage2_cfg_file.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }

        let stage2_cfg = Stage2Config::from_config(&stage2_cfg_file)?;

        info!(
            "Successfully read stage 2 config file from {}",
            stage2_cfg_file.display()
        );

        info!("Setting log level to {:?}", stage2_cfg.get_log_level());
        Logger::set_default_level(&stage2_cfg.get_log_level());
        if let Some((device, fstype)) = stage2_cfg.get_log_device() {
            Stage2::init_logging(device, fstype);
        }

        /*
        // TODO: probably paranoid
        if root_device != stage2_cfg.get_root_device() {
            let message = format!(
                "The device mounted as root does not match the former root device: {} != {}",
                root_device.display(),
                stage2_cfg.get_root_device().display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }
        */

        // Ensure /boot is mounted in ROOTFS_DIR/boot

        let boot_path = path_append(&root_fs_dir, BOOT_PATH);
        if !dir_exists(&boot_path)? {
            let message = format!(
                "cannot find boot mount point on root device: {}, path {}",
                root_device.display(),
                boot_path.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }

        // TODO: provide fstype for boot
        let boot_device = stage2_cfg.get_boot_device();
        let mut boot_mounted = false;
        if boot_device != root_device {
            mount(
                Some(boot_device),
                &boot_path,
                Some(stage2_cfg.get_boot_fstype()),
                MsFlags::empty(),
                NIX_NONE,
            )
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to mount previous boot device '{}' to '{}' with fstype: {}",
                    &boot_device.display(),
                    &boot_path.display(),
                    stage2_cfg.get_boot_fstype()
                ),
            ))?;
            boot_mounted = true;
        }

        return Ok(Stage2 {
            config: stage2_cfg,
            boot_mounted,
            recoverable_state: false,
            root_fs_path: root_fs_dir,
        });
    }

    pub fn migrate(&mut self) -> Result<(), MigError> {
        trace!("migrate: entered");
        let device_slug = self.config.get_device_slug();

        let mig_tmp_dir = Path::new(MIGRATE_TEMP_DIR);

        info!("migrating '{}'", &device_slug);

        // check if we have enough space to copy files to initramfs
        match get_mem_info() {
            Ok((mem_tot,mem_avail)) => {
                info!("Memory available is {} of {}", format_size_with_unit(mem_avail) , format_size_with_unit(mem_tot));

                let mut required_size = file_size(path_append(&self.root_fs_path, &self.config.get_balena_image()))?;
                required_size += file_size(path_append(&self.root_fs_path,&self.config.get_balena_config()))?;

                let work_dir = path_append(&self.root_fs_path,&self.config.get_work_path());

                if self.config.has_backup() {
                    required_size += file_size(path_append(&work_dir,BACKUP_FILE))?;
                }

                let src_nwmgr_dir = path_append(&work_dir, SYSTEM_CONNECTIONS_DIR);
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

                info!("Memory required for copying files is {}", format_size_with_unit(required_size));

                if mem_avail < required_size + STAGE2_MEM_THRESHOLD {
                    error!("Not enough memory available for copying files");
                    return Err(MigError::from_remark(MigErrorKind::InvState,"Not enough memory available for copying files" ));
                }

            },
            Err(why) => {
                warn!("Failed to retrieve mem info, error: {:?}", why);
            }
        }


        let device = device::from_device_slug(&device_slug)?;

        device.restore_boot(&self.root_fs_path, &self.config)?;

        // boot config restored can reboot
        self.recoverable_state = true;

        ensure_cmds(MIG_REQUIRED_CMDS, MIG_OPTIONAL_CMDS)?;

        if !dir_exists(mig_tmp_dir)? {
            create_dir(mig_tmp_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create migrate temp directory {}",
                    MIGRATE_TEMP_DIR
                ),
            ))?;
        }

        let src = path_append(&self.root_fs_path, self.config.get_balena_image());
        let tgt = path_append(mig_tmp_dir, BALENA_IMAGE_FILE);
        copy(&src, &tgt).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy balena image to migrate temp directory, '{}' -> '{}'",
                src.display(),
                tgt.display()
            ),
        ))?;

        info!("copied balena OS image to '{}'", tgt.display());

        let src = path_append(&self.root_fs_path, self.config.get_balena_config());
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

        let src_nwmgr_dir = path_append(
            &self.root_fs_path,
            path_append(self.config.get_work_path(), SYSTEM_CONNECTIONS_DIR),
        );
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
                        let tgt_path = path_append(&tgt_nwmgr_dir, &src_path.file_name().unwrap());
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
            let source_path = path_append(
                &self.root_fs_path,
                path_append(self.config.get_work_path(), BACKUP_FILE),
            );

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

        if self.boot_mounted {
            umount(&path_append(&self.root_fs_path, BOOT_PATH)).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to unmount former boot device: '{}'",
                    self.config.get_boot_device().display()
                ),
            ))?;
        }

        // Write our buffered log to workdir before unmounting boot if we are not flashing anyway
        if self.config.is_no_flash() && Logger::get_log_dest().is_buffer_dest() {
            let log_dest = path_append(path_append(&self.root_fs_path, self.config.get_work_path()), LOG_FILE_NAME);
            info!("Saving the log to '{}'", log_dest.display());
            Logger::flush();

            if let Some(buffer) = Logger::get_buffer() {
                if let Ok(file) = File::create(&log_dest) {
                    let mut writer = BufWriter::new(file);
                    let _res = writer.write(&buffer);
                    let _res = writer.flush();
                    sync();
                }
            }

            let _res = Logger::set_log_dest(&LogDestination::StreamStderr, NO_STREAM);
        }


        umount(&self.root_fs_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to unmount former root device: '{}'",
                self.config.get_root_device().display()
            ),
        ))?;

        info!("Unmounted root file system");

        // ************************************************************************************
        // * write the gzipped image to disk
        // * from migrate:
        // * gzip -d -c "${MIGRATE_TMP}/${IMAGE_FILE}" | dd of=${BOOT_DEV} bs=4194304 || fail  "failed with gzip -d -c ${MIGRATE_TMP}/${IMAGE_FILE} | dd of=${BOOT_DEV} bs=4194304"

        let target_path = self.config.get_flash_device();

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
            info!("Not flashing due to config parameter no_flash");
            Stage2::exit(&FailMode::Reboot)?;
        }

        if !self.config.is_skip_flash() {
            let image_path = path_append(mig_tmp_dir, BALENA_IMAGE_FILE);
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

            if let Ok(ref dd_cmd) = get_cmd(DD_CMD) {
                debug!("dd found at: {}", dd_cmd);

                let cmd_res_dd = if self.config.is_gzip_internal() {
                    let mut decoder =
                        GzDecoder::new(File::open(&image_path).context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to open gzip file for reading '{}'",
                                image_path.display()
                            ),
                        ))?);

                    debug!("invoking dd");

                    let mut dd_child = Command::new(dd_cmd)
                        .args(&[
                            &format!("of={}", &target_path.to_string_lossy()),
                            &format!("bs={}", DD_BLOCK_SIZE),
                        ])
                        .stdin(Stdio::piped())
                        .spawn()
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("failed to execute command {}", dd_cmd),
                        ))?;

                    self.recoverable_state = false;

                    let start_time = Instant::now();
                    let mut last_elapsed = Duration::new(0, 0);
                    let mut write_count: usize = 0;

                    if let Some(ref mut stdin) = dd_child.stdin {
                        let mut buffer: [u8; DD_BLOCK_SIZE] = [0; DD_BLOCK_SIZE];
                        loop {
                            let bytes_read =
                                decoder.read(&mut buffer).context(MigErrCtx::from_remark(
                                    MigErrorKind::Upstream,
                                    &format!(
                                        "Failed to read uncompressed data from '{}'",
                                        image_path.display()
                                    ),
                                ))?;
                            if bytes_read > 0 {
                                let bytes_written = stdin.write(&buffer[0..bytes_read]).context(
                                    MigErrCtx::from_remark(
                                        MigErrorKind::Upstream,
                                        "Failed to write to dd stdin",
                                    ),
                                )?;
                                write_count += bytes_written;

                                if bytes_read != bytes_written {
                                    error!(
                                        "Read/write count mismatch, read {}, wrote {}",
                                        bytes_read, bytes_written
                                    );
                                }
                                let curr_elapsed = start_time.elapsed();
                                let since_last = curr_elapsed.checked_sub(last_elapsed).unwrap();
                                if since_last.as_secs() >= 10 {
                                    last_elapsed = curr_elapsed;
                                    let secs_elapsed = curr_elapsed.as_secs();
                                    info!(
                                        "{} written @ {}/sec in {} seconds",
                                        format_size_with_unit(write_count as u64),
                                        format_size_with_unit(write_count as u64 / secs_elapsed),
                                        secs_elapsed
                                    );
                                }
                            } else {
                                break;
                            }
                        }

                        let secs_elapsed = start_time.elapsed().as_secs();
                        info!(
                            "{} written @ {}/sec in {} seconds",
                            format_size_with_unit(write_count as u64),
                            format_size_with_unit(write_count as u64 / secs_elapsed),
                            secs_elapsed
                        );
                        dd_child.wait_with_output().context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            "failed to wait for dd command",
                        ))?
                    } else {
                        return Err(MigError::from_remark(
                                MigErrorKind::NotFound,
                                "failed to flash image to target disk, gzip command, failed to retrieve dd stdin",
                            ));
                    }
                } else {
                    if let Ok(ref gzip_cmd) = get_cmd(GZIP_CMD) {
                        debug!("gzip found at: {}", gzip_cmd);
                        let gzip_child = Command::new(gzip_cmd)
                            .args(&["-d", "-c", &image_path.to_string_lossy()])
                            .stdout(Stdio::piped())
                            .spawn()
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("failed to spawn command {}", gzip_cmd),
                            ))?;

                        // TODO: implement progress for gzip or throw this out alltogether

                        if let Some(stdout) = gzip_child.stdout {
                            self.recoverable_state = false;
                            debug!("invoking dd");
                            Command::new(dd_cmd)
                                .args(&[
                                    &format!("of={}", &target_path.to_string_lossy()),
                                    &format!("bs={}", DD_BLOCK_SIZE),
                                ])
                                .stdin(stdout)
                                .output()
                                .context(MigErrCtx::from_remark(
                                    MigErrorKind::Upstream,
                                    &format!("failed to execute command {}", dd_cmd),
                                ))?
                        } else {
                            return Err(MigError::from_remark(
                                    MigErrorKind::NotFound,
                                    "failed to flash image to target disk, gzip command, failed to retrieved stdout",
                                ));
                        }
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::NotFound,
                            "failed to flash image to target disk, gzip command is not present",
                        ));
                    }
                };

                debug!("dd command result: {:?}", cmd_res_dd);

                if cmd_res_dd.status.success() != true {
                    return Err(MigError::from_remark(
                        MigErrorKind::ExecProcess,
                        &format!(
                            "dd terminated with exit code: {:?}",
                            cmd_res_dd.status.code()
                        ),
                    ));
                }

                // TODO: would like to check on gzip process status but ownership issues prevent it

                sync();

                info!(
                    "The Balena OS image has been written to the device '{}'",
                    target_path.display()
                );

                thread::sleep(Duration::new(PRE_PARTPROBE_WAIT_SECS, PARTPROBE_WAIT_NANOS));

                call_cmd(PARTPROBE_CMD, &[&target_path.to_string_lossy()], true)?;

                thread::sleep(Duration::new(
                    POST_PARTPROBE_WAIT_SECS,
                    PARTPROBE_WAIT_NANOS,
                ));

            // TODO: saw weird behaviour here, /dev/disk/by-label/resin-boot not found
            // does something like
            // 'udevadm settle --timeout=20 --exit-if-exists=/dev/disk/by-label/resin-boot'
            // make sense ?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "failed to flash image to target disk, dd command is not present",
                ));
            }
        }
        // check existence of partitions

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_BOOT_PART);

        if file_exists(&part_label) {
            info!("Found labeled partition for '{}'", part_label.display());

            let boot_device = path_append(
                part_label.parent().unwrap(),
                read_link(&part_label).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to read link: '{}'", part_label.display()),
                ))?,
            )
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to canonicalize path from: '{}'",
                    part_label.display()
                ),
            ))?;

            let boot_path = path_append(mig_tmp_dir, BOOT_MNT_DIR);

            create_dir(&boot_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to create mount dir: '{}'", boot_path.display()),
            ))?;

            mount(
                Some(&boot_device),
                &boot_path,
                Some(BALENA_BOOT_FSTYPE),
                MsFlags::empty(),
                NIX_NONE,
            )
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to mount balena device '{}' to '{}' with fstype: {}",
                    &boot_device.display(),
                    &boot_path.display(),
                    BALENA_BOOT_FSTYPE
                ),
            ))?;

            info!(
                "Mounted balena device '{}' on '{}'",
                &boot_device.display(),
                &boot_path.display()
            );

            // TODO: check fingerprints ?

            let src = path_append(mig_tmp_dir, BALENA_CONFIG_FILE);
            let tgt = path_append(&boot_path, BALENA_CONFIG_FILE);

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
                let tgt_path = path_append(&boot_path, SYSTEM_CONNECTIONS_DIR);
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
            self.recoverable_state = true;
        } else {
            let message = format!(
                "unable to find labeled partition: '{}'",
                part_label.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::NotFound, &message));
        }

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_ROOTA_PART);
        if !file_exists(&part_label) {
            let message = format!(
                "unable to find labeled partition: '{}'",
                part_label.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::NotFound, &message));
        }

        info!("Found labeled partition for '{}'", part_label.display());

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_ROOTB_PART);
        if !file_exists(&part_label) {
            let message = format!(
                "unable to find labeled partition: '{}'",
                part_label.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::NotFound, &message));
        }

        info!("Found labeled partition for '{}'", part_label.display());

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_STATE_PART);
        if !file_exists(&part_label) {
            let message = format!(
                "unable to find labeled partition: '{}'",
                part_label.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::NotFound, &message));
        }

        info!("Found labeled partition for '{}'", part_label.display());

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_DATA_PART);
        if file_exists(&part_label) {
            info!("Found labeled partition for '{}'", part_label.display());

            let data_device = path_append(
                part_label.parent().unwrap(),
                read_link(&part_label).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to read link: '{}'", part_label.display()),
                ))?,
            )
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to canonicalize path from: '{}'",
                    part_label.display()
                ),
            ))?;

            let data_path = path_append(mig_tmp_dir, DATA_MNT_DIR);
            create_dir(&data_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to create mount dir: '{}'", data_path.display()),
            ))?;

            mount(
                Some(&data_device),
                &data_path,
                Some(BALENA_DATA_FSTYPE),
                MsFlags::empty(),
                NIX_NONE,
            )
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to mount balena device '{}' on '{}' with fstype: {}",
                    &data_device.display(),
                    &data_path.display(),
                    BALENA_DATA_FSTYPE
                ),
            ))?;

            info!(
                "Mounted balena device '{}' on '{}'",
                &data_device.display(),
                &data_path.display()
            );

            if Logger::get_log_dest().is_buffer_dest() {
                Logger::flush();

                if let Some(buffer) = Logger::get_buffer() {
                    let log_dest = path_append(&data_path, LOG_FILE_NAME);
                    if let Ok(file) = File::create(&log_dest) {
                        let mut writer = BufWriter::new(file);
                        let _res = writer.write(&buffer);
                        let _res = writer.flush();
                        let _res =
                            Logger::set_log_dest(&LogDestination::StreamStderr, Some(writer));
                        info!("Set up logger to log to '{}'", log_dest.display());
                    }
                }
            }

            // TODO: copy log, backup to data_path
            if self.config.has_backup() {
                // TODO: check available disk space
                let source_path = path_append(&mig_tmp_dir, BACKUP_FILE);
                let target_path = path_append(&data_path, BACKUP_FILE);

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
        } else {
            let message = format!(
                "unable to find labeled partition: '{}'",
                part_label.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::NotFound, &message));
        }

        info!(
            "Migration stage 2 was successful, rebooting in {} seconds!",
            REBOOT_DELAY
        );

        thread::sleep(Duration::new(REBOOT_DELAY, 0));

        Stage2::exit(&FailMode::Reboot)?;

        Ok(())
    }

    fn exit(fail_mode: &FailMode) -> Result<(), MigError> {
        trace!("exit: entered with {:?}", fail_mode);

        Logger::flush();
        sync();

        match fail_mode {
            FailMode::Reboot => {
                reboot(RebootMode::RB_AUTOBOOT).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    "failed to reboot",
                ))?;
            }
            FailMode::RescueShell => {
                std::process::exit(1);
            }
        }
        Ok(())
    }

    pub(crate) fn is_recoverable(&self) -> bool {
        self.recoverable_state
    }

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

    fn init_logging(device: &Path, fstype: &str) {
        trace!(
            "init_logging: entered with '{}' fstype: {}",
            device.display(),
            fstype
        );
        info!(
            "Attempting to set up logging to '{}' with fstype: {}",
            device.display(),
            fstype
        );

        let log_mnt_dir = PathBuf::from(LOG_MOUNT_DIR);

        if !if let Ok(res) = dir_exists(&log_mnt_dir) {
            res
        } else {
            warn!("unable to stat path {}", log_mnt_dir.display());
            return;
        } {
            if let Err(_why) = create_dir(&log_mnt_dir) {
                warn!(
                    "failed to create log mount directory directory {}",
                    log_mnt_dir.display()
                );
                return;
            }
        } else {
            warn!("root mount directory {} exists", log_mnt_dir.display());
        }

        debug!(
            "Attempting to mount mount dir '{}' on '{}'",
            device.display(),
            log_mnt_dir.display()
        );

        if let Err(_why) = mount(
            Some(device),
            &log_mnt_dir,
            Some(fstype.as_bytes()),
            MsFlags::empty(),
            NIX_NONE,
        ) {
            warn!(
                "Failed to mount log device '{}' to '{}' with type: {:?}",
                &device.display(),
                &log_mnt_dir.display(),
                fstype
            );
            return;
        }

        let log_file = path_append(&log_mnt_dir, LOG_FILE_NAME);

        let mut writer = BufWriter::new(match File::create(&log_file) {
            Ok(file) => file,
            Err(_why) => {
                warn!("Failed to create log file '{}' ", log_file.display(),);
                return;
            }
        });

        debug!("Attempting to flush log buffer to '{}'", log_file.display());
        if let Some(buffer) = Logger::get_buffer() {
            if let Err(_why) = writer.write(&buffer) {
                warn!("Failed to write to log file '{}' ", log_file.display(),);
                return;
            }
        }

        if let Err(_why) = Logger::set_log_dest(&LogDestination::StreamStderr, Some(writer)) {
            warn!("Failed to set logfile to file '{}' ", log_file.display(),);
        } else {
            info!("Set up logger to log to '{}'", log_file.display());
        }
    }
}
