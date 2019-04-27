use failure::{Fail, ResultExt};
use log::{error, info, warn};
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;

use crate::migrator::common::{
    check_tcp_connect, config_helper::get_json_str, MigErrCtx, MigError, MigErrorKind,
};

const MODULE: &str = "migrator::common::balena_cfg_json";

pub struct BalenaCfgJson {
    doc: Value,
    file: String,
}

impl BalenaCfgJson {
    pub fn new(cfg_file: &str) -> Result<BalenaCfgJson, MigError> {
        Ok(BalenaCfgJson {
            doc: serde_json::from_reader(BufReader::new(File::open(cfg_file).context(
                MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::try_init:cannot open file '{}'", MODULE, cfg_file),
                ),
            )?))
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("{}::new: failed to parse '{}'", MODULE, cfg_file),
            ))?,
            file: String::from(cfg_file),
        })
    }

    pub fn check(&self, xpctd_dev_type: &str) -> Result<(), MigError> {
        info!("Configured for application: {}", self.get_app_name()?);

        if self.get_device_type()? == xpctd_dev_type {
            info!("Configured for device type: {}", xpctd_dev_type);
        } else {
            let message = format!("The device type configured in the config.json file supplied does not match the hardware device type found, expected {}, found {}", xpctd_dev_type, self.get_device_type()?);
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        let vpn_addr = self.get_vpn_endpoint()?;
        let vpn_port = self.get_vpn_port()?;

        if let Ok(_v) = check_tcp_connect(vpn_addr, vpn_port, 60) {
            info!("connection to vpn: {}:{} is ok", vpn_addr, vpn_port);
        } else {
            // TODO: add option require_connect and fail if cobnnection is required but not available
            warn!(
                "failed to connect to vpn server @ {}:{} your device might not come online",
                vpn_addr, vpn_port
            );
        }
        Ok(())
    }

    pub fn get_app_name<'a>(&self) -> Result<&str, MigError> {
        self.get_string_cfg("applicationName")
    }

    pub fn get_device_type<'a>(&self) -> Result<&str, MigError> {
        self.get_string_cfg("deviceType")
    }

    pub fn get_vpn_endpoint<'a>(&self) -> Result<&str, MigError> {
        self.get_string_cfg("vpnEndpoint")
    }

    pub fn get_vpn_port<'a>(&self) -> Result<u16, MigError> {
        let vpn_port = self.get_string_cfg("vpnPort")?;
        Ok(vpn_port.parse::<u16>().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "The key 'vpnEndpoint' is invalid in the config.json supplied in: '{}'.",
                self.file
            ),
        ))?)
    }

    fn get_string_cfg(&self, name: &str) -> Result<&str, MigError> {
        match get_json_str(&self.doc, name) {
            Ok(res) => match res {
                Some(res) => Ok(&res),
                None => Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "The key '{}' is missing in the config.json supplied in: '{}'.",
                        name, self.file
                    ),
                )),
            },
            Err(why) => Err(MigError::from(why.context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "The key '{}' is invalid in the config.json supplied in: '{}'.",
                    name, self.file
                ),
            )))),
        }
    }
}

/*
pub fn check_balena_cfg(cfg_path: &str) -> Result<(),MigError> {

    // TODO: basic sanity test on config.json


    if let Some(vpn_addr) = get_json_str(&parse_res, "vpnEndpoint")? {
        if let Some(vpn_port) = get_json_str(&parse_res, "vpnPort")? {
            let port = vpn_port.parse::<u16>().context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to parse vpn port from {}", vpn_port)))?;
            if let Ok(_v) = check_tcp_connect(vpn_addr, port, 60) {
                info!("connection to vpn: {}:{} is ok", vpn_addr, port);
            } else {
                // TODO: add option require_connect and fail if cobnnection is required but not available
                warn!("failed to connect to vpn server @ {}:{} your device might not come online", vpn_addr, port);
            }
        } else {
            let message = String::from("The balena config does not contain some required fields, please supply a valid config.json");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }
    } else {
        let message = String::from("The balena config does not contain some required fields, please supply a valid config.json");
        error!("{}", message);
        return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
    }

    if let Some(dev_type) = parse_res.get("deviceType") {
        if let Value::String(dev_type) = dev_type {
            if let Some(ref xpctd_type) = migrator.sysinfo.device_slug {
                if xpctd_type == dev_type {
                    info!("Configured for device type: {}", dev_type);
                } else {
                    let message = format!("The device type configured in the config.json file supplied does not match the hardware device type found, expected {}, found {}", xpctd_type, dev_type);
                    error!("{}", message);
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
                }
            } else {
                panic!("migrator.sysinfo.device_slug should not be empty");
            }
        } else {
            let message = String::from("The balena config does contains an invalid value in the device_type field (not string),  please supply a valid config.json");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }
    // TODO: check device type
    } else {
        let message = String::from("The balena config does not contain some required fields, please supply a valid config.json");
        error!("{}", message);
        return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
    }

    info!("The balena OS config looks ok: '{}'", file_info.path);

    Err(MigError::from(MigErrorKind::NotImpl))
}
*/
