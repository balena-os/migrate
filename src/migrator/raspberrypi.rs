use log::{error, info, trace, warn};
use regex::Regex;
use std::path::Path;

use crate::{
    common::{Config, MigError, MigErrorKind},
    linux_common::{Device, MigrateInfo},
    stage2::Stage2Config,
};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;

pub(crate) fn is_rpi(model_string: &str) -> Result<Box<Device>, MigError> {
    trace!(
        "Beaglebone::is_bb: entered with model string: '{}'",
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
                Ok(Box::new(RaspberryPi3 {}))
            }
            _ => {
                let message = format!("The raspberry pi type reported by your device ('{} {}') is not supported by balena-migrate", pitype, model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        warn!("no match for beaglebone on: {}", model_string);
        Err(MigError::from(MigErrorKind::NoMatch))
    }
}

pub(crate) struct RaspberryPi3 {}

impl RaspberryPi3 {
    pub(crate) fn new() -> RaspberryPi3 {
        RaspberryPi3 {}
    }
}

impl<'a> Device for RaspberryPi3 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi3"
    }

    fn setup(&self, _config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "RaspberryPi3::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn restore_boot(&self, _root_path: &Path, _config: &Stage2Config) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
