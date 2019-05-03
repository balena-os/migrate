use yaml_rust::{Yaml, YamlLoader};
use std::fs::read_to_string;
use failure::{ResultExt};
use std::path::{Path, PathBuf};

use crate::common::{
    Stage2Info,
    stage_info::{
        EFI_BOOT_KEY,
        ROOT_DEVICE_KEY,        
        BOOT_DEVICE_KEY,        
        DEVICE_SLUG_KEY,
        BALENA_IMAGE_KEY,        
        BALENA_CONFIG_KEY,
        BACKUP_CONFIG_KEY,
        BACKUP_ORIG_KEY,
        BACKUP_BCKUP_KEY,
        WORK_DIR_KEY,
    },
    MigError,
    MigErrCtx,
    MigErrorKind,
    config_helper::{get_yaml_bool, get_yaml_str, get_yaml_val},
    };

pub(crate) struct Stage2Config {
    efi_boot: bool,
    // drive_device: String,
    boot_device: PathBuf,
    root_device: PathBuf,
    device_slug: String,
    balena_config: PathBuf,
    balena_image: PathBuf,
    work_dir: PathBuf,
    bckup_cfg: Vec<(String,String)>,
}

const MODULE: &str = "stage2::stage2:config";


impl Stage2Config {
    pub fn from_config<P: AsRef<Path>>(path: &P) -> Result<Stage2Config, MigError> {
        // TODO: Dummy, parse from yaml
        let config_str = read_to_string(path).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::from_config: failed to read stage2_config from file: '{}'", MODULE, path.as_ref().display())))?; 
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

        if let Yaml::Array(ref array) = get_yaml_val(&yaml_cfg,&[BACKUP_CONFIG_KEY])?.unwrap() {
            for value in array {
                if let Yaml::Hash(_v) = value {
                    bckup_cfg.push((String::from(get_yaml_str(value,&[BACKUP_ORIG_KEY])?.unwrap()),String::from(get_yaml_str(value,&[BACKUP_BCKUP_KEY])?.unwrap()) ))
                }
            }
        }
        
        Ok(Stage2Config{
            efi_boot: get_yaml_bool(&yaml_cfg, &[EFI_BOOT_KEY])?.unwrap(), 
            root_device: PathBuf::from(get_yaml_str(&yaml_cfg, &[ROOT_DEVICE_KEY])?.unwrap()),
            boot_device: PathBuf::from(get_yaml_str(&yaml_cfg, &[BOOT_DEVICE_KEY])?.unwrap()),
            device_slug: String::from(get_yaml_str(&yaml_cfg, &[DEVICE_SLUG_KEY])?.unwrap()),
            balena_image: PathBuf::from(get_yaml_str(&yaml_cfg, &[BALENA_IMAGE_KEY])?.unwrap()),
            balena_config: PathBuf::from(get_yaml_str(&yaml_cfg, &[BALENA_CONFIG_KEY])?.unwrap()),
            work_dir: PathBuf::from(get_yaml_str(&yaml_cfg, &[WORK_DIR_KEY])?.unwrap()),
            bckup_cfg,
        })
    }
}

impl<'a> Stage2Info<'a> for Stage2Config {
    fn is_efi_boot(&self) -> bool {
        self.efi_boot
    }

    fn get_root_device(&'a self) -> &'a Path {
        self.root_device.as_path()
    }

    fn get_boot_device(&'a self) -> &'a Path {
        self.boot_device.as_path()
    }

    fn get_device_slug(&'a self) -> &'a str {
        &self.device_slug
    }

    fn get_balena_image(&'a self) -> &'a Path {
        self.balena_image.as_path()
    }

    fn get_balena_config(&'a self) -> &'a Path {
        self.balena_config.as_path()
    }

    fn get_backups(&'a self) -> &'a Vec<(String,String)> {
        &self.bckup_cfg
    }
    
    fn get_work_path(&'a self) -> &'a Path {
        &self.work_dir
    }
}