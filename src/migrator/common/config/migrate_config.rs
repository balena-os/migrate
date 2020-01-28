use log::error;
use std::path::{Path, PathBuf};

use crate::{
    common::{MigError, MigErrorKind},
    defs::FailMode,
};

use crate::common::config::balena_config::FileRef;
use serde::{Deserialize, Serialize};

const NO_NMGR_FILES: &[PathBuf] = &[];

const NO_BACKUP_VOLUMES: &[VolumeConfig] = &[];

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub(crate) enum MigMode {
    //    #[serde(rename = "agent")]
    //    Agent,
    #[serde(rename = "immediate")]
    Immediate,
    #[serde(rename = "pretend")]
    Pretend,
}

impl MigMode {
    pub fn from_str(mode: &str) -> Result<Self, MigError> {
        match mode.to_lowercase().as_str() {
            "immediate" => Ok(MigMode::Immediate),
            //            "agent" => Ok(MigMode::Agent),
            "pretend" => Ok(MigMode::Pretend),
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("new: invalid value for parameter mode: '{}'", mode),
            )),
        }
    }
}

const DEFAULT_MIG_MODE: MigMode = MigMode::Pretend;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub(crate) enum UEnvStrategy {
    #[serde(rename = "uname")]
    UName,
    #[serde(rename = "manual")]
    Manual,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct UBootCfg {
    pub strategy: Option<UEnvStrategy>,
    pub mmc_index: Option<u8>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct WatchdogCfg {
    pub path: PathBuf,
    pub interval: Option<u64>,
    pub close: Option<bool>,
}

/*
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct UBootEnv {
    pub mlo: PathBuf,
    pub image: PathBuf,
}
*/

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
    None,
    All,
    List(Vec<String>),
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) enum DeviceSpec {
    #[serde(rename = "uuid")]
    Uuid(String),
    #[serde(rename = "partuuid")]
    PartUuid(String),
    #[serde(rename = "devpath")]
    DevicePath(PathBuf),
    #[serde(rename = "path")]
    Path(PathBuf),
    #[serde(rename = "label")]
    Label(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct LogConfig {
    pub console: Option<bool>,
    pub level: Option<String>,
    pub drive: Option<DeviceSpec>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MigrateConfig {
    work_dir: Option<PathBuf>,
    mode: Option<MigMode>,
    reboot: Option<u64>,
    all_wifis: Option<bool>,
    wifis: Option<Vec<String>>,
    log: Option<LogConfig>,
    kernel: Option<FileRef>,
    initrd: Option<FileRef>,
    device_tree: Option<Vec<FileRef>>,
    // TODO: check fail mode processing
    fail_mode: Option<FailMode>,
    backup: Option<Vec<VolumeConfig>>,
    // TODO: find a good way to do digests on NetworkManager files
    nwmgr_files: Option<Vec<PathBuf>>,
    require_nwmgr_config: Option<bool>,
    gzip_internal: Option<bool>,
    tar_internal: Option<bool>,
    watchdogs: Option<Vec<WatchdogCfg>>,
    delay: Option<u64>,
    kernel_opts: Option<String>,
    force_flash_device: Option<PathBuf>,
    uboot: Option<UBootCfg>,
}

impl<'a> MigrateConfig {
    pub fn default() -> MigrateConfig {
        MigrateConfig {
            work_dir: None,
            mode: Some(DEFAULT_MIG_MODE.clone()),
            reboot: None,
            all_wifis: None,
            wifis: None,
            log: None,
            kernel: None,
            initrd: None,
            device_tree: None,
            fail_mode: None,
            backup: None,
            nwmgr_files: None,
            require_nwmgr_config: None,
            gzip_internal: None,
            tar_internal: None,
            watchdogs: None,
            delay: None,
            kernel_opts: None,
            force_flash_device: None,
            uboot: None,
        }
    }

    pub fn check(&self) -> Result<(), MigError> {
        if let Some(ref uboot_cfg) = self.uboot {
            if let Some(mmc_index) = uboot_cfg.mmc_index {
                if mmc_index != 0 && mmc_index != 1 {
                    error!("mmc_index must be 0, 1, or undefined, found {}", mmc_index);
                    return Err(MigError::displayed());
                }
            }
        }

        match self.get_mig_mode() {
            //MigMode::Agent => Err(MigError::from(MigErrorKind::NotImpl)),
            _ => {
                if self.work_dir.is_none() {
                    error!("A required parameter was not found: 'work_dir'");
                    return Err(MigError::displayed());
                }

                if self.kernel.is_none() {
                    error!("A required parameter was not found: 'kernel_path'");
                    return Err(MigError::displayed());
                }

                if self.initrd.is_none() {
                    error!("A required parameter was not found: 'initrd_path'");
                    return Err(MigError::displayed());
                }

                Ok(())
            }
        }
    }

    // defaults are implemented in getter functions

    pub fn is_gzip_internal(&self) -> bool {
        if let Some(val) = self.gzip_internal {
            val
        } else {
            true
        }
    }

    #[cfg(target_os = "linux")]
    pub fn is_tar_internal(&self) -> bool {
        if let Some(val) = self.tar_internal {
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
        true
    }

    pub fn get_nwmgr_files(&'a self) -> &'a [PathBuf] {
        if let Some(ref val) = self.nwmgr_files {
            return val.as_slice();
        }
        NO_NMGR_FILES
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

    pub fn get_uboot_cfg(&'a self) -> Option<&'a UBootCfg> {
        if let Some(ref val) = self.uboot {
            Some(val)
        } else {
            None
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

    pub fn get_force_flash_device(&self) -> Option<PathBuf> {
        if let Some(ref val) = self.force_flash_device {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn get_reboot(&'a self) -> &'a Option<u64> {
        &self.reboot
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
            MigrateWifis::List(wifis.clone())
        } else if let Some(ref all_wifis) = self.all_wifis {
            if *all_wifis {
                MigrateWifis::All
            } else {
                MigrateWifis::None
            }
        } else {
            MigrateWifis::None
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

    pub fn get_dtb_refs(&'a self) -> Option<&'a Vec<FileRef>> {
        if let Some(ref path) = self.device_tree {
            Some(path)
        } else {
            None
        }
    }

    pub fn get_log_device(&'a self) -> Option<&'a DeviceSpec> {
        if let Some(ref log_info) = self.log {
            if let Some(ref val) = log_info.drive {
                return Some(val);
            }
        }
        None
    }

    pub fn get_log_level(&'a self) -> &'a str {
        if let Some(ref log_info) = self.log {
            if let Some(ref val) = log_info.level {
                return val;
            }
        }
        "warn"
    }

    pub fn get_log_console(&self) -> bool {
        if let Some(ref log_info) = self.log {
            if let Some(console) = log_info.console {
                return console;
            }
        }
        false
    }

    // The following functions can only be safely called after check has succeeded

    pub fn get_work_dir(&'a self) -> &'a Path {
        if let Some(ref dir) = self.work_dir {
            dir
        } else {
            panic!("work_dir is not set");
        }
    }

    pub fn get_kernel_path(&'a self) -> &'a FileRef {
        if let Some(ref path) = self.kernel {
            path
        } else {
            panic!("kernel path is not set");
        }
    }

    pub fn get_initrd_path(&'a self) -> &'a FileRef {
        if let Some(ref path) = self.initrd {
            path
        } else {
            panic!("initramfs path is not set");
        }
    }
}
