use failure::ResultExt;
use log::{info, Level};
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_yaml;

pub const EMPTY_BACKUPS: &[(String, String)] = &[];

const MODULE: &str = "stage2::stage2:config";

use crate::{
    common::{config::balena_config::FSDump, MigErrCtx, MigError, MigErrorKind},
    defs::{BootType, DeviceType, FailMode},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum CheckedImageType {
    Flasher(PathBuf),
    FileSystems(FSDump),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct ImageInfo {
    pub req_space: u64,
    pub image: CheckedImageType,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct Stage2LogConfig {
    pub device: PathBuf,
    pub fstype: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct MountConfig {
    // the device to mount
    device: PathBuf,
    // the devices to fs_type
    fstype: String,
    // Path to target directory in mount
    path: PathBuf,
}

impl<'a> MountConfig {
    pub fn new(device: &Path, fstype: &str, path: &Path) -> MountConfig {
        MountConfig {
            device: PathBuf::from(device),
            fstype: String::from(fstype),
            path: PathBuf::from(path),
        }
    }

    pub fn get_device(&'a self) -> &'a Path {
        &self.device.as_path()
    }
    pub fn get_fstype(&'a self) -> &'a str {
        &self.fstype
    }
    pub fn get_path(&'a self) -> &'a Path {
        &self.path.as_path()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum PathType {
    Path(PathBuf),
    Mount(MountConfig),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Stage2Config {
    // what to do on failure
    fail_mode: FailMode,
    // no_flash mode - stop after unmounting root if true
    no_flash: bool,
    // which device to flash - derive from /root partition if not set (windows)
    flash_device: Option<PathBuf>,
    // balena config file
    balena_config: PathBuf,
    // balena OS image file in work_path
    balena_image: ImageInfo,
    // working directory  in path on root or mount partition
    work_path: PathType,
    // backed up former boot configuration (from , to) expected in boot manager
    boot_bckup: Option<Vec<(String, String)>>,
    // backup present in work_dir/backup.tgz
    has_backup: bool,
    // use rust internal gzip
    gzip_internal: bool,
    // stage 2 log level
    log_level: String,
    // stage 2 log destination
    log_to: Option<Stage2LogConfig>,
    // device type
    device_type: DeviceType,
    // boot type
    boot_type: BootType,
}

impl<'a> Stage2Config {
    fn from_str(config_str: &str) -> Result<Stage2Config, MigError> {
        Ok(
            serde_yaml::from_str(&config_str).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to parse stage2 config",
            ))?,
        )
    }

    fn to_str(&self) -> Result<String, MigError> {
        Ok(serde_yaml::to_string(self).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to serialize stage2 config",
        ))?)
    }

    pub fn from_config<P: AsRef<Path>>(path: &P) -> Result<Stage2Config, MigError> {
        // TODO: Dummy, parse from yaml
        let config_str = read_to_string(path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::from_config: failed to read stage2_config from file: '{}'",
                MODULE,
                path.as_ref().display()
            ),
        ))?;

        Stage2Config::from_str(&config_str)
    }

    pub fn get_log_level(&self) -> Level {
        if let Ok(level) = Level::from_str(&self.log_level) {
            level
        } else {
            Level::Debug
        }
    }

    pub fn get_log_device(&'a self) -> Option<(&'a Path, &'a str)> {
        if let Some(ref log_to) = self.log_to {
            Some((&log_to.device, &log_to.fstype))
        } else {
            None
        }
    }

    pub fn has_backup(&self) -> bool {
        self.has_backup
    }

    pub fn is_no_flash(&self) -> bool {
        self.no_flash
    }

    pub fn is_gzip_internal(&self) -> bool {
        self.gzip_internal
    }

    pub fn get_flash_device(&'a self) -> Option<&'a PathBuf> {
        if let Some(ref flash_device) = self.flash_device {
            Some(flash_device)
        } else {
            None
        }
    }

    pub fn get_boot_type(&'a self) -> &'a BootType {
        &self.boot_type
    }

    pub fn get_device_type(&'a self) -> &'a DeviceType {
        &self.device_type
    }

    pub fn get_balena_image(&'a self) -> &'a ImageInfo {
        &self.balena_image
    }

    pub fn get_balena_config(&'a self) -> &'a Path {
        self.balena_config.as_path()
    }

    pub fn get_boot_backups(&'a self) -> &'a [(String, String)] {
        if let Some(ref boot_bckup) = self.boot_bckup {
            boot_bckup.as_slice()
        } else {
            EMPTY_BACKUPS
        }
    }

    pub fn get_work_path(&'a self) -> &'a PathType {
        &self.work_path
    }

    pub fn get_fail_mode(&'a self) -> &'a FailMode {
        &self.fail_mode
    }
}

pub(crate) struct Required<T> {
    name: String,
    data: Option<T>,
}

impl<T: Clone> Required<T> {
    pub fn new(name: &str, default: Option<&T>) -> Required<T> {
        Required {
            name: String::from(name),
            data: if let Some(default) = default {
                Some(default.clone())
            } else {
                None
            },
        }
    }

    fn get<'a>(&'a self) -> Result<&'a T, MigError> {
        if let Some(ref val) = self.data {
            Ok(val)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("A required parameters was not initialized: '{}'", self.name),
            ))
        }
    }

    fn set(&mut self, val: T) {
        self.data = Some(val);
    }
    fn set_ref(&mut self, val: &T) {
        self.data = Some(val.clone());
    }
}

pub(crate) struct Optional<T> {
    data: Option<T>,
}

impl<T: Clone> Optional<T> {
    pub fn new(default: Option<&T>) -> Optional<T> {
        Optional {
            data: if let Some(default) = default {
                Some(default.clone())
            } else {
                None
            },
        }
    }

    fn get<'a>(&'a self) -> &'a Option<T> {
        &self.data
    }

    fn set(&mut self, val: T) {
        self.data = Some(val);
    }

    fn set_ref(&mut self, val: &T) {
        self.data = Some(val.clone());
    }
}

pub(crate) struct Stage2ConfigBuilder {
    fail_mode: Required<FailMode>,
    no_flash: Required<bool>,
    flash_device: Optional<PathBuf>,
    balena_config: Required<PathBuf>,
    balena_image: Required<ImageInfo>,
    work_path: Required<PathType>,
    boot_bckup: Optional<Vec<(String, String)>>,
    has_backup: Required<bool>,
    gzip_internal: Required<bool>,
    log_level: Required<String>,
    log_to: Optional<Stage2LogConfig>,
    device_type: Required<DeviceType>,
    boot_type: Required<BootType>,
}

impl<'a> Stage2ConfigBuilder {
    pub fn default() -> Stage2ConfigBuilder {
        Stage2ConfigBuilder {
            fail_mode: Required::new("fail_mode", Some(&FailMode::Reboot)),
            no_flash: Required::new("no_flash", Some(&true)),
            flash_device: Optional::new(None),
            balena_config: Required::new("balena_config", None),
            balena_image: Required::new("balena_image", None),
            work_path: Required::new("work_path", None),
            boot_bckup: Optional::new(None),
            has_backup: Required::new("has_backup", None),
            gzip_internal: Required::new("gzip_internal", Some(&true)),
            log_level: Required::new("log_level", Some(&String::from("warn"))),
            log_to: Optional::new(None),
            device_type: Required::new("device_type", None),
            boot_type: Required::new("boot_type", None),
        }
    }

    pub fn build(&self) -> Result<Stage2Config, MigError> {
        let result = Stage2Config {
            fail_mode: self.fail_mode.get()?.clone(),
            no_flash: self.no_flash.get()?.clone(),
            flash_device: self.flash_device.get().clone(),
            balena_config: self.balena_config.get()?.clone(),
            balena_image: self.balena_image.get()?.clone(),
            work_path: self.work_path.get()?.clone(),
            boot_bckup: self.boot_bckup.get().clone(),
            has_backup: *self.has_backup.get()?,
            gzip_internal: *self.gzip_internal.get()?,
            log_level: self.log_level.get()?.clone(),
            log_to: self.log_to.get().clone(),
            device_type: self.device_type.get()?.clone(),
            boot_type: self.boot_type.get()?.clone(),
        };

        Ok(result)
    }

    /*    pub fn write_stage2_cfg(&self) -> Result<(), MigError> {
            self.write_stage2_cfg_to(&Path::new(STAGE2_CFG_FILE))
        }
    */
    pub fn write_stage2_cfg_to(&self, file: &Path) -> Result<(), MigError> {
        // TODO: check first

        let mut cfg_str = String::from("# Balena Migrate Stage2 Config\n");
        cfg_str.push_str("# auto-created by balena migrate - do not edit\n");
        cfg_str.push_str(&self.build()?.to_str()?);
        File::create(file)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to open file for writing: {}'", file.display()),
            ))?
            .write_all(cfg_str.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to write to config file: {}'", file.display()),
            ))?;
        info!("Wrote stage2 config to '{}'", file.display());
        Ok(())
    }

    // *****************************************************************
    // Setter functions

    pub fn set_failmode(&mut self, val: &FailMode) {
        self.fail_mode.set_ref(val);
    }

    pub fn set_no_flash(&mut self, val: bool) {
        self.no_flash.set(val);
    }

    pub fn set_flash_device(&mut self, val: PathBuf) {
        self.flash_device.set(val);
    }

    pub fn set_balena_config(&mut self, val: PathBuf) {
        self.balena_config.set(val);
    }

    pub fn set_balena_image(&mut self, val: ImageInfo) {
        self.balena_image.set(val);
    }

    pub fn set_work_path(&mut self, val: &PathType) {
        self.work_path.set_ref(val);
    }

    pub fn set_boot_bckup(&mut self, boot_backup: Vec<(String, String)>) {
        self.boot_bckup.set(boot_backup);
    }

    pub fn set_has_backup(&mut self, val: bool) {
        self.has_backup.set(val);
    }

    pub fn set_gzip_internal(&mut self, val: bool) {
        self.gzip_internal.set(val);
    }

    pub fn set_device_type(&mut self, dev_type: &DeviceType) {
        self.device_type.set_ref(dev_type);
    }

    pub fn set_log_level(&mut self, val: String) {
        self.log_level.set(val);
    }

    pub fn set_log_to(&mut self, val: Stage2LogConfig) {
        self.log_to.set(val);
    }

    pub fn set_boot_type(&mut self, val: &BootType) {
        self.boot_type.set_ref(val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONFIG: &str = r##"
migrate:
  mode: IMMEDIATE
  all_wifis: true
  reboot: 10
  log_to:
    drive: '/dev/sda1'
    fs_type: ext4
balena:
  image: image.gz
  config: config.json
"##;

    #[test]
    fn assert_test_config1() -> () {
        let _config = Stage2Config::from_str(TEST_CONFIG);
    }
}
