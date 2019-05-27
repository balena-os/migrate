use failure::{Fail, ResultExt};
use log::error;
use serde::{Deserialize, Serialize};
use std::fs::read_to_string;
use std::path::Path;

use crate::{
    boot_manager::BootType,
    common::{Config, MigErrCtx, MigError, MigErrorKind, OSArch},
    //linux::LinuxMigrator,
    linux_common::migrate_info::MigrateInfo,
    stage2::stage2_config::{Stage2Config, Stage2ConfigBuilder},
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum DeviceType {
    BeagleboneGreen,
    BeagleboneBlack,
    BeagleboardXM,
    IntelNuc,
    RaspberryPi3,
}

pub(crate) trait Device {
    fn get_device_slug(&self) -> &'static str;
    fn get_device_type(&self) -> DeviceType;
    fn get_boot_type(&self) -> BootType;
    fn setup(
        &self,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError>;
    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError>;
}

pub(crate) fn from_config(
    device_type: &DeviceType,
    boot_type: &BootType,
) -> Result<Box<Device>, MigError> {
    match device_type {
        DeviceType::BeagleboneGreen => Ok(Box::new(beaglebone::BeagleboneGreen::from_boot_type(
            boot_type,
        ))),
        DeviceType::BeagleboneBlack => Ok(Box::new(beaglebone::BeagleboneBlack::from_boot_type(
            boot_type,
        ))),
        DeviceType::BeagleboardXM => Ok(Box::new(beaglebone::BeagleboardXM::from_boot_type(
            boot_type,
        ))),
        DeviceType::RaspberryPi3 => Ok(Box::new(raspberrypi::RaspberryPi3::from_boot_type(
            boot_type,
        ))),
        DeviceType::IntelNuc => Ok(Box::new(intel_nuc::IntelNuc::from_boot_type(boot_type))),
        /*        _ => {
                    let message = format!("unexpected device type: {}", &slug);
                    error!("{}", &message);
                    Err(MigError::from_remark(MigErrorKind::InvState, &message))
                }
        */
    }
}

pub(crate) fn get_device(
    dev_info: &MigrateInfo,
    config: &Config,
    s2_cfg: &mut Stage2ConfigBuilder,
) -> Result<Box<Device>, MigError> {
    match dev_info.os_arch {
        OSArch::ARMHF => {
            let dev_tree_model =
                read_to_string(DEVICE_TREE_MODEL).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "get_device: unable to determine model due to inaccessible file '{}'",
                        DEVICE_TREE_MODEL
                    ),
                ))?;

            if let Some(device) = raspberrypi::is_rpi(dev_info, config, s2_cfg, &dev_tree_model)? {
                return Ok(device);
            }

            if let Some(device) = beaglebone::is_bb(dev_info, config, s2_cfg, &dev_tree_model)? {
                return Ok(device);
            }

            let message = format!(
                "Your device type: '{}' is not supported by balena-migrate.",
                dev_tree_model
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
        OSArch::AMD64 => {
            return Ok(Box::new(intel_nuc::IntelNuc::from_config(
                dev_info, config, s2_cfg,
            )?))
        }
        /*            OSArch::I386 => {
                    migrator.init_i386()?;
                },
        */
        _ => {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "get_device: unexpected OsArch encountered: {}",
                    dev_info.os_arch
                ),
            ));
        }
    }
}

/*
pub(crate) fn u_boot_valid(_mig_info: &MigrateInfo) -> Result<bool, MigError> {
    // TODO: ensure valid u-boot setup based on partition layout
    // where are uEnv.txt files or other boot configuration files ?
    // where are kernel files ?
    Ok(true)
}
*/
