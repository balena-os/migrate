
use super::{log_config::LogConfig, YamlConfig};
use std::path::{Path, PathBuf};

use crate::{
    common::{
        config_helper::{get_yaml_bool, get_yaml_int, get_yaml_str, get_yaml_val},
        MigError, MigErrorKind,
        FailMode,
    },
};

use yaml_rust::Yaml;

const MODULE: &str = "common::config::migrate_config";

#[derive(Debug, PartialEq)]
pub enum MigMode {
    AGENT,
    IMMEDIATE,
    PRETEND,
}

const DEFAULT_MODE: MigMode = MigMode::PRETEND;

#[derive(Debug)]
pub(crate) struct MigrateConfig {
    pub work_dir: PathBuf,
    pub mode: MigMode,
    pub reboot: Option<u64>,
    pub all_wifis: bool,
    pub wifis: Vec<String>,
    pub log_to: Option<LogConfig>,
    pub kernel_file: Option<PathBuf>,
    pub initramfs_file: Option<PathBuf>,
    pub force_slug: Option<String>,
    pub fail_mode: Option<FailMode>,
}

impl<'a> MigrateConfig {
    pub fn default() -> MigrateConfig {
        MigrateConfig {
            work_dir: PathBuf::from("./"),
            fail_mode: None,
            mode: DEFAULT_MODE,
            reboot: None,
            all_wifis: false,
            wifis: Vec::new(),
            log_to: None,
            kernel_file: None,
            initramfs_file: None,
            force_slug: None,
        }
    }

    pub fn get_kernel_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.kernel_file {
            path
        } else {
            panic!("kernel path is not set");
        }
    }

    pub fn get_initramfs_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.initramfs_file {
            path
        } else {
            panic!("initramfs path is not set");
        }
    }

    pub fn check(&self, mig_mode: &MigMode) -> Result<(), MigError> {
        // TODO: implement
        Ok(())
    }
}

impl YamlConfig for MigrateConfig {
    fn from_yaml(yaml: &Yaml) -> Result<Box<MigrateConfig>, MigError> {
        let mut config = MigrateConfig::default();

        if let Some(work_dir) = get_yaml_str(yaml, &["work_dir"])? {
            config.work_dir = PathBuf::from(work_dir);
        }

        if let Some(kernel_file) = get_yaml_str(yaml, &["kernel_file"])? {
            config.kernel_file = Some(PathBuf::from(kernel_file));
        }

        if let Some(initramfs_file) = get_yaml_str(yaml, &["initramfs_file"])? {
            config.initramfs_file = Some(PathBuf::from(initramfs_file));
        }

        if let Some(fail_mode) = get_yaml_str(yaml, &["fail_mode"])? {
            config.fail_mode = Some(FailMode::from_str(fail_mode)?.clone());
        }

        if let Some(mode) = get_yaml_str(yaml, &["mode"])? {
            if mode.to_lowercase() == "immediate" {
                config.mode = MigMode::IMMEDIATE;
            } else if mode.to_lowercase() == "agent" {
                config.mode = MigMode::AGENT;
            } else if mode.to_lowercase() == "pretend" {
                config.mode = MigMode::PRETEND;
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::from_string: invalid value for migrate mode '{}'",
                        MODULE, mode
                    ),
                ));
            }
        }

        // Param: reboot - must be > 0
        if let Some(reboot_timeout) = get_yaml_int(yaml, &["reboot"])? {
            if reboot_timeout > 0 {
                config.reboot = Some(reboot_timeout as u64);
            } else {
                config.reboot = None;
            }
        }

        // Param: all_wifis - must be > 0
        if let Some(all_wifis) = get_yaml_bool(yaml, &["all_wifis"])? {
            config.all_wifis = all_wifis;
        }

        if let Some(wifis) = get_yaml_val(yaml, &["wifis"])? {
            if let Yaml::Array(wifis) = wifis {
                for ssid in wifis {
                    if let Yaml::String(ssid) = ssid {
                        config.wifis.push(ssid.clone());
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                                "{}::from_string: invalid value for wifis - ssid , expected string, got  '{:?}'",
                                MODULE, ssid
                            ),
                        ));
                    }
                }
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::from_string: invalid value for wifis, expected array, got  '{:?}'",
                        MODULE, wifis,
                    ),
                ));
            }
        }

        // Params: log_to: drive, fs_type
        if let Some(log_section) = get_yaml_val(yaml, &["log_to"])? {
            config.log_to = Some(*LogConfig::from_yaml(log_section)?);
        }

        // Param: all_wifis - must be > 0
        if let Some(force_slug) = get_yaml_str(yaml, &["force_slug"])? {
            config.force_slug = Some(String::from(force_slug));
        }

        Ok(Box::new(config))
    }
}
