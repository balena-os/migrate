use log::{debug, error, info, trace};
use regex::Regex;
use std::path::Path;

use crate::{
    boot_manager::{from_boot_type, BootManager, BootType, RaspiBootManager},
    common::{Config, MigError, MigErrorKind},
    device::{Device, DeviceType},
    linux_common::{migrate_info::MigrateInfo, restore_backups},
    stage2::stage2_config::{Stage2Config, Stage2ConfigBuilder},
};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;

pub(crate) fn is_rpi(
    dev_info: &MigrateInfo,
    config: &Config,
    s2_cfg: &mut Stage2ConfigBuilder,
    model_string: &str,
) -> Result<Option<Box<Device>>, MigError> {
    trace!(
        "raspberrypi::is_rpi: entered with model string: '{}'",
        model_string
    );

    if let Some(captures) = Regex::new(RPI_MODEL_REGEX).unwrap().captures(model_string) {
        let pitype = captures.get(1).unwrap().as_str();
        let model = captures
            .get(2)
            .unwrap()
            .as_str()
            .trim_matches(char::from(0));

        match pitype {
            "3" => {
                info!("Identified RaspberryPi3: model {}", model);
                Ok(Some(Box::new(RaspberryPi3::from_config(
                    dev_info, config, s2_cfg,
                )?)))
            }
            _ => {
                let message = format!("The raspberry pi type reported by your device ('{} {}') is not supported by balena-migrate", pitype, model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        debug!("no match for Raspberry PI on: {}", model_string);
        Ok(None)
    }
}

pub(crate) struct RaspberryPi3 {
    boot_manager: Box<BootManager>,
}

impl RaspberryPi3 {
    pub fn from_config(
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<RaspberryPi3, MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &["Raspbian GNU/Linux 9 (stretch)"];

        let os_name = &dev_info.os_name;

        if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let message = format!("The OS '{}' is not supported for RaspberryPi3", os_name,);
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        Ok(RaspberryPi3 {
            boot_manager: Box::new(RaspiBootManager {}),
        })
    }

    pub fn from_boot_type(boot_type: &BootType) -> RaspberryPi3 {
        RaspberryPi3 {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl<'a> Device for RaspberryPi3 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi3"
    }

    fn get_device_type(&self) -> DeviceType {
        DeviceType::RaspberryPi3
    }

    fn get_boot_type(&self) -> BootType {
        self.boot_manager.get_boot_type()
    }

    fn setup(
        &self,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        self.boot_manager.setup(dev_info, config, s2_cfg)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration for Raspberry Pi 3");

        restore_backups(root_path, config.get_boot_backups())?;

        info!("The original boot configuration was restored");

        Ok(())
    }
}
