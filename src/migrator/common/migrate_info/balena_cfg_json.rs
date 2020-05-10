use failure::ResultExt;
use log::{error, info};
use serde::{
    de::{self, Unexpected},
    Deserialize, Deserializer,
};
use serde_json;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use url::Url;

use crate::{
    common::{
        check_tcp_connect, file_info::RelFileInfo, Config, FileInfo, MigErrCtx, MigError,
        MigErrorKind,
    },
    defs::BALENA_API_PORT,
};

// TODO: get better understanding of required/optional values and implement

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
            Err(_why) => Err(E::invalid_value(Unexpected::Str(v), &self)),
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

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v <= 0xFFFF {
            Ok(v as u16)
        } else {
            Err(E::invalid_value(Unexpected::Unsigned(v), &self))
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.parse::<u16>() {
            Ok(val) => Ok(val),
            Err(_why) => Err(E::invalid_value(Unexpected::Str(v), &self)),
        }
    }
}

fn deserialize_u16_or_string<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(DeserializeU16OrStringVisitor)
}

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
    #[serde(deserialize_with = "deserialize_u16_or_string")]
    pub listen_port: u16,
    #[serde(rename = "vpnPort")]
    #[serde(deserialize_with = "deserialize_u16_or_string")]
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
    pub api_key: Option<String>,
    // TODO: apiKey can not be safely left empty. Device will migrate successfully but not connect
    // to VPN once started. Greg claims optional apiKey required for preloaded images
    #[serde(rename = "deviceApiKey")]
    pub device_api_key: Option<String>,
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
        // TODO: app_name is not checked
        info!("Configured for application: {}", self.config.app_name);

        if self.config.device_type == xpctd_dev_type {
            info!("Configured for device type: {}", xpctd_dev_type);
        } else {
            error!("The device type configured in the config.json file supplied does not match the hardware device type found, expected {}, found {}", xpctd_dev_type, self.config.device_type);
            return Err(MigError::displayed());
        }

        if config.is_check_api() {
            let api_url = Url::parse(&self.config.api_endpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to parse balena api url '{}'",
                    self.config.api_endpoint
                ),
            ))?;

            if let Some(api_host) = api_url.host() {
                let api_host = api_host.to_string();
                let api_port = if let Some(api_port) = api_url.port() {
                    api_port
                } else {
                    BALENA_API_PORT
                };

                if let Ok(_v) = check_tcp_connect(&api_host, api_port, config.get_check_timeout()) {
                    info!("connection to api: {}:{} is ok", api_host, api_port);
                } else {
                    error!(
                        "failed to connect to api server @ {}:{} your device might not come online",
                        self.config.api_endpoint, api_port
                    );
                    return Err(MigError::displayed());
                }
            } else {
                error!(
                    "failed to parse api server url from config.json: {}",
                    self.config.api_endpoint
                );
                return Err(MigError::displayed());
            }
        }

        if config.is_check_vpn() {
            if let Ok(_v) = check_tcp_connect(
                &self.config.vpn_endpoint,
                self.config.vpn_port as u16,
                config.get_check_timeout(),
            ) {
                // TODO: call a command on API instead of just connecting
                info!(
                    "connection to vpn: {}:{} is ok",
                    self.config.vpn_endpoint, self.config.vpn_port
                );
            } else {
                error!(
                    "failed to connect to vpn server @ {}:{} your device might not come online",
                    self.config.vpn_endpoint, self.config.vpn_port
                );
                return Err(MigError::displayed());
            }
        }

        Ok(())
    }

    pub fn get_size(&self) -> u64 {
        self.file.size
    }

    pub fn get_rel_path(&self) -> &PathBuf {
        &self.file.rel_path
    }

    pub fn get_api_key(&self) -> Option<String> {
        if let Some(ref api_key) = self.config.api_key {
            Some(api_key.clone())
        } else {
            None
        }
    }
    pub fn get_api_endpoint(&self) -> String {
        self.config.api_endpoint.clone()
    }
}

#[cfg(test)]
mod tests {
    const CONFIG1: &str = r###"
{ "applicationName":"TestDev",
  "applicationId":1284711,
  "deviceType":"raspberrypi3",
  "userId":120815,
  "username":"g_user",
  "appUpdatePollInterval":600000,
  "listenPort":48484,
  "vpnPort":443,
  "apiEndpoint":"https://api.balena-cloud.com",
  "vpnEndpoint":"vpn.balena-cloud.com",
  "registryEndpoint":"registry2.balena-cloud.com",
  "deltaEndpoint":"https://delta.balena-cloud.com",
  "pubnubSubscribeKey":"",
  "pubnubPublishKey":"",
  "mixpanelToken":"9ef939ea64cb6cd9ef939ea64cb6cd",
  "apiKey":"1xf6r2oNmJJt4M1xf6r2oNmJJt4M"}
"###;

