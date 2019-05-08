use super::MigMode;
use crate::common::{MigError, MigErrorKind};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const MODULE: &str = "common::config::balena_config";

const DEFAULT_API_HOST: &str = "api.balena-cloud.com";
const DEFAULT_API_PORT: u16 = 443;
const DEFAULT_VPN_HOST: &str = "vpn.balena-cloud.com";
const DEFAULT_VPN_PORT: u16 = 443;
const DEFAULT_CHECK_TIMEOUT: u64 = 20;

#[derive(Debug, Deserialize)]
pub struct BalenaConfig {
    image: Option<PathBuf>,
    config: Option<PathBuf>,
    api_host: Option<String>,
    api_port: Option<u16>,
    api_check: Option<bool>,
    vpn_host: Option<String>,
    vpn_port: Option<u16>,
    vpn_check: Option<bool>,
    check_timeout: Option<u64>,
}

impl<'a> BalenaConfig {
    pub(crate) fn default() -> BalenaConfig {
        BalenaConfig {
            image: None,
            config: None,
            api_host: None,
            api_port: None,
            api_check: None,
            vpn_host: None,
            vpn_port: None,
            vpn_check: None,
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

    pub fn get_api_host(&'a self) -> &'a str {
        if let Some(ref api_host) = self.api_host {
            api_host
        } else {
            DEFAULT_API_HOST
        }
    }

    pub fn get_api_port(&self) -> u16 {
        if let Some(api_port) = self.api_port {
            api_port
        } else {
            DEFAULT_API_PORT
        }
    }

    pub fn is_api_check(&self) -> bool {
        if let Some(api_check) = self.api_check {
            api_check
        } else {
            true
        }
    }

    pub fn get_vpn_host(&'a self) -> &'a str {
        if let Some(ref vpn_host) = self.vpn_host {
            vpn_host
        } else {
            DEFAULT_VPN_HOST
        }
    }

    pub fn get_vpn_port(&self) -> u16 {
        if let Some(vpn_port) = self.vpn_port {
            vpn_port
        } else {
            DEFAULT_VPN_PORT
        }
    }

    pub fn is_check_vpn(&self) -> bool {
        if let Some(vpn_check) = self.vpn_check {
            vpn_check
        } else {
            true
        }
    }

    pub fn get_check_timeout(&self) -> u64 {
        if let Some(timeout) = self.check_timeout {
            timeout
        } else {
            DEFAULT_CHECK_TIMEOUT
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
