use serde::Deserialize;

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

#[derive(Debug, Deserialize)]
pub struct LogConfig {
    pub drive: Option<String>,
    pub fs_type: Option<String>,
}

impl LogConfig {
    pub fn default() -> LogConfig {
        LogConfig {
            drive: None,
            fs_type: None,
        }
    }
}
