use failure::ResultExt;
use log::{debug, error, info, trace};
use regex::Regex;
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::Path;

use crate::{
    common::{
        boot_manager::BootManager,
        call, dir_exists,
        disk_util::LabelType,
        file_digest::check_digest,
        file_exists, format_size_with_unit,
        migrate_info::MigrateInfo,
        path_append,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, MIG_INITRD_NAME, MIG_KERNEL_NAME},
    linux::{
        linux_defs::{
            BOOT_PATH, GRUB_CONFIG_DIR, GRUB_CONFIG_FILE, GRUB_MIN_VERSION, KERNEL_CMDLINE_PATH,
            ROOT_PATH,
        },
        linux_defs::{CHMOD_CMD, GRUB_REBOOT_CMD, GRUB_UPDT_CMD},
        lsblk_info::LsblkInfo,
        stage2::mounts::Mounts,
    },
};

const GRUB_UPDT_VERSION_ARGS: [&str; 1] = ["--version"];
const GRUB_UPDT_VERSION_RE: &str = r#"^.*\s+\(GRUB\)\s+([0-9]+)\.([0-9]+)[^0-9].*$"#;

const GRUB_CFG_TEMPLATE: &str = r##"
#!/bin/sh
exec tail -n +3 $0
# This file provides an easy way to add custom menu entries.  Simply type the
# menu entries you want to add after this comment.  Be careful not to change
# the 'exec tail' line above.

menuentry "balena-migrate" {
  insmod gzio
  insmod __PART_MOD__
  insmod __FSTYPE_MOD__

  __ROOT_CMD__
  linux __LINUX__
  initrd  __INITRD_NAME__
}
"##;

pub(crate) struct GrubBootManager {
    // valid is just used to enforce the use of new
    bootmgr_path: Option<PathInfo>,
}

impl<'a> GrubBootManager {
    pub fn new() -> GrubBootManager {
        GrubBootManager { bootmgr_path: None }
    }

    /******************************************************************
     * Ensure grub (update-grub) exists and retrieve its version
     * as (major,minor)
     ******************************************************************/

    fn get_grub_version() -> Result<(String, String), MigError> {
        trace!("get_grub_version: entered");

        let cmd_res =
            call(GRUB_UPDT_CMD, &GRUB_UPDT_VERSION_ARGS, true).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "get_grub_version: call '{} {:?}'",
                    GRUB_UPDT_CMD, GRUB_UPDT_VERSION_ARGS
                ),
            ))?;

        if cmd_res.status.success() {
            let re = Regex::new(GRUB_UPDT_VERSION_RE).unwrap();
            if let Some(captures) = re.captures(cmd_res.stdout.as_ref()) {
                Ok((
                    String::from(captures.get(1).unwrap().as_str()),
                    String::from(captures.get(2).unwrap().as_str()),
                ))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "get_grub_version: failed to parse grub version string: {}",
                        cmd_res.stdout
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "get_os_arch: command failed: {}",
                    cmd_res.status.code().unwrap_or(0)
                ),
            ))
        }
    }
}

