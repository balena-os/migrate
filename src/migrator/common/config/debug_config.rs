use log::debug;

use super::{MigMode};
use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_str},
    MigError,
};
use std::path::PathBuf;

use yaml_rust::Yaml;
use serde::{Deserialize};

#[derive(Debug,Deserialize)]
pub(crate) struct DebugConfig {
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
