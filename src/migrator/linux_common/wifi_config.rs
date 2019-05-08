use failure::{Fail, ResultExt};
use log::{debug, trace, warn};
use regex::Regex;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::{
    common::{dir_exists, file_exists, is_balena_file, MigErrCtx, MigError, MigErrorKind, path_append},
};

const WPA_CONFIG_FILE: &str = "/etc/wpa_supplicant/wpa_supplicant.conf";
//const NWM_CONFIG_DIR: &str = "/etc/NetworkManager/system-connections/";
const CONNMGR_CONFIG_DIR: &str = "/var/lib/connman/";

const SKIP_REGEX: &str = r##"^(\s*#.*|\s*)$"##;
const WPA_NET_START_REGEX: &str = r#"^\s*network\s*=\s*\{\s*$"#;
const WPA_NET_PARAM1_REGEX: &str = r#"^\s*(\S+)\s*=\s*"([^"]+)"\s*$"#;
const WPA_NET_PARAM2_REGEX: &str = r#"^\s*(\S+)\s*=\s*(\S+)\s*$"#;
const WPA_NET_END_REGEX: &str = r#"^\s*\}\s*$"#;

const CONNMGR_PARAM_REGEX: &str = r#"^\s*(\S+)\s*=\s*(\S+)\s*$"#;

const NWMGR_CONTENT: &str = r##"## created by balena-migrate
[connection]
id="__FILE_NAME__"
type=wifi

[wifi]
hidden=true
mode=infrastructure
ssid="__SSID__"

[ipv4]
method=auto

[ipv6]
addr-gen-mode=stable-privacy
method=auto
"##;

const NWMGR_CONTENT_PSK: &str = r##"[wifi-security]
auth-alg=open
key-mgmt=wpa-psk
psk="__PSK__"
"##;

#[derive(Debug, PartialEq, Clone)]
enum WpaState {
    Init,
    Network,
}

pub(crate) struct WifiConfig {
    ssid: String,
    psk: Option<String>,
    // TODO: prepare for static config
}

