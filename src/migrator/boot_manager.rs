use std::path::{Path};
use serde::{Deserialize, Serialize};

use crate::{
    common::{Config, MigError, MigErrorKind},
    linux_common::{MigrateInfo},
    stage2::stage2_config::{Stage2Config},
};

pub(crate) mod u_boot_manager;
pub(crate) use u_boot_manager::{UBootManager};
pub(crate) mod grub_boot_manager;
pub(crate) use grub_boot_manager::{GrubBootManager};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum BootType {
    UBoot,
    Raspi,
    Efi,
    Grub,
}

pub(crate) fn from_boot_type(boot_type: &BootType) -> Box<BootManager> {
    match boot_type {
        BootType::UBoot => Box::new(UBootManager),
        BootType::Grub => Box::new(GrubBootManager),
        BootType::Efi => Box::new(EfiBootManager),
        BootType::Raspi => Box::new(RaspiBootManager),
    }
}

pub(crate) trait BootManager {
    fn get_boot_type(&self) -> BootType;
    fn can_migrate(&self, config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError>;
    fn setup(&self, mig_info: &mut MigrateInfo) -> Result<(), MigError>;
    fn restore(&self, slug: &str, root_path: &Path, config: &Stage2Config) -> Result<(), MigError>;
    fn set_bootmgr_path(&self,config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError>;
}


pub(crate) struct EfiBootManager;

impl EfiBootManager {
    pub fn new() -> EfiBootManager { EfiBootManager{} }
}

impl BootManager for EfiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Efi
    }

    fn can_migrate(&self,config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(&self, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, slug: &str, root_path: &Path,  config: &Stage2Config) -> Result<(), MigError>  {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn set_bootmgr_path(&self,config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

}

pub(crate) struct RaspiBootManager;

impl RaspiBootManager {
    pub fn new() -> RaspiBootManager { RaspiBootManager{} }
}

impl BootManager for RaspiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Raspi
    }

    fn can_migrate(&self, config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(&self, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, slug: &str, root_path: &Path,  config: &Stage2Config) -> Result<(), MigError>  {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn set_bootmgr_path(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
