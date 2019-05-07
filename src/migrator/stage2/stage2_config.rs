use failure::ResultExt;
use log::warn;
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use yaml_rust::{Yaml, YamlLoader};

pub const EFI_BOOT_KEY: &str = "efi_boot";
//pub const DRIVE_DEVICE_KEY: &str = "drive_device";
pub const ROOT_DEVICE_KEY: &str = "root_device";
pub const BOOT_DEVICE_KEY: &str = "boot_device";
pub const BOOT_FSTYPE_KEY: &str = "boot_fstype";
pub const EFI_DEVICE_KEY: &str = "efi_device";
pub const EFI_FSTYPE_KEY: &str = "efi_fstype";
pub const FLASH_DEVICE_KEY: &str = "flash_device";
pub const SKIP_FLASH_KEY: &str = "skip_flash";
pub const DEVICE_SLUG_KEY: &str = "device_slug";
pub const BALENA_IMAGE_KEY: &str = "balena_image";
pub const BALENA_CONFIG_KEY: &str = "balena_config";
pub const BACKUP_CONFIG_KEY: &str = "backup_config";

pub const WORK_DIR_KEY: &str = "work_dir";
pub const FAIL_MODE_KEY: &str = "fail_mode";
pub const NO_FLASH_KEY: &str = "no_flash";

pub const BACKUP_ORIG_KEY: &str = "orig";
pub const BACKUP_BCKUP_KEY: &str = "bckup";

const MODULE: &str = "stage2::stage2:config";

use crate::{
    common::{
        config_helper::{get_yaml_bool, get_yaml_str, get_yaml_val},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::STAGE2_CFG_FILE,
    linux_common::{FailMode, MigrateInfo},
};

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
    bckup_cfg: Vec<(String, String)>,
}

