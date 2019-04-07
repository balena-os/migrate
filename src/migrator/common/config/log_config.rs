use yaml_rust::{Yaml};
use super::{YamlConfig, get_yaml_str};
use crate::migrator::{MigError};

#[derive(Debug)]
pub struct LogConfig {
    pub drive: String,
    pub fs_type: String,
}

impl LogConfig {
    pub fn default() -> LogConfig {
        LogConfig{
            drive: String::from(""),
            fs_type: String::from(""),
        }
    }
}

impl YamlConfig for LogConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        format!(
            "{}log_to:\n{}  drive: '{}'\n{}  fs_type: '{}'\n", prefix, prefix, self.drive, prefix , self.fs_type)
    }

    fn from_yaml(&mut self, yaml: & Yaml) -> Result<(),MigError> {
        if let Some(log_drive) = get_yaml_str(yaml, &["drive"])? {
            if let Some(log_fs_type) = get_yaml_str(yaml, &["fs_type"])? {
                self.drive = String::from(log_drive);
                self.fs_type =  String::from(log_fs_type);                
            }    
        }
        Ok(())
    }  
}