    const CONFIG2: & str = r###"
{"applicationName":"test",
 "applicationId":13454711,
 "deviceType":"beaglebone-green",	
 "userId":44815,	
 "username":"thomasr",
 "appUpdatePollInterval":"600000",	
 "listenPort":"48484",	
 "vpnPort":443,	
 "apiEndpoint":"https://api.balena-cloud.com",
 "vpnEndpoint":"vpn.balena-cloud.com",
 "registryEndpoint":"registry2.balena-cloud.com", 	
 "deltaEndpoint":"https://delta.balena-cloud.com",
 "pubnubSubscribeKey":"",	
 "pubnubPublishKey":"",	
 "mixpanelToken":"9ef939ea64cb6cd9ef939ea64cb6cd",
 "apiKey":"abcabcabcabcabcabcabcabcabca",
 "os": {    "sshKeys": [
   "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDb6MO7mLf5kXjRgTsaDzAH3ee74if4Endy/ZCBxGwt4vG4kl6bP9Ky7JBN5neG/srrrG4ezWkn2I9lz+MNqazT6TmzpBp1gan3CE0IVQRmdoaSW0V/n3oAucfN0tx0RZ7Zkn5CqnzNfLvTGSzlGM8g2Sfqpd3lCEIrQJFlagOqPW2eBB9FQrI+i8+cwM2iny25h4Fl7yiZIQ579hEHNDM8sCsrSfmApbpTnL7uNJM2gsJlpMNnrQjPAN16zViOmvgKB/BwuuvzGYMSVXRA/vb5GVhcPsAUT0sE1hgaEb"
  ]}
}"###;

    // Testing Device API Key case, such as when there's a pre-provisioned device
    const CONFIG3: &str = r###"
{"applicationName":"abc",
 "applicationId":123,
 "deviceType":"raspberrypi3",
 "userId":456,
 "username":"test",
 "appUpdatePollInterval":600000,
 "listenPort":48484,
 "vpnPort":443,
 "apiEndpoint":"https://api.balena-cloud.com",
 "vpnEndpoint":"vpn.balena-cloud.com",
 "registryEndpoint":"registry2.balena-cloud.com",
 "deltaEndpoint":"https://delta.balena-cloud.com",
 "pubnubSubscribeKey":"",
 "pubnubPublishKey":"",
 "mixpanelToken":"xyzxyzxyz",
 "deviceApiKey":"aaaaaaaaaaaa",
 "registered_at":1573045985,
 "deviceId":789,
 "uuid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
"###;

    use super::*;

    #[test]
    fn read_conf_ok1() {
        let config: BalenaConfig = serde_json::from_str(CONFIG1).unwrap();
        assert_eq!(config.app_name, "TestDev");
        assert_eq!(config.app_id, 1_284_711);
        assert_eq!(config.vpn_port, 443);
        assert_eq!(config.device_type, "raspberrypi3");
        assert_eq!(config.api_key.unwrap(), "1xf6r2oNmJJt4M1xf6r2oNmJJt4M");
        assert_eq!(config.device_api_key, None);
        assert_eq!(config.user_id, 120_815);
        assert_eq!(config.username, "g_user");
        assert_eq!(config.app_poll_interval, 600_000);
        assert_eq!(config.listen_port, 48_484);
        assert_eq!(config.api_endpoint, "https://api.balena-cloud.com");
        assert_eq!(config.vpn_endpoint, "vpn.balena-cloud.com");
        assert_eq!(config.registry_endpoint, "registry2.balena-cloud.com");
        assert_eq!(config.delta_endpoint, "https://delta.balena-cloud.com");
        assert_eq!(config.pubnub_subscr_key, "");
        assert_eq!(config.pubnub_publish_key, "");
        assert_eq!(config.mixpanel_token, "9ef939ea64cb6cd9ef939ea64cb6cd");
    }

    // TODO: make tests 2, 3 meaningfull
    #[test]
    fn read_conf_ok2() {
        let config: BalenaConfig = serde_json::from_str(CONFIG2).unwrap();
        assert_eq!(config.app_name, "test");
        assert_eq!(config.app_id, 13_454_711);
        assert_eq!(config.vpn_port, 443);
        assert_eq!(config.api_key.unwrap(), "abcabcabcabcabcabcabcabcabca");
        assert_eq!(config.device_api_key, None);
    }

    #[test]
    fn read_conf_ok3() {
        let config: BalenaConfig = serde_json::from_str(CONFIG3).unwrap();
        assert_eq!(config.api_key, None);
        assert_eq!(config.device_api_key.unwrap(), "aaaaaaaaaaaa");
    }
}
