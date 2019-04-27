use super::YamlConfig;
use crate::migrator::common::config_helper::get_yaml_bool;
use crate::migrator::MigError;
use yaml_rust::Yaml;

#[derive(Debug)]
pub struct DebugConfig {
    pub fake_admin: bool,
}

impl DebugConfig {
    pub fn default() -> DebugConfig {
        DebugConfig { fake_admin: false }
    }
}

impl YamlConfig for DebugConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        format!(
            "{}debug:\n{}  fake_admin: {}\n",
            prefix, prefix, self.fake_admin
        )
    }
    fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(value) = get_yaml_bool(yaml, &["fake_admin"])? {
            self.fake_admin = value;
        }

        Ok(())
    }
}
