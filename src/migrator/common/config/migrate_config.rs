use yaml_rust::{Yaml};
use super::{YamlConfig, LogConfig, get_yaml_val, get_yaml_str, get_yaml_int, get_yaml_bool};
use crate::migrator::{MigError, MigErrorKind};

const MODULE: &str = "common::config::migrate_config"; 

#[derive(Debug)]
pub enum MigMode {
    INVALID,
    AGENT,
    IMMEDIATE,
    PRETEND,
}

const DEFAULT_MODE: MigMode = MigMode::INVALID;


#[derive(Debug)]
pub struct MigrateConfig {
    pub home_dir: String,
    pub mode: MigMode,
    pub reboot: Option<u64>,
    pub all_wifis: bool,
    pub log_to: Option<LogConfig>,
    pub kernel_file: String,
    pub initramfs_file: String,
} 

impl MigrateConfig {
    pub fn default() -> MigrateConfig {
        MigrateConfig {
            home_dir: String::from("."),
            mode: DEFAULT_MODE,
            reboot: None,
            all_wifis: false,
            log_to: None,
            kernel_file: String::from(""),
            initramfs_file: String::from(""),
        }
    }
}

impl YamlConfig for MigrateConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = format!("{}migrate:\n{}  home_dir: '{}'\n{}  mode: '{:?}'\n{}  all_wifis: {}\n", prefix, prefix, self.home_dir, prefix, self.mode, prefix, self.all_wifis);
        if let Some(i) = self.reboot {
            output += &format!("{}  reboot: {}\n", prefix, i);
        }

        if self.kernel_file.is_empty() == false {
            output += &format!("{}  kernel_file: {}\n", prefix, self.kernel_file);
        }

        if self.initramfs_file.is_empty() == false {
            output += &format!("{}  initramfs_file: {}\n", prefix, self.initramfs_file);
        }

        let next_prefix = String::from(prefix) + "  ";        
        if let Some(ref log_to) = self.log_to {
            output += &log_to.to_yaml(&next_prefix);
        }

        output
    }

    fn from_yaml(&mut self, yaml: & Yaml) -> Result<(),MigError> {

        if let Some(home_dir) = get_yaml_str(yaml, &["home_dir"])? {
            self.home_dir = String::from(home_dir);
        }

        if let Some(kernel_file) = get_yaml_str(yaml, &["kernel_file"])? {
            self.kernel_file = String::from(kernel_file);
        }

        if let Some(initramfs_file) = get_yaml_str(yaml, &["initramfs_file"])? {
            self.initramfs_file = String::from(initramfs_file);
        }

        if let Some(mode) = get_yaml_str(yaml, &["mode"])? {
            if mode.to_lowercase() == "immediate" {
                self.mode = MigMode::IMMEDIATE;
            } else if mode.to_lowercase() == "agent" {
                self.mode = MigMode::AGENT;
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::from_string: invalid value for migrate mode '{}'", MODULE, mode)));
            }            
        }

        // Param: reboot - must be > 0 
        if let Some(reboot_timeout) = get_yaml_int(yaml, &["reboot"])? {
            if reboot_timeout > 0 {
                self.reboot = Some(reboot_timeout as u64);      
            } else {
                self.reboot = None;      
            }
        }

        // Param: all_wifis - must be > 0 
        if let Some(all_wifis) = get_yaml_bool(yaml, &["all_wifis"])? {
            self.all_wifis = all_wifis;      
        }

        // Params: log_to: drive, fs_type 
        if let Some(log_section) = get_yaml_val(yaml, &["log_to"])? {
            if let Some(ref mut log_to) = self.log_to {
                log_to.from_yaml(yaml)?;
            } else {
                let mut log_to = LogConfig::default();
                log_to.from_yaml(log_section)?;
                self.log_to = Some(log_to);
            }
        }
        
        Ok(())
    }
}
