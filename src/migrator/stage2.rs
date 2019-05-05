use failure::ResultExt;
use log::{debug, error, info, warn};
use mod_logger::Logger;
use nix::{
    mount::{mount, umount, MsFlags},
    sys::reboot::{reboot, RebootMode},
};
use regex::Regex;
use std::fs::{copy, create_dir, read_dir};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::{
    common::{dir_exists, file_exists, parse_file, MigErrCtx, MigError, MigErrorKind},
    defs::{BOOT_PATH, STAGE2_CFG_FILE, SYSTEM_CONNECTIONS_DIR},
    linux_common::{
        call_cmd, ensure_cmds, get_cmd, path_append, Device, FailMode, DD_CMD, GZIP_CMD,
        PARTPROBE_CMD, REBOOT_CMD,
    },
};

pub(crate) mod stage2_config;
pub(crate) use stage2_config::Stage2Config;

use crate::beaglebone::BeagleboneGreen;
use crate::intel_nuc::IntelNuc;
use crate::raspberrypi::RaspberryPi3;

// for starters just restore old boot config, only required command is mount

// later ensure all other required commands

const INIT_LOG_LEVEL: &str = "debug";
const KERNEL_CMDLINE: &str = "/proc/cmdline";
const ROOT_DEVICE_REGEX: &str = r#"\sroot=(\S+)\s"#;
const ROOT_FSTYPE_REGEX: &str = r#"\srootfstype=(\S+)\s"#;
const ROOTFS_DIR: &str = "/tmp_root";
const MIGRATE_TEMP_DIR: &str = "/migrate_tmp";

const DD_BLOCK_SIZE: u64 = 4194304;

const MIG_REQUIRED_CMDS: &'static [&'static str] = &[DD_CMD, PARTPROBE_CMD, REBOOT_CMD];
const MIG_OPTIONAL_CMDS: &'static [&'static str] = &[];

const BALENA_IMAGE_FILE: &str = "balenaOS.img.gz";
const BALENA_CONFIG_FILE: &str = "config.json";

const NIX_NONE: Option<&'static [u8]> = None;

pub(crate) struct Stage2 {
    config: Stage2Config,
    boot_mounted: bool,
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
        let root_device = if let Some(parse_res) =
            parse_file(KERNEL_CMDLINE, &Regex::new(&ROOT_DEVICE_REGEX).unwrap())?
        {
            PathBuf::from(parse_res.get(1).unwrap().trim_matches(char::from(0)))
        } else {
            // TODO: manually scan possible devices for config file
            return Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!("failed to parse {} for root device", KERNEL_CMDLINE),
            ));
        };

        let root_fs_type = if let Some(parse_res) =
            parse_file(KERNEL_CMDLINE, &Regex::new(&ROOT_FSTYPE_REGEX).unwrap())?
        {
            String::from(
                parse_res
                    .get(1)
                    .unwrap()
                    .as_str()
                    .trim_matches(char::from(0)),
            )
        } else {
            // TODO: manually scan possible devices for config file
            return Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!("failed to parse {} for root fs type", KERNEL_CMDLINE),
            ));
        };

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
            Some(root_fs_type.as_str()),
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
        });
    }

    pub fn migrate(&self) -> Result<(), MigError> {
        let device_slug = self.config.get_device_slug();

        let root_fs_dir = Path::new(ROOTFS_DIR);
        let mig_tmp_dir = Path::new(MIGRATE_TEMP_DIR);

        info!("migrating '{}'", &device_slug);

        let device = match device_slug {
            "beaglebone-green" => {
                let device: Box<Device> = Box::new(BeagleboneGreen::new());
                device
            }
            "raspberrypi-3" => {
                let device: Box<Device> = Box::new(RaspberryPi3::new());
                device
            }
            "intel-nuc" => {
                let device: Box<Device> = Box::new(IntelNuc::new());
                device
            }
            _ => {
                let message = format!("unexpected device type: {}", device_slug);
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }
        };

        device.restore_boot(&PathBuf::from(ROOTFS_DIR), &self.config)?;

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
                        copy(&src_path,&tgt_path)
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

        if self.config.is_no_flash() {
            info!("Not flashing due to config parameter no_flash");
            Stage2::exit(&FailMode::Reboot)?;
        }

        // ************************************************************************************
        // * write the gzipped image to disk
        // TODO: try using internal gzip
        // TODO: test-flash to external device
        // * from migrate:
        // * gzip -d -c "${MIGRATE_TMP}/${IMAGE_FILE}" | dd of=${BOOT_DEV} bs=4194304 || fail  "failed with gzip -d -c ${MIGRATE_TMP}/${IMAGE_FILE} | dd of=${BOOT_DEV} bs=4194304"

        let image_path = path_append(mig_tmp_dir, self.config.get_balena_image());
        let target_path = self.config.get_flash_device();

        info!(
            "flashing '{}' to '{}'",
            image_path.display(),
            target_path.display()
        );
        if let Ok(ref gzip_cmd) = get_cmd(GZIP_CMD) {
            if let Ok(ref dd_cmd) = get_cmd(DD_CMD) {
                let cmd1 = Command::new(gzip_cmd)
                    .args(&["-d", "-c", &image_path.to_string_lossy()])
                    .stdout(Stdio::piped())
                    .spawn()
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("failed to spawn command {}", gzip_cmd),
                    ))?;

                if let Some(cmd1_stdout) = cmd1.stdout {
                    let cmd_res = Command::new(dd_cmd)
                        .args(&[
                            &format!("of={}", &target_path.to_string_lossy()),
                            &format!("bs={}", DD_BLOCK_SIZE),
                        ])
                        .stdin(cmd1_stdout)
                        .output()
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("failed to execute command {}", dd_cmd),
                        ))?;
                    debug!("dd command result: {:?}", cmd_res);
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvState,
                        "failed to flash image to target disk, gzip stdout not present",
                    ));
                }
            }
        }

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

    pub(crate) fn default_exit() -> Result<(), MigError> {
        Stage2::exit(FailMode::get_default())
    }

    pub(crate) fn error_exit(&self) -> Result<(), MigError> {
        Stage2::exit(self.config.get_fail_mode())
    }
}
