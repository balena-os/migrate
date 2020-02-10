use failure::ResultExt;
use log::{debug, info, Level};
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_yaml;

const EMPTY_BACKUPS: &[BackupCfg] = &[];

use crate::defs::DeviceSpec;
use crate::linux::lsblk_info::partition::Partition;

use crate::common::device_info::DeviceInfo;
use crate::{
    common::{
        config::balena_config::PartCheck, file_info::RelFileInfo, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType, FailMode},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct CheckedPartDump {
    pub blocks: u64,
    pub archive: RelFileInfo,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct CheckedFSDump {
    pub extended_blocks: u64,
    pub device_slug: String,
    pub check: Option<PartCheck>,
    pub max_data: Option<bool>,
    pub mkfs_direct: Option<bool>,
    pub boot: CheckedPartDump,
    pub root_a: CheckedPartDump,
    pub root_b: CheckedPartDump,
    pub state: CheckedPartDump,
    pub data: CheckedPartDump,
}

#[allow(clippy::large_enum_variant)] //TODO refactor to remove clippy warning
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum CheckedImageType {
    Flasher(RelFileInfo),
    FileSystems(CheckedFSDump),
}

impl CheckedImageType {
    pub fn get_required_space(&self) -> u64 {
        match self {
            CheckedImageType::Flasher(ref flasher) => flasher.size,
            CheckedImageType::FileSystems(ref file_systems) => {
                file_systems.boot.archive.size
                    + file_systems.root_a.archive.size
                    + file_systems.root_b.archive.size
                    + file_systems.state.archive.size
                    + file_systems.data.archive.size
            }
        }
    }
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

    #[allow(dead_code)]
    pub fn get_device(&'a self) -> &'a Path {
        &self.device.as_path()
    }

    #[allow(dead_code)]
    pub fn get_fstype(&'a self) -> &'a str {
        &self.fstype
    }

    #[allow(dead_code)]
    pub fn get_path(&'a self) -> &'a Path {
        &self.path.as_path()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct LogDevice {
    pub device: PathBuf,
    pub fs_type: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct BackupCfg {
    pub device: DeviceSpec,
    pub source: PathBuf,
    pub backup: PathBuf,
}

impl BackupCfg {
    pub fn from_partition(device: Partition, source: &Path, backup: &Path) -> BackupCfg {
        BackupCfg {
            device: if let Some(uuid) = device.uuid {
                DeviceSpec::Uuid(uuid)
            } else if let Some(partuuid) = device.partuuid {
                DeviceSpec::PartUuid(partuuid)
            } else if let Some(label) = device.label {
                DeviceSpec::Label(label)
            } else {
                DeviceSpec::DevicePath(device.get_path())
            },
            source: source.to_path_buf(),
            backup: backup.to_path_buf(),
        }
    }

    pub fn from_device_info(device: &DeviceInfo, source: &Path, backup: &Path) -> BackupCfg {
        BackupCfg {
            device: if let Some(ref uuid) = device.uuid {
                DeviceSpec::Uuid(uuid.clone())
            } else if let Some(ref partuuid) = device.part_uuid {
                DeviceSpec::PartUuid(partuuid.clone())
            } else if let Some(ref label) = device.part_label {
                DeviceSpec::Label(label.clone())
            } else {
                DeviceSpec::DevicePath(PathBuf::from(&device.device))
            },
            source: source.to_path_buf(),
            backup: backup.to_path_buf(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum PathType {
    Path(PathBuf),
    Mount(MountConfig),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct UbootMbrBackup {
    pub mlo_backup: PathBuf,
    pub uboot_backup: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Stage2Config {
    // what to do on failure
    fail_mode: FailMode,
    // no_flash mode - stop after unmounting root if true
    no_flash: bool,
    // which device to flash - derive from /root partition if not set (windows)
    force_flash_device: Option<PathBuf>,
    // free form debug parameters
    hacks: Option<Vec<String>>,
    // balena config file
    balena_config: PathBuf,
    // balena OS image file in work_path
    balena_image: CheckedImageType,
    // working directory  in path on root or mount partition
    work_path: PathType,
    // backed up former boot configuration (from , to) expected in boot manager
    boot_bckup: Option<Vec<BackupCfg>>,
    // backup present in work_dir/backup.tgz
    has_backup: bool,
    // use rust internal gzip
    gzip_internal: bool,
    // stage 2 log level
    log_level: String,
    // stage 2 log destination
    log_to: Option<LogDevice>,
    // log also to console
    log_console: bool,
    // device type
    device_type: DeviceType,
    // boot type
    boot_type: BootType,
    // delay migration in stage 2
    migrate_delay: Option<u64>,
    // mlo & uboot backup - files extracted from MBR to be restored n stage2
    uboot_mbr_backup: Option<UbootMbrBackup>,
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

    #[allow(dead_code)]
    pub fn from_config<P: AsRef<Path>>(path: &P) -> Result<Stage2Config, MigError> {
        // TODO: Dummy, parse from yaml
        let config_str = read_to_string(path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "from_config: failed to read stage2_config from file: '{}'",
                path.as_ref().display()
            ),
        ))?;

        Stage2Config::from_str(&config_str)
    }

    #[allow(dead_code)]
    pub fn is_log_console(&self) -> bool {
        self.log_console
    }

    #[allow(dead_code)]
    pub fn get_log_level(&self) -> Level {
        if let Ok(level) = Level::from_str(&self.log_level) {
            level
        } else {
            Level::Debug
        }
    }

    #[allow(dead_code)]
    pub fn get_hacks(&'a self) -> Option<&'a Vec<String>> {
        if let Some(ref hacks) = self.hacks {
            Some(hacks)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn get_log_device(&'a self) -> Option<&'a LogDevice> {
        if let Some(ref log_to) = self.log_to {
            Some(&log_to)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn has_backup(&self) -> bool {
        self.has_backup
    }

    #[allow(dead_code)]
    pub fn is_no_flash(&self) -> bool {
        self.no_flash
    }

    #[allow(dead_code)]
    pub fn is_gzip_internal(&self) -> bool {
        self.gzip_internal
    }

    #[allow(dead_code)]
    pub fn get_force_flash_device(&'a self) -> Option<&'a PathBuf> {
        if let Some(ref flash_device) = self.force_flash_device {
            Some(flash_device)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn get_migrate_delay(&self) -> u64 {
        if let Some(val) = self.migrate_delay {
            val
        } else {
            0
        }
    }

    #[allow(dead_code)]
    pub fn get_boot_type(&'a self) -> &'a BootType {
        &self.boot_type
    }

    #[allow(dead_code)]
    pub fn get_device_type(&'a self) -> &'a DeviceType {
        &self.device_type
    }

    #[allow(dead_code)]
    pub fn get_balena_image(&'a self) -> &'a CheckedImageType {
        &self.balena_image
    }

    #[allow(dead_code)]
    pub fn get_balena_config(&'a self) -> &'a Path {
        self.balena_config.as_path()
    }

    pub fn get_boot_backups(&'a self) -> &'a [BackupCfg] {
        if let Some(ref boot_bckup) = self.boot_bckup {
            boot_bckup.as_slice()
        } else {
            EMPTY_BACKUPS
        }
    }

    #[allow(dead_code)]
    pub fn get_work_path(&'a self) -> &'a PathType {
        &self.work_path
    }

    #[allow(dead_code)]
    pub fn get_fail_mode(&'a self) -> &'a FailMode {
        &self.fail_mode
    }

    pub fn get_uboot_mbr_backup(&'a self) -> &Option<UbootMbrBackup> {
        &self.uboot_mbr_backup
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

    fn get(&self) -> Result<&T, MigError> {
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

    fn get(&'_ self) -> &'_ Option<T> {
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
    force_flash_device: Optional<PathBuf>,
    balena_config: Required<PathBuf>,
    balena_image: Required<CheckedImageType>,
    work_path: Required<PathType>,
    boot_bckup: Optional<Vec<BackupCfg>>,
    has_backup: Required<bool>,
    gzip_internal: Required<bool>,
    log_level: Required<String>,
    log_to: Optional<LogDevice>,
    log_console: Required<bool>,
    device_type: Required<DeviceType>,
    boot_type: Required<BootType>,
    migrate_delay: Optional<u64>,
    hacks: Optional<Vec<String>>,
    uboot_mbr_backup: Optional<UbootMbrBackup>,
}

impl<'a> Stage2ConfigBuilder {
    pub fn default() -> Stage2ConfigBuilder {
        Stage2ConfigBuilder {
            fail_mode: Required::new("fail_mode", Some(&FailMode::Reboot)),
            no_flash: Required::new("no_flash", Some(&true)),
            force_flash_device: Optional::new(None),
            balena_config: Required::new("balena_config", None),
            balena_image: Required::new("balena_image", None),
            work_path: Required::new("work_path", None),
            boot_bckup: Optional::new(None),
            has_backup: Required::new("has_backup", None),
            gzip_internal: Required::new("gzip_internal", Some(&true)),
            log_level: Required::new("log_level", Some(&String::from("warn"))),
            log_to: Optional::new(None),
            log_console: Required::new("log_console", Some(&false)),
            device_type: Required::new("device_type", None),
            boot_type: Required::new("boot_type", None),
            migrate_delay: Optional::new(None),
            hacks: Optional::new(None),
            uboot_mbr_backup: Optional::new(None),
        }
    }

    pub fn build(&self) -> Result<Stage2Config, MigError> {
        let result = Stage2Config {
            fail_mode: self.fail_mode.get()?.clone(),
            no_flash: *self.no_flash.get()?,
            force_flash_device: self.force_flash_device.get().clone(),
            balena_config: self.balena_config.get()?.clone(),
            balena_image: self.balena_image.get()?.clone(),
            work_path: self.work_path.get()?.clone(),
            boot_bckup: self.boot_bckup.get().clone(),
            has_backup: *self.has_backup.get()?,
            gzip_internal: *self.gzip_internal.get()?,
            log_level: self.log_level.get()?.clone(),
            log_to: self.log_to.get().clone(),
            log_console: *self.log_console.get()?,
            device_type: *self.device_type.get()?,
            boot_type: *self.boot_type.get()?,
            migrate_delay: *self.migrate_delay.get(),
            hacks: self.hacks.get().clone(),
            uboot_mbr_backup: self.uboot_mbr_backup.get().clone(),
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

        debug!("write_stage2_cfg_to: config: '{}'", cfg_str);

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

    pub fn set_force_flash_device(&mut self, val: PathBuf) {
        self.force_flash_device.set(val);
    }

    pub fn set_balena_config(&mut self, val: PathBuf) {
        self.balena_config.set(val);
    }

    pub fn set_balena_image(&mut self, val: CheckedImageType) {
        self.balena_image.set(val);
    }

    pub fn set_work_path(&mut self, val: &PathType) {
        self.work_path.set_ref(val);
    }

    #[allow(clippy::ptr_arg)]
    pub fn set_hacks(&mut self, val: &Vec<String>) {
        self.hacks.set_ref(val);
    }

    #[allow(dead_code)]
    pub fn set_boot_bckup(&mut self, boot_backup: Vec<BackupCfg>) {
        self.boot_bckup.set(boot_backup);
    }

    #[allow(dead_code)]
    pub fn set_has_backup(&mut self, val: bool) -> bool {
        self.has_backup.set(val);
        val
    }

    #[cfg(target_os = "windows")]
    pub fn get_has_backup(&self) -> bool {
        if let Ok(has_backup) = self.has_backup.get() {
            *has_backup
        } else {
            false
        }
    }

    pub fn set_gzip_internal(&mut self, val: bool) {
        self.gzip_internal.set(val);
    }

    pub fn set_device_type(&mut self, dev_type: DeviceType) {
        self.device_type.set(dev_type);
    }

    pub fn set_log_level(&mut self, val: String) {
        self.log_level.set(val);
    }

    pub fn set_log_to(&mut self, val: LogDevice) {
        self.log_to.set(val);
    }

    pub fn set_log_console(&mut self, val: bool) {
        self.log_console.set(val);
    }

    pub fn set_boot_type(&mut self, val: BootType) {
        self.boot_type.set(val);
    }

    pub fn set_migrate_delay(&mut self, val: u64) {
        self.migrate_delay.set_ref(&val);
    }

    pub fn set_uboot_mbr_backup(&mut self, val: UbootMbrBackup) {
        self.uboot_mbr_backup.set_ref(&val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONFIG: &str = r##"
fail_mode: Reboot
no_flash: true
force_flash_device: ~
balena_config: config.json
balena_image:
  req_space: 139522865
  image:
    Flasher:
      path: balena-cloud-intel-nuc-2.38.3+rev5-v9.15.7.img.gz
      hash:
        Md5: c55a19eacc425c3e75a007ae4249b85d
work_path:
  Path: /home/thomas/migrate
boot_bckup: ~
has_backup: false
gzip_internal: true
log_level: debug
log_to:
  device: /dev/sdb1
  fstype: vfat
log_console: false
device_type: IntelNuc
boot_type: Grub
migrate_delay: 0
watchdogs: ~'
"##;

    #[test]
    fn assert_test_config1() {
        let _config = Stage2Config::from_str(TEST_CONFIG);
    }
}
