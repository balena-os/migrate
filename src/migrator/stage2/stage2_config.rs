use failure::ResultExt;
use log::warn;
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_yaml;

pub const EFI_BOOT_KEY: &str = "efi_boot";
//pub const DRIVE_DEVICE_KEY: &str = "drive_device";
pub const ROOT_DEVICE_KEY: &str = "root_device";
pub const BOOT_DEVICE_KEY: &str = "boot_device";
pub const BOOT_FSTYPE_KEY: &str = "boot_fstype";
pub const EFI_DEVICE_KEY: &str = "efi_device";
pub const EFI_FSTYPE_KEY: &str = "efi_fstype";
pub const FLASH_DEVICE_KEY: &str = "flash_device";
pub const HAS_BACKUP_KEY: &str = "has_backup";
pub const SKIP_FLASH_KEY: &str = "skip_flash";
pub const DEVICE_SLUG_KEY: &str = "device_slug";
pub const BALENA_IMAGE_KEY: &str = "balena_image";
pub const BALENA_CONFIG_KEY: &str = "balena_config";
pub const BOOT_BACKUP_KEY: &str = "boot_bckup";

pub const WORK_DIR_KEY: &str = "work_dir";
pub const FAIL_MODE_KEY: &str = "fail_mode";
pub const NO_FLASH_KEY: &str = "no_flash";

pub const GZIP_INTERNAL_KEY: &str = "gzip_internal";

/*
pub const BBCKUP_SOURCE_KEY: &str = "source";
pub const BBCKUP_BCKUP_KEY: &str = "backup";
*/

pub const EMPTY_BACKUPS: &[(String, String)] = &[];

const MODULE: &str = "stage2::stage2:config";

use crate::{
    common::{Config, FailMode, MigErrCtx, MigError, MigErrorKind},
    defs::STAGE2_CFG_FILE,
    linux_common::MigrateInfo,
};

#[derive(Debug, Deserialize)]
pub(crate) struct Stage2Config {
    efi_boot: bool,
    // what to do on failure
    fail_mode: FailMode,
    // pretend mode stop after unmounting root
    no_flash: bool,
    // skip the flashing, only makes sense with fake / forced flash device
    skip_flash: bool,
    // which device to flash
    flash_device: PathBuf,
    boot_device: PathBuf,
    boot_fstype: String,
    root_device: PathBuf,
    efi_device: Option<PathBuf>,
    efi_fstype: Option<String>,
    device_slug: String,
    balena_config: PathBuf,
    balena_image: PathBuf,
    work_dir: PathBuf,
    boot_bckup: Option<Vec<(String, String)>>,
    has_backup: bool,
    gzip_internal: bool,
}

