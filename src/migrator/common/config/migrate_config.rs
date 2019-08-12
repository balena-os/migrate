use log::error;
use std::path::{Path, PathBuf};

use crate::{
    common::{MigError, MigErrorKind},
    defs::FailMode,
};

use serde::{Deserialize, Serialize};

const MODULE: &str = "common::config::migrate_config";
const NO_NMGR_FILES: &[PathBuf] = &[];

const NO_BACKUP_VOLUMES: &[VolumeConfig] = &[];

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub(crate) enum MigMode {
    Agent,
    Immediate,
    Pretend,
    FSExtract,
    FlashExtract,
}

impl MigMode {
    pub fn from_str(mode: &str) -> Result<Self, MigError> {
        match mode.to_lowercase().as_str() {
            "fsextract" => Ok(MigMode::FSExtract),
            "flashextract" => Ok(MigMode::FlashExtract),
            "immediate" => Ok(MigMode::Immediate),
            "agent" => Ok(MigMode::Agent),
            "pretend" => Ok(MigMode::Pretend),
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

const DEFAULT_MIG_MODE: MigMode = MigMode::Pretend;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct WatchdogCfg {
    pub path: PathBuf,
    pub interval: Option<u64>,
    pub close: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct UBootEnv {
    pub mlo: PathBuf,
    pub image: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ItemConfig {
    pub source: String,
    pub target: Option<String>,
    // TODO: filter.allow, filter.deny
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
    pub console: Option<bool>,
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
    kernel_path: Option<PathBuf>,
    initrd_path: Option<PathBuf>,
    dtb_path: Option<PathBuf>,
    force_slug: Option<String>,
    fail_mode: Option<FailMode>,
    backup: Option<Vec<VolumeConfig>>,
    nwmgr_files: Option<Vec<PathBuf>>,
    require_nwmgr_config: Option<bool>,
    gzip_internal: Option<bool>,
    extract_device: Option<String>,
    watchdogs: Option<Vec<WatchdogCfg>>,
    delay: Option<u64>,
    kernel_opts: Option<String>,
    force_flash_device: Option<PathBuf>,
    uboot_env: Option<UBootEnv>,
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
            kernel_path: None,
            initrd_path: None,
            dtb_path: None,
            force_slug: None,
            fail_mode: None,
            backup: None,
            nwmgr_files: None,
            require_nwmgr_config: None,
            gzip_internal: None,
            extract_device: None,
            watchdogs: None,
            delay: None,
            kernel_opts: None,
            force_flash_device: None,
            uboot_env: None,
        }
    }

    pub fn check(&self) -> Result<(), MigError> {
        match self.get_mig_mode() {
            MigMode::Agent => Err(MigError::from(MigErrorKind::NotImpl)),
            _ => {
                if let None = self.work_dir {
                    error!("A required parameter was not found: 'work_dir'");
                    return Err(MigError::displayed());
                }

                if let None = self.kernel_path {
                    error!("A required parameter was not found: 'kernel_path'");
                    return Err(MigError::displayed());
                }

                if let None = self.initrd_path {
                    error!("A required parameter was not found: 'initrd_path'");
                    return Err(MigError::displayed());
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

    pub fn get_delay(&self) -> u64 {
        if let Some(val) = self.delay {
            val
        } else {
            0
        }
    }

    pub fn get_watchdogs(&'a self) -> Option<&'a Vec<WatchdogCfg>> {
        if let Some(ref val) = self.watchdogs {
            Some(val)
        } else {
            None
        }
    }

    pub fn get_kernel_opts(&self) -> Option<String> {
        if let Some(ref val) = self.kernel_opts {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn get_force_flash_device(&'a self) -> Option<&'a Path> {
        if let Some(ref val) = self.force_flash_device {
            Some(val)
        } else {
            None
        }
    }

    pub fn get_reboot(&'a self) -> &'a Option<u64> {
        &self.reboot
    }

    /*
    pub fn get_force_slug(&self) -> Option<String> {
        if let Some(ref val) = self.force_slug {
            Some(val.clone())
        } else {
            None
        }
    }
    */

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

    pub fn set_extract_device(&mut self, device: &str) {
        self.extract_device = Some(String::from(device));
    }

    pub fn get_extract_device(&'a self) -> Option<&'a str> {
        if let Some(ref device) = self.extract_device {
            Some(device)
        } else {
            None
        }
    }

    pub fn set_work_dir(&mut self, work_dir: PathBuf) {
        self.work_dir = Some(work_dir);
    }

    pub fn has_work_dir(&self) -> bool {
        if let Some(ref _dummy) = self.work_dir {
            true
        } else {
            false
        }
    }

    /*
        pub fn get_uboot_env(&'a self) -> Option<&UBootEnv> {
            if let Some(ref uboot_env) = self.uboot_env {
                Some(uboot_env)
            } else {
                None
            }
        }
    */

    // The following functions can only be safely called after check has succeeded

    pub fn get_work_dir(&'a self) -> &'a Path {
        if let Some(ref dir) = self.work_dir {
            dir
        } else {
            panic!("work_dir is not set");
        }
    }

    pub fn get_kernel_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.kernel_path {
            path
        } else {
            panic!("kernel path is not set");
        }
    }

    pub fn get_initrd_path(&'a self) -> &'a Path {
        if let Some(ref path) = self.initrd_path {
            path
        } else {
            panic!("initramfs path is not set");
        }
    }

    pub fn get_dtb_path(&'a self) -> Option<&'a Path> {
        if let Some(ref path) = self.dtb_path {
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

    pub fn get_log_console(&self) -> bool {
        if let Some(ref log_info) = self.log {
            if let Some(console) = log_info.console {
                return console;
            }
        }
        return false;
    }
}