impl<'a> WifiConfig {
    pub fn scan(ssid_filter: &Vec<String>) -> Result<Vec<WifiConfig>, MigError> {
        trace!("WifiConfig::scan: entered with {:?}", ssid_filter);
        let mut list: Vec<WifiConfig> = Vec::new();
        WifiConfig::from_wpa(&mut list, ssid_filter)?;
        WifiConfig::from_connman(&mut list, ssid_filter)?;
        Ok(list)
    }

    pub fn get_ssid(&'a self) -> &'a str {
        &self.ssid
    }

    fn parse_conmgr_file(file_path: &Path) -> Result<Option<WifiConfig>, MigError> {
        let mut ssid = String::from("");
        let mut psk: Option<String> = None;

        let skip_re = Regex::new(SKIP_REGEX).unwrap();
        let param_re = Regex::new(CONNMGR_PARAM_REGEX).unwrap();
        let file = File::open(file_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to open file {}", file_path.display()),
        ))?;

        for line in BufReader::new(file).lines() {
            match line {
                Ok(line) => {
                    if skip_re.is_match(&line) {
                        debug!("parse_conmgr_file: skipping line: '{}'", line);
                        continue;
                    }

                    debug!("parse_conmgr_file: processing line '{}'", line);

                    if let Some(captures) = param_re.captures(&line) {
                        let param = captures.get(1).unwrap().as_str();
                        let value = captures.get(2).unwrap().as_str();

                        if param == "Name" {
                            ssid = String::from(value);
                            continue;
                        }

                        if param == "Passphrase" {
                            psk = Some(String::from(value));
                            continue;
                        }
                    }

                    debug!("ignoring line '{}' from '{}'", line, file_path.display());
                    continue;
                }
                Err(why) => {
                    return Err(MigError::from(why.context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("unexpected read error from {}", file_path.display()),
                    ))));
                }
            }
        }

        if !ssid.is_empty() {
            Ok(Some(WifiConfig { ssid, psk }))
        } else {
            Ok(None)
        }
    }

    fn from_connman(
        wifis: &mut Vec<WifiConfig>,
        ssid_filter: &Vec<String>,
    ) -> Result<(), MigError> {
        if dir_exists(CONNMGR_CONFIG_DIR)? {
            let paths = read_dir(CONNMGR_CONFIG_DIR).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to list directory '{}'", CONNMGR_CONFIG_DIR),
            ))?;

            for path in paths {
                if let Ok(path) = path {
                    let dir_path = path.path();
                    debug!("got path '{}'", dir_path.display());
                    if let Some(dir_name) = dir_path.file_name() {
                        if dir_name.to_string_lossy().starts_with("wifi_")
                            && dir_path.metadata().unwrap().is_dir()
                        {
                            debug!("examining connmgr path '{}'", dir_path.display());
                            let settings_path = path_append(dir_path, "settings");
                            if settings_path.exists() {
                                debug!("examining connmgr path '{}'", settings_path.display());
                                if let Some(wifi) = WifiConfig::parse_conmgr_file(&settings_path)? {
                                    let mut valid = ssid_filter.len() == 0;
                                    if !valid {
                                        if let Some(_pos) =
                                            ssid_filter.iter().position(|r| r.as_str() == wifi.ssid)
                                        {
                                            valid = true;
                                        }
                                    }
                                    if valid {
                                        if let Some(_pos) =
                                            wifis.iter().position(|r| r.ssid == wifi.ssid)
                                        {
                                            debug!("Network '{}' is already contained in wifi list, skipping duplicate definition", wifi.ssid);
                                        } else {
                                            wifis.push(wifi);
                                        }
                                    }
                                }
                            }
                        } else {
                            debug!(
                                "no match on '{}' starts_with(wifi_): {} is_dir: {}",
                                dir_path.display(),
                                dir_name.to_string_lossy().starts_with("wifi_"),
                                dir_path.metadata().unwrap().is_dir()
                            );
                        }
                    } else {
                        warn!("Not processing invalid path '{}'", path.path().display());
                    }
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Error reading entry from directory '{}'",
                            CONNMGR_CONFIG_DIR
                        ),
                    ));
                }
            }
        } else {
            debug!(
                "WifiConfig::from_connman: directory not found: '{}'",
                CONNMGR_CONFIG_DIR
            );
        }

        Ok(())
    }

    fn from_wpa(wifis: &mut Vec<WifiConfig>, ssid_filter: &Vec<String>) -> Result<(), MigError> {
        trace!("WifiConfig::from_wpa: entered with {:?}", ssid_filter);

        if file_exists(WPA_CONFIG_FILE) {
            debug!("WifiConfig::from_wpa: scanning '{}'", WPA_CONFIG_FILE);
            let skip_re = Regex::new(SKIP_REGEX).unwrap();
            let net_start_re = Regex::new(WPA_NET_START_REGEX).unwrap();
            let net_end_re = Regex::new(WPA_NET_END_REGEX).unwrap();
            let net_param1_re = Regex::new(WPA_NET_PARAM1_REGEX).unwrap();
            let net_param2_re = Regex::new(WPA_NET_PARAM2_REGEX).unwrap();
            let file = File::open(WPA_CONFIG_FILE).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to open file {}", WPA_CONFIG_FILE),
            ))?;
            let mut state = WpaState::Init;
            let mut last_state = state.clone();
            let mut ssid: Option<String> = None;
            let mut psk: Option<String> = None;

            for line in BufReader::new(file).lines() {
                if last_state != state {
                    debug!("from_wpa:  {:?} -> {:?}", last_state, state);
                    last_state = state.clone()
                }

                match line {
                    Ok(line) => {
                        if skip_re.is_match(&line) {
                            debug!("skipping line: '{}'", line);
                            continue;
                        }

                        debug!("from_wpa: processing line '{}'", line);
                        match state {
                            WpaState::Init => {
                                if net_start_re.is_match(&line) {
                                    state = WpaState::Network;
                                } else {
                                    debug!("unexpected line '{}' in state {:?} while parsing file '{}'", &line, state, WPA_CONFIG_FILE);
                                }
                            }
                            WpaState::Network => {
                                if net_end_re.is_match(&line) {
                                    debug!("in state {:?} found end of network", state);
                                    if let Some(ssid) = ssid {
                                        // TODO: check if ssid is in filter list

                                        let mut valid = ssid_filter.len() == 0;
                                        if !valid {
                                            if let Some(_pos) =
                                                ssid_filter.iter().position(|r| r.as_str() == ssid)
                                            {
                                                valid = true;
                                            }
                                        }

                                        if valid == true {
                                            if let Some(_pos) =
                                                wifis.iter().position(|r| r.ssid == ssid)
                                            {
                                                debug!("Network '{}' is already contained in wifi list, skipping duplicate definition", ssid);
                                            } else {
                                                wifis.push(WifiConfig { ssid, psk });
                                            }
                                        } else {
                                            debug!("Network '{}' is not contained in filter: {:?}, not migrating", ssid, ssid_filter);
                                        }
                                    } else {
                                        warn!("empty network config encountered");
                                    }

                                    state = WpaState::Init;
                                    ssid = None;
                                    psk = None;
                                    continue;
                                }

                                if let Some(captures) = net_param1_re.captures(&line) {
                                    let param = captures.get(1).unwrap().as_str();
                                    let value = captures.get(2).unwrap().as_str();
                                    debug!(
                                        "in state {:?} got param: '{}', value: '{}'",
                                        state, param, value
                                    );
                                    match param {
                                        "ssid" => {
                                            debug!("in state {:?} set ssid to '{}'", state, value);
                                            ssid = Some(String::from(value));
                                        }
                                        "psk" => {
                                            debug!("in state {:?} set psk to '{}'", state, value);
                                            psk = Some(String::from(value));
                                        }
                                        _ => {
                                            debug!("in state {:?} ignoring line '{}'", state, line);
                                        }
                                    }
                                    continue;
                                }

                                if let Some(captures) = net_param2_re.captures(&line) {
                                    let param = captures.get(1).unwrap().as_str();
                                    let value = captures.get(2).unwrap().as_str();
                                    debug!(
                                        "in state {:?} got param: '{}', value: '{}'",
                                        state, param, value
                                    );
                                    match param {
                                        "ssid" => {
                                            debug!("in state {:?} set ssid to '{}'", state, value);
                                            ssid = Some(String::from(value));
                                        }
                                        "psk" => {
                                            debug!("in state {:?} set psk to '{}'", state, value);
                                            psk = Some(String::from(value));
                                        }
                                        _ => {
                                            debug!("in state {:?} ignoring line '{}'", state, line);
                                        }
                                    }
                                    continue;
                                }

                                warn!("in state {:?} ignoring line '{}'", state, line);
                            }
                        }
                    }
                    Err(why) => {
                        return Err(MigError::from(why.context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("unexpected read error from {}", WPA_CONFIG_FILE),
                        ))));
                    }
                }
            }
        } else {
            debug!(
                "WifiConfig::from_wpa: file not found: '{}'",
                WPA_CONFIG_FILE
            );
        }

        Ok(())
    }

    pub(crate) fn create_nwmgr_file<P: AsRef<Path>>(
        &self,
        base_path: P,
        last_index: u64,
    ) -> Result<u64, MigError> {
        let mut index = last_index + 1;
        let mut file_ok = false;
        let base_path = base_path.as_ref();
        let mut path = path_append(base_path, &format!("resin-wifi-{}", index));

        while !file_ok {
            if file_exists(&path) {
                if is_balena_file(&path)? {
                    file_ok = true;
                } else {
                    index += 1;
                    path = path_append(base_path, &format!("resin-wifi-{}", index));
                }
            } else {
                file_ok = true;
            }
        }

        let mut nwmgr_file = File::create(&path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to create file in '{}'", path.display()),
        ))?;

        let name = path.file_name().unwrap().to_string_lossy();

        let mut content = NWMGR_CONTENT.replace("__SSID__", &self.ssid);
        content = content.replace("__FILE_NAME__", &name);

        if let Some(ref psk) = self.psk {
            content.push_str(&NWMGR_CONTENT_PSK.replace("__PSK__", psk));
        }

        nwmgr_file
            .write_all(content.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to write new '{:?}'", path.display()),
            ))?;
        Ok(index)
    }
}