impl<'a> Stage2Config {
    pub fn write_stage2_cfg(config: &Config, mig_info: &MigrateInfo) -> Result<(), MigError> {
        let mut cfg_str = String::from("# Balena Migrate Stage2 Config\n");

        let fail_mode = if let Some(ref fail_mode) = config.migrate.fail_mode {
            fail_mode
        } else {
            FailMode::get_default()
        };

        cfg_str.push_str(&format!("{}: '{}'\n", FAIL_MODE_KEY, fail_mode.to_string()));

        cfg_str.push_str(&format!("{}: {}\n", NO_FLASH_KEY, config.debug.no_flash));

        // allow to configure fake flash device
        if let Some(ref force_flash) = config.debug.force_flash_device {
            warn!("setting up flash device as '{}'", force_flash.display());
            cfg_str.push_str(&format!(
                "{}: {}\n",
                FLASH_DEVICE_KEY,
                &force_flash.to_string_lossy()
            ));

            if config.debug.skip_flash {
                warn!("setting {} to {}", SKIP_FLASH_KEY, config.debug.skip_flash);
            }

            cfg_str.push_str(&format!(
                "{}: {}\n",
                SKIP_FLASH_KEY, config.debug.skip_flash,
            ));
        } else {
            cfg_str.push_str(&format!(
                "{}: {}\n",
                FLASH_DEVICE_KEY,
                &mig_info.get_flash_device().to_string_lossy()
            ));

            // no skipping when using the real device
            cfg_str.push_str(&format!("{}: false\n", SKIP_FLASH_KEY,));
        }

        cfg_str.push_str(&format!("{}: {}\n", EFI_BOOT_KEY, mig_info.is_efi_boot()));
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
        cfg_str.push_str(&format!("{}:\n", BACKUP_CONFIG_KEY));
        for bckup in &mig_info.boot_cfg_bckup {
            cfg_str.push_str(&format!("  - {}:      '{}'\n", BACKUP_ORIG_KEY, &bckup.0));
            cfg_str.push_str(&format!("    {}:     '{}'\n", BACKUP_BCKUP_KEY, &bckup.1));
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
        let yaml_cfg = YamlLoader::load_from_str(&config_str).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("{}::from_config: failed to parse", MODULE),
        ))?;

        if yaml_cfg.len() != 1 {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::from_config: invalid number of configs in file: {}",
                    MODULE,
                    yaml_cfg.len()
                ),
            ));
        }

        let yaml_cfg = yaml_cfg.get(0).unwrap();

        let mut bckup_cfg: Vec<(String, String)> = Vec::new();

        if let Yaml::Array(ref array) = get_yaml_val(&yaml_cfg, &[BACKUP_CONFIG_KEY])?.unwrap() {
            for value in array {
                if let Yaml::Hash(_v) = value {
                    bckup_cfg.push((
                        String::from(get_yaml_str(value, &[BACKUP_ORIG_KEY])?.unwrap()),
                        String::from(get_yaml_str(value, &[BACKUP_BCKUP_KEY])?.unwrap()),
                    ))
                }
            }
        }

        Ok(Stage2Config {
            efi_boot: get_yaml_bool(&yaml_cfg, &[EFI_BOOT_KEY])?.unwrap(),
            fail_mode: Stage2Config::init_fail_mode(&yaml_cfg).clone(),
            no_flash: get_yaml_bool(&yaml_cfg, &[NO_FLASH_KEY])?.unwrap(),
            skip_flash: get_yaml_bool(&yaml_cfg, &[SKIP_FLASH_KEY])?.unwrap(),
            flash_device: PathBuf::from(get_yaml_str(&yaml_cfg, &[FLASH_DEVICE_KEY])?.unwrap()),
            root_device: PathBuf::from(get_yaml_str(&yaml_cfg, &[ROOT_DEVICE_KEY])?.unwrap()),
            boot_device: PathBuf::from(get_yaml_str(&yaml_cfg, &[BOOT_DEVICE_KEY])?.unwrap()),
            boot_fstype: String::from(get_yaml_str(&yaml_cfg, &[BOOT_FSTYPE_KEY])?.unwrap()),
            efi_device: if let Some(efi_device) = get_yaml_str(&yaml_cfg, &[BOOT_DEVICE_KEY])? {
                Some(PathBuf::from(efi_device))
            } else {
                None
            },
            efi_fstype: if let Some(efi_fstype) = get_yaml_str(&yaml_cfg, &[BOOT_FSTYPE_KEY])? {
                Some(String::from(efi_fstype))
            } else {
                None
            },
            device_slug: String::from(get_yaml_str(&yaml_cfg, &[DEVICE_SLUG_KEY])?.unwrap()),
            balena_image: PathBuf::from(get_yaml_str(&yaml_cfg, &[BALENA_IMAGE_KEY])?.unwrap()),
            balena_config: PathBuf::from(get_yaml_str(&yaml_cfg, &[BALENA_CONFIG_KEY])?.unwrap()),
            work_dir: PathBuf::from(get_yaml_str(&yaml_cfg, &[WORK_DIR_KEY])?.unwrap()),
            bckup_cfg,
        })
    }

    fn init_fail_mode(yaml_cfg: &Yaml) -> &'static FailMode {
        match get_yaml_str(yaml_cfg, &[FAIL_MODE_KEY]) {
            Ok(val) => {
                if let Some(val) = val {
                    match FailMode::from_str(val) {
                        Ok(mode) => mode,
                        Err(_why) => {
                            warn!(
                                "Failed to parse FailMode from {}, defaulting to {:?}. ",
                                val,
                                FailMode::get_default()
                            );
                            FailMode::get_default()
                        }
                    }
                } else {
                    warn!(
                        "FailMode not found in stage2 config, defaulting to {:?}",
                        FailMode::get_default()
                    );
                    FailMode::get_default()
                }
            }
            Err(why) => {
                warn!("Failed to retrieve FailMode from stage2 config, defaulting to {:?}. Error was {:?} ", FailMode::get_default(), why);
                FailMode::get_default()
            }
        }
    }

    pub fn is_no_flash(&self) -> bool {
        self.no_flash
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

    pub fn get_backups(&'a self) -> &'a Vec<(String, String)> {
        &self.bckup_cfg
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
