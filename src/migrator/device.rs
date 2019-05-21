use failure::{Fail, ResultExt};
use log::{debug, error, info, warn};
use std::fs::{read_to_string, File};
use std::path::{Path, PathBuf};
use std::io::Write;

use crate::linux_common::call_cmd;
use crate::{
    common::{call, path_append, Config, MigErrCtx, MigError, MigErrorKind, OSArch},
    //linux::LinuxMigrator,
    defs::{
        BOOT_PATH, GRUB_CONF_PATH, KERNEL_CMDLINE_PATH, MIG_INITRD_NAME, MIG_KERNEL_NAME, ROOT_PATH,
    },
    linux_common::{
        disk_info::label_type::LabelType, get_grub_version, whereis, MigrateInfo, CHMOD_CMD,
        GRUB_REBOOT_CMD, GRUB_UPDT_CMD,
    },
    stage2::Stage2Config,
};



mod beaglebone;
mod intel_nuc;
mod raspberrypi;

const MODULE: &str = "device";
const DEVICE_TREE_MODEL: &str = "/proc/device-tree/model";

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

pub(crate) trait Device {
    fn get_device_slug(&self) -> &'static str;
    fn can_migrate(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError>;
    // fn is_supported_os(&self, mig_info: &MigrateInfo) -> Result<bool, MigError>;
    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError>;
    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError>;
}

pub(crate) fn from_device_slug(slug: &str) -> Result<Box<Device>, MigError> {
    match slug {
        "beaglebone-green" => Ok(Box::new(beaglebone::BeagleboneGreen::new())),
        "raspberrypi3" => Ok(Box::new(raspberrypi::RaspberryPi3::new())),
        "intel-nuc" => Ok(Box::new(intel_nuc::IntelNuc::new())),
        _ => {
            let message = format!("unexpected device type: {}", &slug);
            error!("{}", &message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
    }
}

pub(crate) fn get_device(mig_info: &MigrateInfo) -> Result<Box<Device>, MigError> {
    let os_arch = mig_info.get_os_arch();
    match os_arch {
        OSArch::ARMHF => {
            let dev_tree_model =
                read_to_string(DEVICE_TREE_MODEL).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "{}::init_armhf: unable to determine model due to inaccessible file '{}'",
                        MODULE, DEVICE_TREE_MODEL
                    ),
                ))?;

            if let Ok(device) = raspberrypi::is_rpi(&dev_tree_model) {
                return Ok(device);
            }

            if let Ok(device) = beaglebone::is_bb(&dev_tree_model) {
                return Ok(device);
            }

            let message = format!(
                "Your device type: '{}' is not supported by balena-migrate.",
                dev_tree_model
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
        OSArch::AMD64 => Ok(Box::new(intel_nuc::IntelNuc::new())),
        /*            OSArch::I386 => {
                    migrator.init_i386()?;
                },
        */
        _ => {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::get_device: unexpected OsArch encountered: {}",
                    MODULE, os_arch
                ),
            ));
        }
    }
}

pub(crate) fn grub_valid(_config: &Config, _mig_info: &MigrateInfo) -> Result<bool, MigError> {
    let grub_version = match get_grub_version() {
        Ok(version) => version,
        Err(why) => match why.kind() {
            MigErrorKind::NotFound => {
                warn!("The grub version could not be established, grub does not appear to be installed");
                return Ok(false);
            }
            _ => return Err(why),
        },
    };

    // TODO: check more indications of a valid grub installation
    // TODO: really expect versions > 2 to be downwards compatible ?
    debug!("found update-grub version: '{:?}'", grub_version);
    Ok(grub_version
        .0
        .parse::<u8>()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to establish grub version from '{}'", grub_version.0),
        ))?
        >= 2)
}

