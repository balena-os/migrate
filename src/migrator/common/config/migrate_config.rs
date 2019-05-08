
use super::{LogConfig,BackupConfig};
use std::path::{Path, PathBuf};

use crate::{
    common::{
        config_helper::{get_yaml_bool, get_yaml_int, get_yaml_str, get_yaml_val},
        MigError, MigErrorKind,
        FailMode,
    },
};

use serde::{Deserialize};
use yaml_rust::Yaml;

const MODULE: &str = "common::config::migrate_config";

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub(crate) enum MigMode {
    AGENT,
    IMMEDIATE,
    PRETEND,
}

impl MigMode {
    pub fn from_str(mode: &str) -> Result<Self, MigError> {
        match mode.to_lowercase().as_str() {
            "immediate" => Ok(MigMode::IMMEDIATE),
            "agent" => Ok(MigMode::AGENT),
            "pretend" => Ok(MigMode::PRETEND),
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::new: invalid value for parameter mode: '{}'",
                        MODULE, mode
                    ),
                ));
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum MigrateWifis {
    NONE,
    ALL,
    SOME(Vec<String>),
}


const DEFAULT_MIG_MODE: MigMode = MigMode::PRETEND;

#[derive(Debug, Deserialize)]
pub(crate) struct MigrateConfig {
    pub work_dir: Option<PathBuf>,
    pub mode: Option<MigMode>,
    pub reboot: Option<u64>,
    pub all_wifis: Option<bool>,
    pub wifis: Option<Vec<String>>,
    pub log_to: Option<LogConfig>,
    pub kernel_file: Option<PathBuf>,
    pub initramfs_file: Option<PathBuf>,
    pub force_slug: Option<String>,
    pub fail_mode: Option<FailMode>,
    pub backup_config: Option<BackupConfig>
}

impl<'a> MigrateConfig {
    pub fn default() -> MigrateConfig {
        MigrateConfig{
            work_dir: None,
            mode: Some(DEFAULT_MIG_MODE.clone()),
            reboot: None,
            all_wifis: None,
            wifis: None,
            log_to: None,
            kernel_file: None,
            initramfs_file: None,
            force_slug: None,
            fail_mode: Some(FailMode::get_default().clone()),
            backup_config: None,
        }
    }

    pub fn check(&mut self) -> Result<(), MigError> {
        // TODO: implement
        if let None = self.mode {
            self.mode = Some(DEFAULT_MIG_MODE.clone());
        }

        if let Some(ref mode) = self.mode {
            match mode {
                MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
                _ => {
                    if let None = self.work_dir {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, "A required parameter was not found: 'work_dir'"));
                    }

                    if let None = self.fail_mode {
                        self.fail_mode = Some(FailMode::get_default().clone());
                    }

                    if let None = self.kernel_file {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, "A required parameter was not found: 'kernel_file'"));
                    }

                    if let None = self.initramfs_file {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, "A required parameter was not found: 'initramfs_file'"));
                    }

                    Ok(())
                }
            }
        } else {
            panic!("migrate mode is not set");
        }
    }



    pub fn get_wifis(&self) -> MigrateWifis {
        if let Some(ref wifis) = self.wifis {
            MigrateWifis::SOME(wifis.clone())
        } else {
            if let Some(ref all_wifis) = self.all_wifis {
                if *all_wifis {
                    MigrateWifis::ALL
                } else {
                    MigrateWifis::NONE
                }
            } else {
                MigrateWifis::NONE
            }
        }
    }


    // The following functions can only be safely called after check has succeeded

    pub fn get_work_dir(&'a self) -> &'a Path {
        if let Some(ref dir) = self.work_dir {
            dir
        } else {
            panic!("work_dir is not set");
        }
    }


    pub fn get_mig_mode(&'a self) -> &'a MigMode {
        if let Some(ref mode) = self.mode {
            mode
        } else {
            panic!("migrate mode is not set");
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
}

