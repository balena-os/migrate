use super::{LogConfig, YamlConfig};
use std::path::{Path, PathBuf};

use crate::{
    common::{
        config_helper::{get_yaml_bool, get_yaml_int, get_yaml_str, get_yaml_val},
        MigError, MigErrorKind,
    },
    linux_common::FailMode,
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
    pub work_dir: String,
    pub mode: MigMode,
    pub reboot: Option<u64>,
    pub all_wifis: bool,
    pub wifis: Vec<String>,
    pub log_to: Option<LogConfig>,
    pub kernel_file: Option<PathBuf>,
    pub initramfs_file: Option<PathBuf>,
    pub force_slug: Option<String>,
    pub fail_mode: Option<FailMode>,
    pub no_flash: bool,
}

impl<'a> MigrateConfig {
    pub fn default() -> MigrateConfig {
        MigrateConfig {
            work_dir: String::from("."),
            fail_mode: None,
            mode: DEFAULT_MODE,
            reboot: None,
            all_wifis: false,
            wifis: Vec::new(),
            log_to: None,
            kernel_file: None,
            initramfs_file: None,
            force_slug: None,
            no_flash: false,
        }
    }

    pub(crate) fn get_kernel_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.kernel_file {
            path
        } else {
            panic!("kernel path is not set");
        }
    }

    pub(crate) fn get_initramfs_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.initramfs_file {
            path
        } else {
            panic!("initramfs path is not set");
        }
    }
}

impl YamlConfig for MigrateConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = format!(
            "{}migrate:\n{}  work_dir: '{}'\n{}  mode: '{:?}'\n",
            prefix, prefix, self.work_dir, prefix, self.mode
        );

        let next_prefix = String::from(prefix) + "  ";

        if self.wifis.len() > 0 {
            output += &format!("{}  wifis:\n", prefix);
            for wifi in &self.wifis {
                output += &format!("{}  - '{}'\n", next_prefix, wifi);
            }
        } else {
            output += &format!("{}  all_wifis: {}\n", prefix, self.all_wifis);
        }

        if let Some(i) = self.reboot {
            output += &format!("{}  reboot: {}\n", prefix, i);
        }

        if let Some(ref kernel_file) = self.kernel_file {
            output += &format!(
                "{}  kernel_file: {}\n",
                prefix,
                &kernel_file.to_string_lossy()
            );
        }

        if let Some(ref initramfs_file) = self.initramfs_file {
            output += &format!(
                "{}  initramfs_file: {}\n",
                prefix,
                &initramfs_file.to_string_lossy()
            );
        }

        if let Some(slug) = &self.force_slug {
            output += &format!("{}  force_slug: '{}'\n", prefix, slug);
        }

        if let Some(ref log_to) = self.log_to {
            output += &log_to.to_yaml(&next_prefix);
        }

        output
    }

    fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(work_dir) = get_yaml_str(yaml, &["work_dir"])? {
            self.work_dir = String::from(work_dir);
        }

        if let Some(kernel_file) = get_yaml_str(yaml, &["kernel_file"])? {
            self.kernel_file = Some(PathBuf::from(kernel_file));
        }

        if let Some(initramfs_file) = get_yaml_str(yaml, &["initramfs_file"])? {
            self.initramfs_file = Some(PathBuf::from(initramfs_file));
        }

        if let Some(fail_mode) = get_yaml_str(yaml, &["fail_mode"])? {
            self.fail_mode = Some(FailMode::from_str(fail_mode)?.clone());
        }

        if let Some(no_flash) = get_yaml_bool(yaml, &["no_flash"])? {
            self.no_flash = no_flash;
        }

        if let Some(mode) = get_yaml_str(yaml, &["mode"])? {
            if mode.to_lowercase() == "immediate" {
                self.mode = MigMode::IMMEDIATE;
            } else if mode.to_lowercase() == "agent" {
                self.mode = MigMode::AGENT;
            } else if mode.to_lowercase() == "pretend" {
                self.mode = MigMode::PRETEND;
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
                self.reboot = Some(reboot_timeout as u64);
            } else {
                self.reboot = None;
            }
        }

        // Param: all_wifis - must be > 0
        if let Some(all_wifis) = get_yaml_bool(yaml, &["all_wifis"])? {
            self.all_wifis = all_wifis;
        }

        if let Some(wifis) = get_yaml_val(yaml, &["wifis"])? {
            if let Yaml::Array(wifis) = wifis {
                for ssid in wifis {
                    if let Yaml::String(ssid) = ssid {
                        self.wifis.push(ssid.clone());
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
            if let Some(ref mut log_to) = self.log_to {
                log_to.from_yaml(yaml)?;
            } else {
                let mut log_to = LogConfig::default();
                log_to.from_yaml(log_section)?;
                self.log_to = Some(log_to);
            }
        }

        // Param: all_wifis - must be > 0
        if let Some(force_slug) = get_yaml_str(yaml, &["force_slug"])? {
            self.force_slug = Some(String::from(force_slug));
        }

        Ok(())
    }
}
