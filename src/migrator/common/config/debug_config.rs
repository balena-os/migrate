use log::debug;

use super::{MigMode, YamlConfig};
use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_str},
    MigError,
};
use std::path::PathBuf;

use yaml_rust::Yaml;

#[derive(Debug)]
pub struct DebugConfig {
    // ignore non admin user
    pub fake_admin: bool,
    // flash on this device instead of / device
    pub force_flash_device: Option<PathBuf>,
    // skip the flashing (only makes sense with force_flash_device)
    pub skip_flash: bool,
    // pretend mode, stop after unmounting former root
    pub no_flash: bool,
}

impl DebugConfig {
    pub fn default() -> DebugConfig {
        DebugConfig {
            fake_admin: false,
            force_flash_device: None,
            skip_flash: false,
            // TODO: default to false when project is mature
            no_flash: true,
        }
    }

    pub fn check(&self, mig_mode: &MigMode) -> Result<(), MigError> {
        // TODO: implement
        Ok(())
    }
}

impl YamlConfig for DebugConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<DebugConfig>, MigError> {
        let mut config = DebugConfig::default();
        if let Some(value) = get_yaml_bool(yaml, &["fake_admin"])? {
            debug!("fake_admin: {}", value);
            config.fake_admin = value;
        }

        if let Some(value) = get_yaml_str(yaml, &["force_flash_device"])? {
            debug!("force_flash_device: {}", value);
            config.force_flash_device = Some(PathBuf::from(value));

            if let Some(value) = get_yaml_bool(yaml, &["skip_flash"])? {
                debug!("skip_flash: {}", value);
                config.skip_flash = value;
            }
        }

        if let Some(no_flash) = get_yaml_bool(yaml, &["no_flash"])? {
            debug!("no_flash: {}", no_flash);
            config.no_flash = no_flash;
        }

        Ok(Box::new(config))
    }
/*
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = format!(
            "{}debug:\n{}  fake_admin: {}\n{}  no_flash: {}\n",
            prefix, prefix, self.fake_admin, prefix, self.no_flash
        );

        if let Some(ref force_flash) = self.force_flash_device {
            output += &format!(
                "{}  force_flash_device: {}\n",
                prefix,
                &force_flash.to_string_lossy()
            );
        }
        output
    }
*/

}
