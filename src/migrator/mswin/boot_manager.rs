use crate::{
    common::{Config, stage2_config::Stage2ConfigBuilder, MigError, MigErrorKind},
    mswin::{
        migrate_info::{MigrateInfo},
    },
    defs::{BootType}
};

pub(crate) trait BootManager {
    fn get_boot_type(&self) -> BootType;
    fn can_migrate(
        &mut self,
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
}

pub(crate) struct EfiBootManager{
    msw_device: bool,
}

impl EfiBootManager {
    pub fn new(msw_device: bool) -> EfiBootManager {
        EfiBootManager { msw_device }
    }
}

impl BootManager for EfiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::MswEfi
    }

    fn can_migrate(
        &mut self,
        _dev_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(
        &self,
        _dev_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
