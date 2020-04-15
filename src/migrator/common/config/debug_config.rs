use std::path::{Path, PathBuf};

use super::MigMode;
use crate::common::MigError;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct DebugConfig {
    // flash on this device instead of / device
    force_flash_device: Option<PathBuf>,
    // pretend mode, stop after unmounting former root
    no_flash: Option<bool>,
    // free form debug parameters, eg. dump-efi
    hacks: Option<Vec<String>>,

    gzip_internal: Option<bool>,
}

impl<'a> DebugConfig {
    pub fn default() -> DebugConfig {
        DebugConfig {
            force_flash_device: None,
            no_flash: None,
            hacks: None,
            gzip_internal: None,
        }
    }

    pub fn set_no_flash(&mut self, no_flash: bool) {
        self.no_flash = Some(no_flash);
    }

    pub fn is_no_flash(&self) -> bool {
        if let Some(val) = self.no_flash {
            val
        } else {
            // TODO: change to false when mature
            false
        }
    }

    pub fn is_gzip_internal(&self) -> bool {
        if let Some(val) = self.gzip_internal {
            val
        } else {
            true
        }
    }

    pub fn get_hacks(&'a self) -> Option<&'a Vec<String>> {
        if let Some(ref val) = self.hacks {
            Some(val)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn get_hack(&'a self, param: &str) -> Option<&'a String> {
        if let Some(ref hacks) = self.hacks {
            if let Some(hack) = hacks
                .iter()
                .find(|hack| (hack.as_str() == param) || hack.starts_with(&format!("{}:", param)))
            {
                Some(hack)
            } else {
                None
            }
        } else {
            None
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
        Ok(())
    }
}
