use failure::{ResultExt};
use log::{error, info, warn};
use serde_json;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use serde::{Deserialize, Deserializer, de::{self, Unexpected}};
use std::fmt;


use crate::common::{
    check_tcp_connect, file_info::RelFileInfo, Config, FileInfo, MigErrCtx, MigError, MigErrorKind,
};

struct DeserializeU64OrStringVisitor;

impl<'de> de::Visitor<'de> for DeserializeU64OrStringVisitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer or a string")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        match v.parse::<u64>() {
            Ok(val) => Ok(val),
            Err(_why) => {
                Err(E::invalid_value(Unexpected::Str(v), &self))
            }
        }
    }
}

fn deserialize_u64_or_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
{
    deserializer.deserialize_any(DeserializeU64OrStringVisitor)
}

struct DeserializeU16OrStringVisitor;

impl<'de> de::Visitor<'de> for DeserializeU16OrStringVisitor {
    type Value = u16;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer or a string")
    }

    fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        match v.parse::<u16>() {
            Ok(val) => Ok(val),
            Err(_why) => {
                Err(E::invalid_value(Unexpected::Str(v), &self))
            }
        }
    }
}

fn deserialize_u16_or_string<'de, D>(deserializer: D) -> Result<u16, D::Error>
    where
        D: Deserializer<'de>,
{
    deserializer.deserialize_any(DeserializeU16OrStringVisitor)
}

// TODO: make u16 work

#[derive(Debug, Deserialize, Clone)]
struct BalenaConfig {
    #[serde(rename = "applicationName")]
    pub app_name: String,
    #[serde(rename = "applicationId")]
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub app_id: u64,
    #[serde(rename = "deviceType")]
    pub device_type: String,
    #[serde(rename = "userId")]
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub user_id: u64,
    pub username: String,
    #[serde(rename = "appUpdatePollInterval")]
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub app_poll_interval: u64,
    #[serde(rename = "listenPort")]
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub listen_port: u64,
    #[serde(rename = "vpnPort")]
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub vpn_port: u64,
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
            if let Ok(_v) = check_tcp_connect(&self.config.vpn_endpoint, self.config.vpn_port as u16, config.balena.get_check_timeout())
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
