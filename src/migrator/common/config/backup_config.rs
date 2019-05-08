use log::debug;

use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_str},
    MigError,
};
use std::path::PathBuf;

use yaml_rust::Yaml;
use serde::{Deserialize};

#[derive(Debug,Deserialize)]
pub(crate) struct ItemConfig {

}


#[derive(Debug,Deserialize)]
pub(crate) struct VolumeConfig {
    name: String,
    items: Vec<ItemConfig>,
}

#[derive(Debug,Deserialize)]
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
