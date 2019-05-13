use failure::{ResultExt};
use log::{debug, error, info, warn};
use mod_logger::Logger;
use nix::{
    mount::{mount, umount, MsFlags},
    sys::reboot::{reboot, RebootMode},
    unistd::sync,
};
use regex::Regex;
use std::fs::{read_to_string, copy, create_dir, read_dir, read_link};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::{
    common::{
        dir_exists, file_exists, parse_file, path_append, FailMode, MigErrCtx, MigError,
        MigErrorKind,
    },
    defs::{
        BALENA_BOOT_FSTYPE, BALENA_BOOT_PART, BALENA_DATA_FSTYPE, BALENA_DATA_PART,
        BALENA_ROOTA_PART, BALENA_ROOTB_PART, BALENA_STATE_PART, BOOT_PATH, DISK_BY_LABEL_PATH,
        DISK_BY_PARTUUID_PATH, STAGE2_CFG_FILE, SYSTEM_CONNECTIONS_DIR,
    },
    linux_common::{
        call_cmd, ensure_cmds, get_cmd, DD_CMD, GZIP_CMD, PARTPROBE_CMD, REBOOT_CMD,
    },

    device::{self},
};

pub(crate) mod stage2_config;
pub(crate) use stage2_config::Stage2Config;

// for starters just restore old boot config, only required command is mount

// later ensure all other required commands

const REBOOT_DELAY: u64 = 3;

const INIT_LOG_LEVEL: &str = "debug";
const KERNEL_CMDLINE: &str = "/proc/cmdline";
const ROOT_DEVICE_REGEX: &str = r#"\sroot=(\S+)\s"#;
const ROOT_PARTUUID_REGEX: &str = r#"^PARTUUID=(\S+)$"#;

const ROOT_FSTYPE_REGEX: &str = r#"\srootfstype=(\S+)\s"#;
const ROOTFS_DIR: &str = "/tmp_root";
const MIGRATE_TEMP_DIR: &str = "/migrate_tmp";
const BOOT_MNT_DIR: &str = "mnt_boot";
const DATA_MNT_DIR: &str = "mnt_data";

const DD_BLOCK_SIZE: u64 = 4194304;

const MIG_REQUIRED_CMDS: &'static [&'static str] = &[DD_CMD, PARTPROBE_CMD, GZIP_CMD, REBOOT_CMD];
const MIG_OPTIONAL_CMDS: &'static [&'static str] = &[];

const BALENA_IMAGE_FILE: &str = "balenaOS.img.gz";
const BALENA_CONFIG_FILE: &str = "config.json";

const NIX_NONE: Option<&'static [u8]> = None;
const PARTPROBE_WAIT_SECS: u64 = 5;
const PARTPROBE_WAIT_NANOS: u32 = 0;

pub(crate) struct Stage2 {
    config: Stage2Config,
    boot_mounted: bool,
    recoverable_state: bool,
}

