use failure::ResultExt;
use log::error;
use std::fs::read_to_string;

use crate::{
    common::{
        device::Device, migrate_info::MigrateInfo, stage2_config::Stage2ConfigBuilder, Config,
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType, OSArch},
};

mod beaglebone;
mod intel_nuc;
mod raspberrypi;

const DEVICE_TREE_MODEL: &str = "/proc/device-tree/model";

pub(crate) fn from_config(
    device_type: DeviceType,
    boot_type: BootType,
) -> Result<Box<dyn Device>, MigError> {
    match device_type {
        DeviceType::BeagleboneGreen => Ok(Box::new(beaglebone::BeagleboneGreen::from_boot_type(
            boot_type,
        ))),
        /*        DeviceType::BeagleboneBlack => Ok(Box::new(beaglebone::BeagleboneBlack::from_boot_type(
            boot_type,
        ))), */
        DeviceType::BeagleboardXM => Ok(Box::new(beaglebone::BeagleboardXM::from_boot_type(
            boot_type,
        ))),
        DeviceType::RaspberryPi2 => Ok(Box::new(raspberrypi::RaspberryPi2::from_boot_type(
            boot_type,
        ))),
        DeviceType::RaspberryPi3 => Ok(Box::new(raspberrypi::RaspberryPi3::from_boot_type(
            boot_type,
        ))),
        DeviceType::RaspberryPi4_64 => Ok(Box::new(raspberrypi::RaspberryPi4_64::from_boot_type(
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
    mig_info: &MigrateInfo,
    config: &Config,
    s2_cfg: &mut Stage2ConfigBuilder,
) -> Result<Box<dyn Device>, MigError> {
    match mig_info.os_arch {
        OSArch::ARMHF => {
            let dev_tree_model = String::from(
                read_to_string(DEVICE_TREE_MODEL)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "get_device: unable to determine model due to inaccessible file '{}'",
                            DEVICE_TREE_MODEL
                        ),
                    ))?
                    .trim_end_matches('\0')
                    .trim_end(),
            );

            if let Some(device) = raspberrypi::is_rpi(mig_info, config, s2_cfg, &dev_tree_model)? {
                return Ok(device);
            }

            if let Some(device) = beaglebone::is_bb(mig_info, config, s2_cfg, &dev_tree_model)? {
                return Ok(device);
            }

            let message = format!(
                "Your device type: '{}' is not supported by balena-migrate.",
                dev_tree_model
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
        OSArch::AMD64 => Ok(Box::new(intel_nuc::IntelNuc::from_config(
            mig_info, config, s2_cfg,
        )?)),
        /*            OSArch::I386 => {
                    migrator.init_i386()?;
                },
        */
        _ => Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "get_device: unexpected OsArch encountered: {}",
                mig_info.os_arch
            ),
        )),
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
