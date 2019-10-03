use crate::{
    common::{
        device_info::DeviceInfo, migrate_info::MigrateInfo, stage2_config::Stage2ConfigBuilder,
        Config, MigError,
    },
    defs::BootType,
};

#[cfg(target_os = "linux")]
use crate::{common::stage2_config::Stage2Config, linux::stage2::mounts::Mounts};

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
    fn get_bootmgr_path(&self) -> DeviceInfo;
}
