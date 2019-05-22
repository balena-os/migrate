use crate::{
    common::{Config, MigError, MigErrCtx, MigErrorKind},
    linux_common::{MigrateInfo},
    stage2::stage2_config::{Stage2Config},
    boot_manager::{BootManager},
};


pub(crate) struct UBootManager;

impl BootManager for UBootManager {
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
