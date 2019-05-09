/*
use log::debug;

use crate::common::{
    MigError,
};
use std::path::PathBuf;
*/

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ItemConfig {
    source: String,
    target: Option<String>,
    filter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VolumeConfig {
    name: String,
    items: Vec<ItemConfig>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BackupConfig {
    volumes: Vec<VolumeConfig>,
}

impl<'a> BackupConfig {
    pub(crate) fn default() -> BackupConfig {
        BackupConfig {
            volumes: Vec::new(),
        }
    }
}
