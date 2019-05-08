use super::YamlConfig;
use crate::common::{config_helper::get_yaml_str, MigError, MigErrorKind};

use yaml_rust::Yaml;

// *************************************************************************
// * Log configuration
// * Stage 1
// * - from LOG_CONFIG variable
// * Stage2
// * - log level from where ?
// * - log to initramfs
// * - after flashing
// *   - move log file to destination configured in this config
// *   - or to configured log drive

#[derive(Debug)]
pub struct LogConfig {
    pub drive: String,
    pub fs_type: String,
}

impl LogConfig {
    pub fn default() -> LogConfig {
        LogConfig {
            drive: String::from(""),
            fs_type: String::from(""),
        }
    }
}

impl YamlConfig for LogConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<LogConfig>, MigError> {
        Ok(Box::new(
            LogConfig{
                drive:
                    if let Some(log_drive) = get_yaml_str(yaml, &["drive"])? {
                        String::from(log_drive)
                    } else {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, "failed to retrieve parameter drive for LogConfig"));
                    },
                fs_type:
                    if let Some(log_fs_type) = get_yaml_str(yaml, &["fs_type"])? {
                        String::from(log_fs_type)
                    } else {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, "failed to retrieve parameter 'fs_type' for LogConfig"));
                    },
            }
        ))
    }

    /*
        fn to_yaml(&self, prefix: &str) -> String {
            format!(
                "{}log_to:\n{}  drive: '{}'\n{}  fs_type: '{}'\n",
                prefix, prefix, self.drive, prefix, self.fs_type
            )
            // TODO: incomplete add log_levels
        }
    */

}
