use std::path::{Path, PathBuf};

use crate::common::{FailMode, MigError, MigErrorKind};

use serde::Deserialize;

const MODULE: &str = "common::config::migrate_config";
const NO_NMGR_FILES: &[PathBuf] = &[];

const NO_BACKUP_VOLUMES: &[VolumeConfig] = &[];

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

const DEFAULT_MIG_MODE: MigMode = MigMode::PRETEND;

#[derive(Debug, Deserialize)]
pub(crate) struct ItemConfig {
    pub source: String,
    pub target: Option<String>,
    pub filter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VolumeConfig {
    pub volume: String,
    pub items: Vec<ItemConfig>,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum MigrateWifis {
    NONE,
    ALL,
    SOME(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct LogConfig {
    pub level: Option<String>,
    pub drive: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MigrateConfig {
    work_dir: Option<PathBuf>,
    mode: Option<MigMode>,
    reboot: Option<u64>,
    all_wifis: Option<bool>,
    wifis: Option<Vec<String>>,
    log: Option<LogConfig>,
    kernel_file: Option<PathBuf>,
    initramfs_file: Option<PathBuf>,
    dtb_file: Option<PathBuf>,
    force_slug: Option<String>,
    fail_mode: Option<FailMode>,
    backup: Option<Vec<VolumeConfig>>,
    nwmgr_files: Option<Vec<PathBuf>>,
    require_nwmgr_config: Option<bool>,
    gzip_internal: Option<bool>,
    // COPY_NMGR_FILES="eth0_static enp2s0_static enp3s0_static"
}

impl<'a> MigrateConfig {
    // TODO: implement log & backup config getters

    pub fn default() -> MigrateConfig {
        MigrateConfig {
            work_dir: None,
            mode: Some(DEFAULT_MIG_MODE.clone()),
            reboot: None,
            all_wifis: None,
            wifis: None,
            log: None,
            kernel_file: None,
            initramfs_file: None,
            dtb_file: None,
            force_slug: None,
            fail_mode: None,
            backup: None,
            nwmgr_files: None,
            require_nwmgr_config: None,
            gzip_internal: None,
        }
    }

    pub fn check(&self) -> Result<(), MigError> {
        match self.get_mig_mode() {
            MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
            _ => {
                if let None = self.work_dir {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        "A required parameter was not found: 'work_dir'",
                    ));
                }

                if let None = self.kernel_file {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        "A required parameter was not found: 'kernel_file'",
                    ));
                }

                if let None = self.initramfs_file {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        "A required parameter was not found: 'initramfs_file'",
                    ));
                }

                Ok(())
            }
        }
    }

    pub fn is_gzip_internal(&self) -> bool {
        if let Some(val) = self.gzip_internal {
            val
        } else {
            true
        }
    }

    pub fn get_backup_volumes(&'a self) -> &'a [VolumeConfig] {
        if let Some(ref val) = self.backup {
            val.as_ref()
        } else {
            NO_BACKUP_VOLUMES
        }
    }

    pub fn require_nwmgr_configs(&self) -> bool {
        if let Some(val) = self.require_nwmgr_config {
            return val;
        }
        return true;
    }

    pub fn get_nwmgr_files(&'a self) -> &'a [PathBuf] {
        if let Some(ref val) = self.nwmgr_files {
            return val.as_ref();
        }
        return NO_NMGR_FILES;
    }

    pub fn set_mig_mode(&mut self, mode: &MigMode) {
        self.mode = Some(mode.clone());
    }

    pub fn get_mig_mode(&'a self) -> &'a MigMode {
        if let Some(ref mode) = self.mode {
            mode
        } else {
            &DEFAULT_MIG_MODE
        }
    }

    pub fn get_reboot(&'a self) -> &'a Option<u64> {
        &self.reboot
    }

    pub fn get_force_slug(&self) -> Option<String> {
        if let Some(ref val) = self.force_slug {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn get_fail_mode(&'a self) -> &'a FailMode {
        if let Some(ref val) = self.fail_mode {
            val
        } else {
            FailMode::get_default()
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

    pub fn set_work_dir(&mut self, work_dir: PathBuf) {
        self.work_dir = Some(work_dir);
    }

    pub fn get_work_dir(&'a self) -> &'a Path {
        if let Some(ref dir) = self.work_dir {
            dir
        } else {
            panic!("work_dir is not set");
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

    pub fn get_dtb_path(&'a self) -> Option<&'a Path> {
        if let Some(ref path) = self.dtb_file {
            Some(path)
        } else {
            None
        }
    }

    pub fn get_log_device(&'a self) -> Option<&'a Path> {
        if let Some(ref log_info) = self.log {
            if let Some(ref val) = log_info.drive {
                return Some(val);
            }
        }
        return None;
    }

    pub fn get_log_level(&'a self) -> &'a str {
        if let Some(ref log_info) = self.log {
            if let Some(ref val) = log_info.level {
                return val;
            }
        }
        return "warn";
    }
}
