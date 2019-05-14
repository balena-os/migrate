use super::MigMode;
use crate::common::{MigError, MigErrorKind};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const MODULE: &str = "common::config::balena_config";

use crate::defs::{DEFAULT_API_CHECK_TIMEOUT, DEFAULT_API_HOST, DEFAULT_API_PORT};

/*
#[derive(Debug, Deserialize)]
pub struct Host {
    host: Option<String>,
    port: Option<u16>,
    check: Option<bool>,
}
*/

#[derive(Debug, Deserialize)]
pub struct ApiInfo {
    host: Option<String>,
    port: Option<u16>,
    check: Option<bool>,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BalenaConfig {
    image: Option<PathBuf>,
    config: Option<PathBuf>,
    app_name: Option<String>,
    api: Option<ApiInfo>,
    check_vpn: Option<bool>,
    check_timeout: Option<u64>,
}

impl<'a> BalenaConfig {
    pub(crate) fn default() -> BalenaConfig {
        BalenaConfig {
            image: None,
            config: None,
            app_name: None,
            api: None,
            check_vpn: None,
            check_timeout: None,
        }
    }

    pub(crate) fn check(&self, mig_mode: &MigMode) -> Result<(), MigError> {
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

    pub fn get_app_name(&'a self) -> Option<&'a str> {
        if let Some(ref val) = self.app_name {
            Some(val)
        } else {
            None
        }
    }

    pub fn get_api_host(&'a self) -> &'a str {
        if let Some(ref api) = self.api {
            if let Some(ref val) = api.host {
                return val;
            }
        }

        return DEFAULT_API_HOST;
    }

    pub fn get_api_port(&self) -> u16 {
        if let Some(ref api) = self.api {
            if let Some(ref val) = api.port {
                return *val;
            }
        }

        return DEFAULT_API_PORT;
    }

    pub fn is_api_check(&self) -> bool {
        if let Some(ref api) = self.api {
            if let Some(ref val) = api.check {
                return *val;
            }
        }

        return true;
    }

    pub fn get_api_key(&self) -> Option<String> {
        if let Some(ref api) = self.api {
            if let Some(ref val) = api.key {
                return Some(val.clone());
            }
        }

        return None;
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

    // The following functions can only be safely called after check has succeeded

    pub fn get_image_path(&'a self) -> &'a Path {
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
