use super::MigMode;
use crate::common::{file_digest::HashInfo, MigError, MigErrorKind};
use log::debug;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MODULE: &str = "common::config::balena_config";

use crate::defs::DEFAULT_API_CHECK_TIMEOUT;

// TODO: also store optional bootable flag, partition type and start offset ?
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct PartDump {
    pub blocks: u64,
    pub archive: FileRef,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum PartCheck {
    None,
    Read,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct FileRef {
    pub path: PathBuf,
    pub hash: Option<HashInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum ImageType {
    Flasher(FileRef),
    FileSystems(FSDump),
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApiInfo {
    host: Option<String>,
    port: Option<u16>,
    check: Option<bool>,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BalenaConfig {
    image: Option<ImageType>,
    config: Option<PathBuf>,
    app_name: Option<String>,
    api: Option<ApiInfo>,
    check_vpn: Option<bool>,
    check_timeout: Option<u64>,
}

impl<'a> BalenaConfig {
    pub fn default() -> BalenaConfig {
        BalenaConfig {
            image: None,
            config: None,
            app_name: None,
            api: None,
            check_vpn: None,
            check_timeout: None,
        }
    }

    pub fn check(&self, mig_mode: &MigMode) -> Result<(), MigError> {
        debug!("check: {:?}", self);
        if let MigMode::IMMEDIATE = mig_mode {
            if let None = self.image {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::check: no balena OS image was specified in mode: IMMEDIATE",
                        MODULE
                    ),
                ));
            }

            if let None = self.config {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::check: no config.json was specified in mode: IMMEDIATE",
                        MODULE
                    ),
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

    pub fn get_check_timeout(&self) -> u64 {
        if let Some(timeout) = self.check_timeout {
            timeout
        } else {
            DEFAULT_API_CHECK_TIMEOUT
        }
    }

    pub fn set_image_path(&mut self, image_path: &str) {
        self.image = Some(ImageType::Flasher(FileRef {
            path: PathBuf::from(image_path),
            hash: None,
        }));
    }

    // The following functions can only be safely called after check has succeeded

    pub fn get_image_path(&'a self) -> &'a ImageType {
        if let Some(ref path) = self.image {
            path
        } else {
            panic!("image path is not set");
        }
    }

    pub fn get_config_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.config {
            path
        } else {
            panic!("config path is not set");
        }
    }
}
