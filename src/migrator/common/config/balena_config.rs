use super::{MigMode, YamlConfig};
use crate::common::{
    config_helper::{get_yaml_bool, get_yaml_int, get_yaml_str},
    MigError, MigErrorKind,
};
use std::path::{Path, PathBuf};

use yaml_rust::Yaml;

const MODULE: &str = "common::config::balena_config";

const DEFAULT_API_HOST: &str = "api.balena-cloud.com";
const DEFAULT_API_PORT: u16 = 443;
const DEFAULT_VPN_HOST: &str = "vpn.balena-cloud.com";
const DEFAULT_VPN_PORT: u16 = 443;
const DEFAULT_CHECK_TIMEOUT: u64 = 20;

#[derive(Debug)]
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

impl YamlConfig for BalenaConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<BalenaConfig>, MigError> {
        let mut config = BalenaConfig::default();
        if let Some(balena_image) = get_yaml_str(yaml, &["image"])? {
            config.image = Some(PathBuf::from(balena_image));
        }

        // Params: balena_config
        if let Some(balena_config) = get_yaml_str(yaml, &["config"])? {
            config.config = Some(PathBuf::from(balena_config));
        }

        if let Some(api_host) = get_yaml_str(yaml, &["api", "host"])? {
            config.api_host = String::from(api_host);
            if let Some(api_port) = get_yaml_int(yaml, &["api", "port"])? {
                if api_port > 0 && api_port <= 0xFFFF {
                    config.api_port = api_port as u16;
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("{}::from_yaml: invalid alue for port: {}", MODULE, api_port),
                    ));
                }
            }
            if let Some(api_check) = get_yaml_bool(yaml, &["api", "check"])? {
                config.api_check = api_check;
            }
        }

        if let Some(vpn_host) = get_yaml_str(yaml, &["vpn", "host"])? {
            config.vpn_host = String::from(vpn_host);
            if let Some(vpn_port) = get_yaml_int(yaml, &["vpn", "port"])? {
                if vpn_port > 0 && vpn_port <= 0xFFFF {
                    config.vpn_port = vpn_port as u16;
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("{}::from_yaml: invalid alue for port: {}", MODULE, vpn_port),
                    ));
                }
            }
            if let Some(vpn_check) = get_yaml_bool(yaml, &["vpn", "check"])? {
                config.vpn_check = vpn_check;
            }
        }

        if let Some(check_timeout) = get_yaml_int(yaml, &["check_timeout"])? {
            config.check_timeout = check_timeout as u64;
        }

        Ok(Box::new(config))
    }

    /*
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = format!("{}balena:\n", prefix);

        if let Some(ref image) = self.image {
            output += &format!("{}  image: '{}'\n", prefix, &image.to_string_lossy());
        }

        if let Some(ref config) = self.config {
            output += &format!("{}  config: '{}'\n", prefix, &config.to_string_lossy());
        }

        output += &format!(
            "{}  api:\n{}    host: '{}'\n{}    port: {}\n{}    check: {}\n",
            prefix, prefix, self.api_host, prefix, self.api_port, prefix, self.api_check
        );
        output += &format!(
            "{}  vpn:\n{}    host: '{}'\n{}    port: {}\n{}    check: {}\n",
            prefix, prefix, self.vpn_host, prefix, self.vpn_port, prefix, self.vpn_check
        );
        output += &format!("{}  check_timeout: {}\n", prefix, self.check_timeout);
        output
    }


    */
}
