use crate::{
    common::{Config, MigError, MigErrCtx, MigErrorKind},
    linux_common::{MigrateInfo},
    stage2::stage2_config::{Stage2Config},
};

pub(crate) mod uboot_manager;


pub(crate) trait BootManager {
    fn can_migrate(config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError>;
    fn setup(&self, config: &Coonfig, mig__info: MigrateInfo) -> Result<(), MigError>;
    fn restore(&self, config: Stage2Config) -> Result<(), MigError>;
}

pub(crate) struct GrubBootManager;

impl BootManager for GrubBootManager {
    fn can_migrate(config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(&self, config: &Coonfig, mig__info: MigrateInfo) -> Result<(), MigError>  -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, config: Stage2Config) -> Result<(), MigError>  -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
    }
}

pub(crate) struct EfiBootManager;

impl BootManager for EfiBootManager {
    fn can_migrate(config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(&self, config: &Coonfig, mig__info: MigrateInfo) -> Result<(), MigError>  -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, config: Stage2Config) -> Result<(), MigError>  -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
    }
}

pub(crate) struct RpiBootManager;

impl BootManager for EfiBootManager {
    fn can_migrate(config: &Config, mig_info: &MigrateInfo) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn setup(&self, config: &Coonfig, mig__info: MigrateInfo) -> Result<(), MigError>  -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, config: Stage2Config) -> Result<(), MigError>  -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
    }
}
