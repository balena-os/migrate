use failure::{ResultExt};
use log::{error, info, warn};
use serde_json;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use serde::{Deserialize};

use crate::common::{
    check_tcp_connect, file_info::RelFileInfo, Config, FileInfo, MigErrCtx, MigError, MigErrorKind,
};

#[derive(Debug, Deserialize, Clone)]
struct BalenaConfig {
    #[serde(rename = "applicationName")]
    pub app_name: String,
    #[serde(rename = "applicationId")]
    pub app_id: String,
    #[serde(rename = "deviceType")]
    pub device_type: String,
    #[serde(rename = "userId")]
    pub user_id: u64,
    pub username: String,
    #[serde(rename = "appUpdatePollInterval")]
    pub app_poll_interval: u64,
    #[serde(rename = "listenPort")]
    pub listen_port: u16,
    #[serde(rename = "vpnPort")]
    pub vpn_port: u16,
    #[serde(rename = "apiEndpoint")]
    pub api_endpoint: String,
    #[serde(rename = "vpnEndpoint")]
    pub vpn_endpoint: String,
    #[serde(rename = "registryEndpoint")]
    pub registry_endpoint: String,
    #[serde(rename = "deltaEndpoint")]
    pub delta_endpoint: String,
    #[serde(rename = "pubnubSubscribeKey")]
    pub pubnub_subscr_key: String,
    #[serde(rename = "pubnubPublishKey")]
    pub pubnub_publish_key: String,
    #[serde(rename = "mixpanelToken")]
    pub mixpanel_token: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BalenaCfgJson {
    config: BalenaConfig,
    file: RelFileInfo,
}

impl BalenaCfgJson {
    pub fn new(cfg_file: FileInfo) -> Result<BalenaCfgJson, MigError> {
        Ok(BalenaCfgJson {
            config: serde_json::from_reader(BufReader::new(File::open(&cfg_file.path).context(
                MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("new: cannot open file '{}'", cfg_file.path.display()),
                ),
            )?))
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("new: failed to parse '{}'", cfg_file.path.display()),
            ))?,
            file: cfg_file.to_rel_fileinfo()?,
        })
    }

    pub fn check(&self, config: &Config, xpctd_dev_type: &str) -> Result<(), MigError> {
        info!("Configured for application: {}", self.config.app_name);

        if self.config.device_type == xpctd_dev_type {
            info!("Configured for device type: {}", xpctd_dev_type);
        } else {
            error!("The device type configured in the config.json file supplied does not match the hardware device type found, expected {}, found {}", xpctd_dev_type, self.config.device_type);
            return Err(MigError::displayed());
        }

        // TODO: check API too

        if config.balena.is_check_vpn() {
            if let Ok(_v) = check_tcp_connect(&self.config.vpn_endpoint, self.config.vpn_port, config.balena.get_check_timeout())
            {
                info!("connection to vpn: {}:{} is ok", self.config.vpn_endpoint, self.config.vpn_port);
            } else {
                // TODO: add option require_connect and fail if connection is required but not available
                warn!(
                    "failed to connect to vpn server @ {}:{} your device might not come online",
                    self.config.vpn_endpoint, self.config.vpn_port
                );
            }
        }

        Ok(())
    }


    pub fn get_rel_path(& self) -> &PathBuf {
        &self.file.rel_path
    }

}
