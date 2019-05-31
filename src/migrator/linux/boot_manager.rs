use std::path::Path;

use crate::{
    common::{
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::BootType,
    linux::{EnsuredCmds, MigrateInfo},
};

pub(crate) mod u_boot_manager;
pub(crate) use u_boot_manager::UBootManager;
pub(crate) mod grub_boot_manager;
pub(crate) use grub_boot_manager::GrubBootManager;
pub(crate) mod raspi_boot_manager;
pub(crate) use raspi_boot_manager::RaspiBootManager;

pub(crate) fn from_boot_type(boot_type: &BootType) -> Box<BootManager> {
    match boot_type {
        BootType::UBoot => Box::new(UBootManager::new()),
        BootType::Grub => Box::new(GrubBootManager::new()),
        BootType::Efi => Box::new(EfiBootManager::new()),
        BootType::Raspi => Box::new(RaspiBootManager::new()),
    }
}

pub(crate) trait BootManager {
    fn get_boot_type(&self) -> BootType;
    fn can_migrate(
        &mut self,
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError>;
    fn setup(
        &self,
        cmds: &EnsuredCmds,
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
        &mut self,
        _cmds: &mut EnsuredCmds,
        _dev_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(
        &self,
        _cmds: &EnsuredCmds,
        _dev_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(
        &self,
        _slug: &str,
        _root_path: &Path,
        _config: &Stage2Config,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
