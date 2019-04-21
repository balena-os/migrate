use yaml_rust::{Yaml};
use super::{
    YamlConfig, 
    get_yaml_str, 
    get_yaml_int, 
    get_yaml_bool, 
    MigMode
    };
use crate::migrator::{MigError, MigErrorKind};

const MODULE: &str = "common::config::balena_config"; 

const DEFAULT_API_HOST: &str = "api.balena-cloud.com";
const DEFAULT_API_PORT: u16 = 443;
const DEFAULT_VPN_HOST: &str = "vpn.balena-cloud.com";
const DEFAULT_VPN_PORT: u16 = 443;
const DEFAULT_CHECK_TIMEOUT: u64 = 20;


#[derive(Debug)]
pub struct BalenaConfig {
    pub image: String,
    pub config: String,
    pub api_host: String,
    pub api_port: u16,
    pub api_check: bool,
    pub vpn_host: String,
    pub vpn_port: u16,
    pub vpn_check: bool,
    pub check_timeout: u64,    
} 

impl BalenaConfig {
    pub(crate) fn default() -> BalenaConfig {
        BalenaConfig{
            image: String::from(""),
            config: String::from(""),
            api_host: String::from(DEFAULT_API_HOST),
            api_port: DEFAULT_API_PORT,
            api_check: true,
            vpn_host: String::from(DEFAULT_VPN_HOST),
            vpn_port: DEFAULT_VPN_PORT,
            vpn_check: true,
            check_timeout: DEFAULT_CHECK_TIMEOUT,
        }
    }

    pub(crate) fn check(&self, mig_mode: &MigMode) -> Result<(),MigError> {
        if let MigMode::IMMEDIATE = mig_mode {
            if self.image.is_empty() {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no balena OS image was specified in mode: IMMEDIATE", MODULE)));
            }                

            if self.config.is_empty() {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no config.json was specified in mode: IMMEDIATE", MODULE)));
            }  
        }

        Ok(())
    }

}

impl YamlConfig for BalenaConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = format!("{}balena:\n{}  image: '{}'\n{}  config: '{}'\n", prefix, prefix, self.image, prefix , self.config);
        output += &format!("{}  api:\n{}    host: '{}'\n{}    port: {}\n{}    check: {}\n", prefix, prefix, self.api_host, prefix , self.api_port, prefix, self.api_check);
        output += &format!("{}  vpn:\n{}    host: '{}'\n{}    port: {}\n{}    check: {}\n", prefix, prefix, self.vpn_host, prefix , self.vpn_port, prefix, self.vpn_check);
        output += &format!("{}  check_timeout: {}\n", prefix, self.check_timeout);
        output
    }
    fn from_yaml(&mut self, yaml: & Yaml) -> Result<(),MigError> {        
        if let Some(balena_image) = get_yaml_str(yaml, &["image"])? {
            self.image = String::from(balena_image);
        }

        // Params: balena_config 
        if let Some(balena_config) = get_yaml_str(yaml, &["config"])? {
            self.config = String::from(balena_config);                
        }


        if let Some(api_host) = get_yaml_str(yaml, &["api","host"])? {
            self.api_host = String::from(api_host);
            if let Some(api_port) = get_yaml_int(yaml, &["api","port"])? {
                if api_port > 0 && api_port <= 0xFFFF {
                    self.api_port = api_port as u16;
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::from_yaml: invalid alue for port: {}", MODULE, api_port)));
                }
            }
            if let Some(api_check) = get_yaml_bool(yaml, &["api","check"])? {
                self.api_check = api_check;
            }
        }

        if let Some(vpn_host) = get_yaml_str(yaml, &["vpn","host"])? {
            self.vpn_host = String::from(vpn_host);
            if let Some(vpn_port) = get_yaml_int(yaml, &["vpn","port"])? {
                if vpn_port > 0 && vpn_port <= 0xFFFF {
                    self.vpn_port = vpn_port as u16;
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::from_yaml: invalid alue for port: {}", MODULE, vpn_port)));
                }
            }
            if let Some(vpn_check) = get_yaml_bool(yaml, &["vpn","check"])? {
                self.vpn_check = vpn_check;
            }
        }

        if let Some(check_timeout) = get_yaml_int(yaml, &["check_timeout"])? {
            self.check_timeout = check_timeout as u64;
        }

        Ok(())
    }
}
