use log::{trace, warn, error};
use regex::{Regex};

use crate::common::{MigError, MigErrorKind};
use crate::linux_common::{DeviceStage1, MigrateInfo};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;

pub(crate) fn is_rpi(model_string: &str) -> Result<Box<DeviceStage1>, MigError> {
    trace!(
        "Beaglebone::is_bb: entered with model string: '{}'",
        model_string
    );

/*        
            .unwrap()
            .captures(&dev_tree_model)
        {
            return Ok(self.init_rpi(
                captures.get(1).unwrap().as_str(),
                captures
                    .get(2)
                    .unwrap()
                    .as_str()
                    .trim_matches(char::from(0)),
            )?);
        }
*/

    if let Some(captures) = Regex::new(RPI_MODEL_REGEX)
            .unwrap()
            .captures(model_string) {            

        let model = captures
                        .get(2)
                        .unwrap()
                        .as_str()
                        .trim_matches(char::from(0));

        match model {
            "3" => Ok(Box::new(RaspberryPi3{})),
            _ => {
                let message = format!("The beaglebone model reported by your device ('{}') is not supported by balena-migrate", model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        warn!("no match for beaglebone on: {}", model_string);
        Err(MigError::from(MigErrorKind::NoMatch))
    }
}


struct RaspberryPi3 {}

impl<'a> DeviceStage1 for RaspberryPi3 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi-3"
    }

    fn setup(&self, mig_info: &mut MigrateInfo) -> Result<(),MigError> {
        trace!(
            "RaspberryPi3::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
