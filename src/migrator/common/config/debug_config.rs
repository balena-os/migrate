use std::path::PathBuf;

use super::MigMode;
use crate::common::MigError;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct DebugConfig {
    // flash on this device instead of / device
    force_flash_device: Option<PathBuf>,
    // pretend mode, stop after unmounting former root
    no_flash: Option<bool>,
}

impl<'a> DebugConfig {
    pub fn default() -> DebugConfig {
        DebugConfig {
            force_flash_device: None,
            // TODO: default to false when project is mature
            no_flash: None,
        }
    }

    pub fn is_no_flash(&self) -> bool {
        if let Some(val) = self.no_flash {
            val
        } else {
            // TODO: change to false when mature
            true
        }
    }

    pub fn check(&self, _mig_mode: &MigMode) -> Result<(), MigError> {
        // TODO: implement
        Ok(())
    }
}
