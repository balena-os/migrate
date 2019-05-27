use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{
    common::{Config, MigError, MigErrorKind},
    linux_common::migrate_info::MigrateInfo,
    stage2::stage2_config::{Stage2Config, Stage2ConfigBuilder},
};

pub(crate) mod u_boot_manager;
pub(crate) use u_boot_manager::UBootManager;
pub(crate) mod grub_boot_manager;
pub(crate) use grub_boot_manager::GrubBootManager;
pub(crate) mod raspi_boot_manager;
pub(crate) use raspi_boot_manager::RaspiBootManager;

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
    fn can_migrate(
        &self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError>;
    fn setup(
        &self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError>;
    fn restore(&self, slug: &str, root_path: &Path, config: &Stage2Config) -> Result<(), MigError>;
}

pub(crate) struct EfiBootManager;

impl EfiBootManager {
    pub fn new() -> EfiBootManager {
        EfiBootManager {}
    }
}

impl BootManager for EfiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Efi
    }

    fn can_migrate(
        &self,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(
        &self,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, slug: &str, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
