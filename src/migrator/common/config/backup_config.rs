use log::debug;

use super::YamlConfig;
use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_str},
    MigError,
};
use std::path::PathBuf;

use yaml_rust::Yaml;

#[derive(Debug)]
pub(crate) struct ItemConfig {

}

impl YamlConfig for ItemConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<ItemConfig>, MigError> {
        Ok(Box::new(ItemConfig{}))
    }
}


#[derive(Debug)]
pub(crate) struct VolumeConfig {
    name: String,
    items: Vec<ItemConfig>,
}

impl YamlConfig for VolumeConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<VolumeConfig>, MigError> {
        Ok(Box::new(VolumeConfig{
            name: String::from("dummy"),
            items: Vec::new(),
        }))
    }
}

#[derive(Debug)]
pub(crate) struct BackupConfig {
    volumes: Vec<VolumeConfig>,
}

impl<'a> BackupConfig {
    pub(crate) fn default() -> BackupConfig {
        BackupConfig{
            volumes: Vec::new(),
        }
    }
}

impl YamlConfig for BackupConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<BackupConfig>, MigError> {
        Ok(Box::new(BackupConfig::default()))
    }
}
