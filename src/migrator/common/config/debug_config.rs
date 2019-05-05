use super::YamlConfig;
use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_str},
    MigError,
};
use std::path::PathBuf;

use yaml_rust::Yaml;

#[derive(Debug)]
pub struct DebugConfig {
    pub fake_admin: bool,
    pub fake_flash_device: Option<PathBuf>,
}

impl DebugConfig {
    pub fn default() -> DebugConfig {
        DebugConfig {
            fake_admin: false,
            fake_flash_device: None,
        }
    }
}

impl YamlConfig for DebugConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        let output = format!(
            "{}debug:\n{}  fake_admin: {}\n",
            prefix, prefix, self.fake_admin
        );

        if let Some(fake_flash) = self.fake_flash_device {
            output += &format!(
                "{}  fake_flash_device: {}\n",
                prefix,
                &fake_flash.to_string_lossy()
            );
        }
        output
    }

    fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(value) = get_yaml_bool(yaml, &["fake_admin"])? {
            self.fake_admin = value;
        }

        if let Some(value) = get_yaml_str(yaml, &["fake_flash_device"])? {
            self.fake_flash_device = Some(PathBuf::from(value));
        }

        Ok(())
    }
}