impl Stage2 {
    pub fn try_init() -> Result<Stage2, MigError> {
        // TODO:

        match Logger::initialise(Some(INIT_LOG_LEVEL)) {
            Ok(_s) => info!("Balena Migrate Stage 2 initializing"),
            Err(_why) => {
                println!("Balena Migrate Stage 2 initializing");
                println!("failed to initalize logger");
            }
        }

        let root_fs_dir = Path::new(ROOTFS_DIR);

        // TODO: beaglebone version - make device_slug dependant

        let cmd_line = read_to_string(KERNEL_CMDLINE).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to read file: '{}'", KERNEL_CMDLINE)))?;

        let root_device =
            if let Some(captures) = Regex::new(ROOT_DEVICE_REGEX).unwrap().captures(&cmd_line) {
                let root_dev = captures.get(1).unwrap().as_str();
                if let Some(captures) = Regex::new(ROOT_PARTUUID_REGEX).unwrap().captures(root_dev) {
                    let uuid_part = path_append(DISK_BY_PARTUUID_PATH, captures.get(1).unwrap().as_str());
                    if file_exists(&uuid_part) {
                        path_append(
                            uuid_part.parent().unwrap(),
                            read_link(&uuid_part).context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("failed to read link: '{}'", uuid_part.display()),
                            ))?,
                        )
                            .canonicalize()
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "failed to canonicalize path from: '{}'",
                                    uuid_part.display()
                                ),
                            ))?
                    } else {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Failed to get root device from part-uuid: '{}'", root_dev)));
                    }
                } else {
                    PathBuf::from(root_dev)
                }
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Failed to parse root device from kernel command line: '{}'", cmd_line)));
            };

        let root_fs_type =
            if let Some(captures) = Regex::new(&ROOT_FSTYPE_REGEX).unwrap().captures(&cmd_line) {
                captures.get(1).unwrap().as_str()
            }  else {
                // TODO: manually scan possible devices for config file
                return Err(MigError::from_remark(
                    MigErrorKind::InvState,
                    &format!("failed to parse {} for root fs type", KERNEL_CMDLINE),
                ));
            };

        info!("Using root device '{}' with fs-type: '{}'", root_device.display(), root_fs_type);

        if !dir_exists(ROOTFS_DIR)? {
            create_dir(ROOTFS_DIR).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to create mountpoint for roofs in {}", ROOTFS_DIR),
            ))?;
        } else {
            warn!("root mount directory {} exists", ROOTFS_DIR);
        }

        // TODO: add options to make this more reliable)

        mount(
            Some(&root_device),
            root_fs_dir,
            Some(root_fs_type),
            MsFlags::empty(),
            NIX_NONE,
        )
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to mount previous root device '{}' to '{}' with type: {}",
                &root_device.display(),
                &root_fs_dir.display(),
                root_fs_type
            ),
        ))?;

        let stage2_cfg_file = path_append(root_fs_dir, STAGE2_CFG_FILE);

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
            "Read stage 2 config file from {}",
            stage2_cfg_file.display()
        );

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

        // Ensure /boot is mounted in ROOTFS_DIR/boot

        let boot_path = path_append(root_fs_dir, BOOT_PATH);
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
        });
    }

    pub fn migrate(&mut self) -> Result<(), MigError> {
        let device_slug = self.config.get_device_slug();

        let root_fs_dir = Path::new(ROOTFS_DIR);
        let mig_tmp_dir = Path::new(MIGRATE_TEMP_DIR);

        info!("migrating '{}'", &device_slug);

        let device= device::from_device_slug(&device_slug)?;

        device.restore_boot(&PathBuf::from(ROOTFS_DIR), &self.config)?;

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

        let src = path_append(root_fs_dir, self.config.get_balena_image());
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

        let src = path_append(root_fs_dir, self.config.get_balena_config());
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
            root_fs_dir,
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

        info!("Files copied to RAMFS");

        if self.boot_mounted {
            umount(&path_append(ROOTFS_DIR, BOOT_PATH)).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to unmount former boot device: '{}'",
                    self.config.get_boot_device().display()
                ),
            ))?;
        }

        umount(ROOTFS_DIR).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to unmount former root device: '{}'",
                self.config.get_root_device().display()
            ),
        ))?;

        info!("Unmounted root file system");

        // ************************************************************************************
        // * write the gzipped image to disk
        // TODO: try using internal gzip

        // TODO: test-flash to external device
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

            if let Ok(ref gzip_cmd) = get_cmd(GZIP_CMD) {
                debug!("gzip found at: {}", gzip_cmd);
                if let Ok(ref dd_cmd) = get_cmd(DD_CMD) {
                    debug!("dd found at: {}", dd_cmd);
                    let gzip_child = Command::new(gzip_cmd)
                        .args(&["-d", "-c", &image_path.to_string_lossy()])
                        .stdout(Stdio::piped())
                        .spawn()
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("failed to spawn command {}", gzip_cmd),
                        ))?;

                    debug!("invoking dd");

                    // TODO: plug a progress indicator into the pipe
                    // Idea extract uncompressed size from gzip
                    // Plug into the pipe
                    // report progress at set time or data intervals
                    // TODO: use crate flate2 instead of external gzip

                    if let Some(gzip_stdout) = gzip_child.stdout {
                        // flashing the device - not recoverable after this
                        self.recoverable_state = false;

                        let cmd_res_dd = Command::new(dd_cmd)
                            .args(&[
                                &format!("of={}", &target_path.to_string_lossy()),
                                &format!("bs={}", DD_BLOCK_SIZE),
                            ])
                            .stdin(gzip_stdout)
                            .output()
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("failed to execute command {}", dd_cmd),
                            ))?;

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

                        // TODO: sync !
                        sync();

                        info!(
                            "The Balena OS image has been written to the device '{}'",
                            target_path.display()
                        );

                        call_cmd(PARTPROBE_CMD, &[&target_path.to_string_lossy()], true)?;

                        thread::sleep(Duration::new(PARTPROBE_WAIT_SECS, PARTPROBE_WAIT_NANOS));
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvState,
                            "failed to flash image to target disk, gzip stdout not present",
                        ));
                    }
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        "failed to flash image to target disk, dd command is not present",
                    ));
                }
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    "failed to flash image to target disk, gzip command is not present",
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

        // TODO: copy log, backup to data_path
        // TODO: write logs to data_path
        } else {
            let message = format!(
                "unable to find labeled partition: '{}'",
                part_label.display()
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::NotFound, &message));
        }

        sync();

        info!(
            "Migration stage 2 was successful, rebooting in {} seconds!",
            REBOOT_DELAY
        );

        thread::sleep(Duration::new(REBOOT_DELAY, 0));

        Stage2::exit(&FailMode::Reboot)?;

        Ok(())
    }

    fn exit(fail_mode: &FailMode) -> Result<(), MigError> {
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
        Stage2::exit(FailMode::get_default())
    }

    pub(crate) fn error_exit(&self) -> Result<(), MigError> {
        if self.recoverable_state {
            Stage2::exit(self.config.get_fail_mode())
        } else {
            Stage2::exit(&FailMode::RescueShell)
        }
    }
}