pub(crate) fn grub_install(_config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
    // TODO: implement
    // a) look for grub, ensure version
    // b) create a boot config for balena migration
    // c) call grub-reboot to enable boot once to migrate env

    // let install_drive = mig_info.get_installPath().drive;
    let boot_path = mig_info.get_boot_path();
    let root_path = mig_info.get_root_path();

    /*
        let grub_root = if Some(uuid) = root_path.uuid {
            format!("root=UUID={}", uuid)
        } else {
            if let Some(uuid) = root_path.part_uuid {
                format!("root=PARTUUID={}", uuid)
            } else {
                format!("root={}", &root_path.path.to_string_lossy());
            }
        };
    */

    let grub_boot = if boot_path.device == root_path.device {
        PathBuf::from(BOOT_PATH)
    } else {
        if boot_path.mountpoint == Path::new(BOOT_PATH) {
            PathBuf::from(ROOT_PATH)
        } else {
            // TODO: create appropriate path
            panic!("boot partition mus be mounted in /boot for now");
        }
    };

    let part_type = match LabelType::from_device(&boot_path.drive)? {
        LabelType::GPT => "gpt",
        LabelType::DOS => "msdos",
        _ => {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("Invalid partition type for '{}'", boot_path.drive.display()),
            ));
        }
    };

    let part_mod = format!("part_{}", part_type);

    info!(
        "Boot partition type is '{}' is type '{}'",
        boot_path.drive.display(),
        part_mod
    );

    let root_cmd = if let Some(ref uuid) = boot_path.uuid {
        // TODO: try partuuid too ?local setRootA="set root='${GRUB_BOOT_DEV},msdos${ROOT_PART_NO}'"
        format!("search --no-floppy --fs-uuid --set=root {}", uuid)
    } else {
        format!(
            "search --no-floppy --fs-uuid --set=root {},{}{}",
            boot_path.drive.to_string_lossy(),
            part_type,
            boot_path.index
        )
    };

    debug!("root set to '{}", root_cmd);

    let fstype_mod = match boot_path.fs_type.as_str() {
        "ext2" | "ext3" | "ext4" => "ext2",
        "vfat" => "fat",
        _ => {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Cannot determine grub mod for boot fs type '{}'",
                    boot_path.fs_type
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

    linux.push_str(&format!(" rootfstype={} debug", root_path.fs_type));

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
    File::create(GRUB_CONF_PATH)
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to create grub config file '{}'", GRUB_CONF_PATH),
        ))?
        .write(grub_cfg.as_bytes())
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to write to grub config file '{}'", GRUB_CONF_PATH),
        ))?;

    let cmd_res = call_cmd(CHMOD_CMD, &["+x", GRUB_CONF_PATH], true)?;
    if !cmd_res.status.success() {
        return Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!("Failure from '{}': {:?}", CHMOD_CMD, cmd_res),
        ));
    }

    info!("Grub config written to '{}'", GRUB_CONF_PATH);

    // **********************************************************************
    // ** copy new kernel & iniramfs

    let source_path = mig_info.get_kernel_path();
    let kernel_path = path_append(&mig_info.get_boot_path().path, MIG_KERNEL_NAME);
    std::fs::copy(&source_path, &kernel_path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "failed to copy kernel file '{}' to '{}'",
            source_path.display(),
            kernel_path.display()
        ),
    ))?;
    info!(
        "copied kernel: '{}' -> '{}'",
        source_path.display(),
        kernel_path.display()
    );

    call_cmd(CHMOD_CMD, &["+x", &kernel_path.to_string_lossy()], false)?;

    let source_path = mig_info.get_initrd_path();
    let initrd_path = path_append(&mig_info.get_boot_path().path, MIG_INITRD_NAME);
    std::fs::copy(&source_path, &initrd_path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "failed to copy initrd file '{}' to '{}'",
            source_path.display(),
            initrd_path.display()
        ),
    ))?;
    info!(
        "initramfs kernel: '{}' -> '{}'",
        source_path.display(),
        initrd_path.display()
    );

    let grub_path = match whereis(GRUB_UPDT_CMD) {
        Ok(path) => path,
        Err(why) => {
            warn!(
                "The grub rupdate command '{}' could not be found",
                GRUB_UPDT_CMD
            );
            return Err(MigError::from(why.context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to find command {}", GRUB_UPDT_CMD),
            ))));
        }
    };

    let grub_args = [];
    let cmd_res = call(&grub_path, &grub_args, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        "Failed to set up boot configuration'",
    ))?;

    if !cmd_res.status.success() {
        return Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!("Failure from '{}': {:?}", GRUB_UPDT_CMD, cmd_res),
        ));
    }

    let grub_path = match whereis(GRUB_REBOOT_CMD) {
        Ok(path) => path,
        Err(why) => {
            warn!(
                "The grub reboot update command '{}' could not be found",
                GRUB_REBOOT_CMD
            );
            return Err(MigError::from(why.context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to find command {}", GRUB_REBOOT_CMD),
            ))));
        }
    };

    let grub_args = ["balena-migrate"];
    let cmd_res = call(&grub_path, &grub_args, true).context(MigErrCtx::from_remark(
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

pub(crate) fn u_boot_valid(_mig_info: &MigrateInfo) -> Result<bool, MigError> {
    // TODO: ensure valid u-boot setup based on partition layout
    // where are uEnv.txt files or other boot configuration files ?
    // where are kernel files ?
    Ok(true)
}
