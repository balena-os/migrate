use crate::{
    common::{
        migrate_info::MigrateInfo,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError,
    },
    defs::BootType,
};

#[cfg(target_os = "linux")]
use crate::linux::stage2::mounts::Mounts;

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
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError>;

    #[cfg(target_os = "linux")]
    fn restore(&self, mounts: &Mounts, config: &Stage2Config) -> bool;
    // TODO: make return reference
    fn get_bootmgr_path(&self) -> PathInfo;
}
