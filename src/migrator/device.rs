use failure::ResultExt;
use log::{debug, error, warn};
use std::fs;
use std::path::Path;

use crate::{
    //linux::LinuxMigrator,
    common::{Config, MigErrCtx, MigError, MigErrorKind, OSArch},
    linux_common::{get_grub_version, MigrateInfo},
    stage2::Stage2Config,
};

mod beaglebone;
mod intel_nuc;
mod raspberrypi;

const MODULE: &str = "device";
const DEVICE_TREE_MODEL: &str = "/proc/device-tree/model";

const GRUB_CFG_TEMPLATE: &str = r##"
#!/bin/sh
exec tail -n +3 \$0
# This file provides an easy way to add custom menu entries.  Simply type the
# menu entries you want to add after this comment.  Be careful not to change
# the 'exec tail' line above.

menuentry "resin-migration" {
  insmod gzio
  insmod __$partMod__
  insmod ext2

  __$setRootA__
  $setRootB
  linux __$KERNEL_CMD_LINE__
  initrd  __${BOOT_DIR}/${INITRAMFS_NAME}__
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
                fs::read_to_string(DEVICE_TREE_MODEL).context(MigErrCtx::from_remark(
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

pub(crate) fn grub_install(
    _config: &Config,
    _mig_info: &mut MigrateInfo,
) -> Result<bool, MigError> {
    // TODO: implement
    // a) look for grub, ensure version
    // b) create a boot config for balena migration
    // c) call grub-reboot to enable boot once to migrate env

    Err(MigError::from(MigErrorKind::NotImpl))
}

pub(crate) fn u_boot_valid() -> Result<bool, MigError> {
    // TODO: ensure valid u-boot setup based on partition layout
    Err(MigError::from(MigErrorKind::NotImpl))
}
