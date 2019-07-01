use std::path::{Path, PathBuf};

use super::MigMode;
use crate::common::MigError;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct DebugConfig {
    // ignore non admin user
    fake_admin: Option<bool>,
    // flash on this device instead of / device
    force_flash_device: Option<PathBuf>,
    // pretend mode, stop after unmounting former root
    no_flash: Option<bool>,
}

impl<'a> DebugConfig {
    pub fn default() -> DebugConfig {
        DebugConfig {
            fake_admin: None,
            force_flash_device: None,
            // TODO: default to false when project is mature
            no_flash: None,
        }
    }

    #[cfg(debug_assertions)]
    pub fn is_fake_admin(&self) -> bool {
        if let Some(val) = self.fake_admin {
            val
        } else {
            false
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

    pub fn get_force_flash_device(&'a self) -> Option<&'a Path> {
        if let Some(ref val) = self.force_flash_device {
            Some(val)
        } else {
            None
        }
    }

    pub fn check(&self, _mig_mode: &MigMode) -> Result<(), MigError> {
        // TODO: implement
        Ok(())
    }
}