impl<'a> BootManager for GrubBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Grub
    }

    fn get_bootmgr_path(&self) -> PathInfo {
        self.bootmgr_path.as_ref().unwrap().clone()
    }
    // TODO: do we need to distiguish like in u-boot ?
    fn get_boot_path(&self) -> PathInfo {
        self.bootmgr_path.as_ref().unwrap().clone()
    }

    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        trace!("can_migrate: entered");

        // TODO: several things to do:
        //  make sure grub is actually the active boot manager

        if !dir_exists(BOOT_PATH)? || !dir_exists(GRUB_CONFIG_DIR)? {
            error!("Some directories required for the grub boot manager could not be found");
            return Ok(false);
        }

        let lsblk_info = LsblkInfo::all()?;

        let boot_path = if let Some(boot_path) = PathInfo::from_path(BOOT_PATH, &lsblk_info)? {
            boot_path
        } else {
            error!("Could not find boot path '{}'", BOOT_PATH);
            return Err(MigError::displayed());
        };

        let grub_version = GrubBootManager::get_grub_version()?;
        info!(
            "grub-install version is {}.{}",
            grub_version.0, grub_version.1
        );

        if grub_version.0 < String::from(GRUB_MIN_VERSION) {
            error!("Your version of grub-install ({}.{}) is not supported. balena-migrate requires grub version 2 or higher.", grub_version.0, grub_version.1);
            return Ok(false);
        }

        if !dir_exists(GRUB_CONFIG_DIR)? {
            error!(
                "The grub configuration directory '{}' could not be found.",
                GRUB_CONFIG_DIR
            );
            return Ok(false);
        }

        // TODO: this could be more reliable, taking into account the size of the existing files
        // vs the size of the files that will be copied

        let mut boot_req_space = if !file_exists(path_append(&boot_path.path, MIG_KERNEL_NAME)) {
            mig_info.kernel_file.size
        } else {
            0
        };

        boot_req_space += if !file_exists(path_append(&boot_path.path, MIG_INITRD_NAME)) {
            mig_info.initrd_file.size
        } else {
            0
        };

        if boot_path.fs_free < boot_req_space {
            error!("The boot directory '{}' does not have enough space to store the migrate kernel and initramfs. Required space is {}",
                   boot_path.path.display(), format_size_with_unit(boot_req_space));
            return Ok(false);
        }

        self.bootmgr_path = Some(boot_path);

        Ok(true)
    }

    fn setup(
        &self,
        mig_info: &MigrateInfo,
        _s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        trace!("setup: entered");

        // TODO: implement
        // b) create a boot config for balena migration
        // c) call grub-reboot to enable boot once to migrate env

        // let install_drive = mig_info.get_installPath().drive;
        let boot_path = self.bootmgr_path.as_ref().unwrap();

        // path to kernel & initramfs at boot time depends on how /boot is mounted
        // either / (for a /boot mount or /boot for a directory of /root file system)
        let grub_boot: &Path = if boot_path.path == boot_path.mountpoint {
            // /boot is a mounted filesystem
            &Path::new(ROOT_PATH)
        } else {
            // /boot is a directory on /root
            &boot_path.path
        };

        let part_type = match LabelType::from_device(&boot_path.device_info.drive)? {
            LabelType::GPT => "gpt",
            LabelType::Dos => "msdos",
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Invalid partition type for '{}'",
                        boot_path.device_info.drive.display()
                    ),
                ));
            }
        };

        let part_mod = format!("part_{}", part_type);

        info!(
            "Boot partition type for '{}' is '{}'",
            boot_path.device_info.drive.display(),
            part_mod
        );

        let root_cmd = if let Some(ref uuid) = boot_path.device_info.uuid {
            // TODO: try partuuid too ?local setRootA="set root='${GRUB_BOOT_DEV},msdos${ROOT_PART_NO}'"
            format!("search --no-floppy --fs-uuid --set=root {}", uuid)
        } else {
            format!(
                "search --no-floppy --fs-uuid --set=root {},{}{}",
                boot_path.device_info.drive.to_string_lossy(),
                part_type,
                boot_path.device_info.index
            )
        };

        debug!("root set to '{}", root_cmd);

        let fstype_mod = match boot_path.device_info.fs_type.as_str() {
            "ext2" | "ext3" | "ext4" => "ext2",
            "vfat" => "fat",
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Cannot determine grub mod for boot fs type '{}'",
                        boot_path.device_info.fs_type
                    ),
                ));
            }
        };

        let mut linux = String::from(path_append(&grub_boot, MIG_KERNEL_NAME).to_string_lossy());

        // filter some bullshit out of commandline, else leave it as is

        for word in read_to_string(KERNEL_CMDLINE_PATH)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Unable to read kernel command line from '{}'",
                    KERNEL_CMDLINE_PATH
                ),
            ))?
            .split_whitespace()
        {
            let word_lc = word.to_lowercase();

            if word_lc.starts_with("console=") {
                continue;
            }

            if word_lc.starts_with("boot_image=") {
                continue;
            }

            if word.to_lowercase() == "debug" {
                continue;
            }

            if word.starts_with("rootfstype=") {
                continue;
            }

            linux.push_str(&format!(" {}", word));
        }

        linux.push_str(&format!(
            " rootfstype={} console=tty0 debug",
            boot_path.device_info.fs_type
        ));

        if !kernel_opts.is_empty() {
            linux.push(' ');
            linux.push_str(kernel_opts);
        }

        let mut grub_cfg = String::from(GRUB_CFG_TEMPLATE);

        grub_cfg = grub_cfg.replace("__PART_MOD__", &part_mod);
        grub_cfg = grub_cfg.replace("__FSTYPE_MOD__", &fstype_mod);
        grub_cfg = grub_cfg.replace("__ROOT_CMD__", &root_cmd);
        grub_cfg = grub_cfg.replace("__LINUX__", &linux);
        grub_cfg = grub_cfg.replace(
            "__INITRD_NAME__",
            &path_append(&grub_boot, MIG_INITRD_NAME).to_string_lossy(),
        );

        debug!("grub config: {}", grub_cfg);

        // let mut grub_cfg_file =
        File::create(GRUB_CONFIG_FILE)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to create grub config file '{}'", GRUB_CONFIG_FILE),
            ))?
            .write(grub_cfg.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to write to grub config file '{}'", GRUB_CONFIG_FILE),
            ))?;

        let cmd_res = call(CHMOD_CMD, &["+x", GRUB_CONFIG_FILE], true)?;
        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!("Failure from '{}': {:?}", CHMOD_CMD, cmd_res),
            ));
        }

        info!("Grub config written to '{}'", GRUB_CONFIG_FILE);

        // **********************************************************************
        // ** copy new kernel & iniramfs

        let kernel_path = path_append(&boot_path.path, MIG_KERNEL_NAME);

        std::fs::copy(&mig_info.kernel_file.path, &kernel_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy kernel file '{}' to '{}'",
                mig_info.kernel_file.path.display(),
                kernel_path.display()
            ),
        ))?;

        if !check_digest(&kernel_path, &mig_info.kernel_file.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied kernel file '{}' to {:?}",
                    kernel_path.display(),
                    mig_info.kernel_file.hash_info
                ),
            ));
        }

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            kernel_path.display()
        );

        call(CHMOD_CMD, &["+x", &kernel_path.to_string_lossy()], false)?;

        let initrd_path = path_append(&boot_path.path, MIG_INITRD_NAME);
        std::fs::copy(&mig_info.initrd_file.path, &initrd_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy initrd file '{}' to '{}'",
                mig_info.initrd_file.path.display(),
                initrd_path.display()
            ),
        ))?;

        if !check_digest(&initrd_path, &mig_info.initrd_file.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied initrd file '{}' to {:?}",
                    initrd_path.display(),
                    mig_info.initrd_file.hash_info
                ),
            ));
        }

        info!(
            "initramfs kernel: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            initrd_path.display()
        );

        info!("calling '{}'", GRUB_UPDT_CMD);

        let cmd_res = call(GRUB_UPDT_CMD, &[], true).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to set up boot configuration'",
        ))?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!("Failure from '{}': {:?}", GRUB_UPDT_CMD, cmd_res),
            ));
        }

        info!("calling '{}'", GRUB_REBOOT_CMD);

        let cmd_res =
            call(GRUB_REBOOT_CMD, &["balena-migrate"], true).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to activate boot configuration using '{}'",
                    GRUB_REBOOT_CMD,
                ),
            ))?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "Failed to activate boot configuration using '{}': {:?}",
                    GRUB_REBOOT_CMD, cmd_res
                ),
            ));
        }

        Ok(())
    }

    fn restore(&self, _mounts: &Mounts, _config: &Stage2Config) -> bool {
        trace!("restore: entered");
        // Nothing to restore with grub-reboot
        // TODO: might be worthwhile to remove kernel / initramfs and grub config
        // TODO: restore backups anyway if they exist
        true
    }
    /*
        fn set_bootmgr_path(&self,dev_info:& DeviceInfo, config: &Config, s2_cfg: &mut Stage2ConfigBuilder) -> Result<bool, MigError> {
            trace!("set_bootmgr_path: entered");
    */
    /*

    match boot_type {
            BootType::EFI => {
                // TODO: this is EFI specific stuff in a non EFI specific place - try to concentrate uboot / EFI stuff in dedicated module
                if let Some(path_info) = PathInfo::new(EFI_PATH, &lsblk_info)? {
                    Some(path_info)
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!(
                            "the device for path '{}' could not be established",
                            EFI_PATH
                        ),
                    ));
                }
            }
            BootType::UBoot => DiskInfo::get_uboot_mgr_path(&work_path, &lsblk_info)?,
            _ => None,
        },
    */
    /*

        Err(MigError::from(MigErrorKind::NotImpl))
    }
    */
}

/*
pub(crate) fn grub_valid(_config: &Config, _mig_info: &MigrateInfo) -> Result<bool, MigError> {
}

pub(crate) fn grub_install(_config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
}
*/
