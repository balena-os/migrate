use log::{debug, error, info, trace};
use regex::Regex;
use std::path::Path;

use crate::{
    common::{
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType},
    linux::{
        boot_manager::{from_boot_type, BootManager, RaspiBootManager},
        device::Device,
        linux_common::restore_backups,
        EnsuredCmds, MigrateInfo,
    },
};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;

pub(crate) fn is_rpi(
    cmds: &mut EnsuredCmds,
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
                    cmds, dev_info, config, s2_cfg,
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
    pub fn new() -> RaspberryPi3 {
        RaspberryPi3 {
            boot_manager: Box::new(RaspiBootManager::new()),
        }
    }
    pub fn from_config(
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<RaspberryPi3, MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &["Raspbian GNU/Linux 9 (stretch)"];

        let os_name = &mig_info.os_name;

        if let Some(_n) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut boot_manager = RaspiBootManager::new();
            if boot_manager.can_migrate(cmds, mig_info, config, s2_cfg)? {
                Ok(RaspberryPi3::new())
            } else {
                Err(MigError::from(MigErrorKind::Displayed))
            }
        } else {
            let message = format!("The OS '{}' is not supported for RaspberryPi3", os_name,);
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }
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
        cmds: &EnsuredCmds,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        self.boot_manager.setup(cmds, dev_info, config, s2_cfg)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration for Raspberry Pi 3");

        restore_backups(root_path, config.get_boot_backups())?;

        info!("The original boot configuration was restored");

        Ok(())
    }
}