impl<'a> Stage2Config {
    pub fn write_stage2_cfg(config: &Config, mig_info: &MigrateInfo) -> Result<(), MigError> {
        let mut cfg_str = String::from("# Balena Migrate Stage2 Config\n");

        let fail_mode = config.migrate.get_fail_mode();

        cfg_str.push_str(&format!("{}: '{}'\n", FAIL_MODE_KEY, fail_mode.to_string()));

        cfg_str.push_str(&format!(
            "{}: {}\n",
            NO_FLASH_KEY,
            config.debug.is_no_flash()
        ));

        cfg_str.push_str(&format!(
            "{}: {}\n",
            GZIP_INTERNAL_KEY,
            config.migrate.is_gzip_internal()
        ));

        // allow to configure fake flash device
        if let Some(ref force_flash) = config.debug.get_force_flash_device() {
            warn!("setting up flash device as '{}'", force_flash.display());
            cfg_str.push_str(&format!(
                "{}: {}\n",
                FLASH_DEVICE_KEY,
                &force_flash.to_string_lossy()
            ));

            if config.debug.is_skip_flash() {
                warn!("setting {} to true", SKIP_FLASH_KEY);
            }

            cfg_str.push_str(&format!(
                "{}: {}\n",
                SKIP_FLASH_KEY,
                config.debug.is_skip_flash(),
            ));
        } else {
            cfg_str.push_str(&format!(
                "{}: {}\n",
                FLASH_DEVICE_KEY,
                &mig_info.get_install_path().drive.to_string_lossy()
            ));

            // no skipping when using the real device
            cfg_str.push_str(&format!("{}: false\n", SKIP_FLASH_KEY,));
        }

        cfg_str.push_str(&format!("{}: {}\n", EFI_BOOT_KEY, mig_info.is_efi_boot()));
        cfg_str.push_str(&format!("{}: {}\n", HAS_BACKUP_KEY, mig_info.has_backup));

        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            DEVICE_SLUG_KEY,
            mig_info.get_device_slug()
        ));
        //cfg_str.push_str(&format!(      "{}: '{}'\n", DRIVE_DEVICE_KEY, self.get_drive_device()));
        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            BALENA_IMAGE_KEY,
            mig_info.get_balena_image().to_string_lossy()
        ));
        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            BALENA_CONFIG_KEY,
            mig_info.get_balena_config().to_string_lossy()
        ));
        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            ROOT_DEVICE_KEY,
            mig_info.get_root_device().to_string_lossy()
        ));
        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            BOOT_DEVICE_KEY,
            mig_info.get_boot_device().to_string_lossy()
        ));
        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            BOOT_FSTYPE_KEY,
            mig_info.get_boot_fstype()
        ));
        if mig_info.is_efi_boot() {
            cfg_str.push_str(&format!(
                "{}: '{}'\n",
                EFI_DEVICE_KEY,
                mig_info.get_efi_device().unwrap().to_string_lossy()
            ));
            cfg_str.push_str(&format!(
                "{}: '{}'\n",
                EFI_FSTYPE_KEY,
                mig_info.get_efi_fstype().unwrap()
            ));
        }
        cfg_str.push_str(&format!(
            "{}: '{}'\n",
            WORK_DIR_KEY,
            mig_info.get_work_path().to_string_lossy()
        ));

        cfg_str.push_str("# backed up files in boot config\n");
        if mig_info.boot_cfg_bckup.len() > 0 {
            cfg_str.push_str(&format!("{}:\n", BOOT_BACKUP_KEY));
            for bckup in &mig_info.boot_cfg_bckup {
                cfg_str.push_str(&format!("  - ['{}','{}']\n", bckup.0, bckup.1));
            }
        }

        let mut cfg_file = File::create(STAGE2_CFG_FILE).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to create new stage 2 config file '{}'",
                STAGE2_CFG_FILE
            ),
        ))?;
        cfg_file
            .write_all(cfg_str.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to write new  stage 2 config file '{}'",
                    STAGE2_CFG_FILE
                ),
            ))?;

        Ok(())
    }

    fn from_str(config_str: &str) -> Result<Stage2Config, MigError> {
        Ok(
            serde_yaml::from_str(&config_str).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to parse stage2 config",
            ))?,
        )
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

    pub fn has_backup(&self) -> bool {
        self.has_backup
    }

    pub fn is_no_flash(&self) -> bool {
        self.no_flash
    }

    pub fn is_gzip_internal(&self) -> bool {
        self.gzip_internal
    }

    pub fn is_skip_flash(&self) -> bool {
        self.skip_flash
    }

    pub fn is_efi_boot(&self) -> bool {
        self.efi_boot
    }

    pub fn get_flash_device(&'a self) -> &'a Path {
        self.flash_device.as_path()
    }

    pub fn get_root_device(&'a self) -> &'a Path {
        self.root_device.as_path()
    }

    pub fn get_boot_device(&'a self) -> &'a Path {
        self.boot_device.as_path()
    }

    pub fn get_boot_fstype(&'a self) -> &'a str {
        &self.boot_fstype
    }

    pub fn get_device_slug(&'a self) -> &'a str {
        &self.device_slug
    }

    pub fn get_balena_image(&'a self) -> &'a Path {
        self.balena_image.as_path()
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

    pub fn get_work_path(&'a self) -> &'a Path {
        &self.work_dir
    }

    pub fn set_fail_mode(&mut self, mode: &FailMode) {
        self.fail_mode = mode.clone();
    }

    pub fn get_fail_mode(&'a self) -> &'a FailMode {
        &self.fail_mode
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

    fn assert_test_config1(config: &Config) -> () {
        let config = Stage2Config::from_str(TEST_CONFIG);
    }
}
