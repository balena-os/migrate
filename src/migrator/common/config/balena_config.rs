use super::{MigMode};
use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_int, get_yaml_str},
    MigError, MigErrorKind,
};
use std::path::{Path, PathBuf};
use serde::{Deserialize};

use yaml_rust::Yaml;

const MODULE: &str = "common::config::balena_config";

const DEFAULT_API_HOST: &str = "api.balena-cloud.com";
const DEFAULT_API_PORT: u16 = 443;
const DEFAULT_VPN_HOST: &str = "vpn.balena-cloud.com";
const DEFAULT_VPN_PORT: u16 = 443;
const DEFAULT_CHECK_TIMEOUT: u64 = 20;

#[derive(Debug,Deserialize)]
pub struct BalenaConfig {
    pub image: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub api_host: String,
    pub api_port: u16,
    pub api_check: bool,
    pub vpn_host: String,
    pub vpn_port: u16,
    pub vpn_check: bool,
    pub check_timeout: u64,
}

impl<'a> BalenaConfig {
    pub(crate) fn default() -> BalenaConfig {
        BalenaConfig {
            image: None,
            config: None,
            api_host: String::from(DEFAULT_API_HOST),
            api_port: DEFAULT_API_PORT,
            api_check: true,
            vpn_host: String::from(DEFAULT_VPN_HOST),
            vpn_port: DEFAULT_VPN_PORT,
            vpn_check: true,
            check_timeout: DEFAULT_CHECK_TIMEOUT,
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

    pub(crate) fn get_image_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.image {
            path
        } else {
            panic!("image path is not set");
        }
    }

    pub(crate) fn get_config_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.config {
            path
        } else {
            panic!("config path is not set");
        }
    }
}
