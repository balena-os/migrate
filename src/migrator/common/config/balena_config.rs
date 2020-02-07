use super::MigMode;
use crate::common::{file_digest::HashInfo, MigError, MigErrorKind};
use crate::defs::DEFAULT_API_CHECK_TIMEOUT;
use log::debug;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// TODO: also store optional bootable flag, partition type and start offset ?
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct PartDump {
    pub blocks: u64,
    pub archive: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum PartCheck {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct FSDump {
    pub extended_blocks: u64,
    pub device_slug: String,
    pub check: Option<PartCheck>,
    pub max_data: Option<bool>,
    pub mkfs_direct: Option<bool>,
    pub boot: PartDump,
    pub root_a: PartDump,
    pub root_b: PartDump,
    pub state: PartDump,
    pub data: PartDump,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) struct FileRef {
    pub path: PathBuf,
    pub hash: Option<HashInfo>,
}

#[allow(clippy::large_enum_variant)] //TODO refactor to remove clippy warning
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum ImageType {
    #[serde(rename = "dd")]
    Flasher(PathBuf),
    #[serde(rename = "fs")]
    FileSystems(FSDump),
}

#[derive(Debug, Deserialize)]
pub(crate) struct BalenaConfig {
    image: Option<ImageType>,
    config: Option<PathBuf>,
    app_name: Option<String>,
    check_api: Option<bool>,
    check_vpn: Option<bool>,
    check_timeout: Option<u64>,
}

impl<'a> BalenaConfig {
    pub fn default() -> BalenaConfig {
        BalenaConfig {
            image: None,
            config: None,
            app_name: None,
            check_api: None,
            check_vpn: None,
            check_timeout: None,
        }
    }

    pub fn check(&self, mig_mode: &MigMode) -> Result<(), MigError> {
        debug!("check: {:?}", self);
        if let MigMode::Immediate = mig_mode {
            if self.image.is_none() {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "check: no balena OS image was specified in mode: IMMEDIATE",
                ));
            }

            if self.config.is_none() {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "check: no config.json was specified in mode: IMMEDIATE",
                ));
            }
        }

        Ok(())
    }

    pub fn is_check_vpn(&self) -> bool {
        if let Some(ref check_vpn) = self.check_vpn {
            *check_vpn
        } else {
            true
        }
    }

    pub fn is_check_api(&self) -> bool {
        if let Some(ref check_api) = self.check_api {
            *check_api
        } else {
            true
        }
    }

    pub fn get_check_timeout(&self) -> u64 {
        if let Some(timeout) = self.check_timeout {
            timeout
        } else {
            DEFAULT_API_CHECK_TIMEOUT
        }
    }

    pub fn set_image_path(&mut self, image_path: &str) {
        self.image = Some(ImageType::Flasher(PathBuf::from(image_path)));
    }

    // The following functions can only be safely called after check has succeeded

    pub fn get_image_path(&'a self) -> &'a ImageType {
        if let Some(ref path) = self.image {
            path
        } else {
            panic!("The image path is not set in config");
        }
    }

    pub fn get_config_path(&'a self) -> &'a PathBuf {
        if let Some(ref path) = self.config {
            path
        } else {
            panic!("The balena config.json path is not set in config");
        }
    }
}
