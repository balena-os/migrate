use crate::{
    common::{
        migrate_info::MigrateInfo,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError,
    },
    defs::{BootType, DeviceType},
};

#[cfg(target_os = "linux")]
use crate::linux::stage2::mounts::Mounts;

pub(crate) trait Device {
    fn get_device_slug(&self) -> &'static str;
    fn get_device_type(&self) -> DeviceType;
    fn get_boot_type(&self) -> BootType;
    // TODO: make return reference
    // TODO: return device_info instead of path_info
    fn get_boot_device(&self) -> PathInfo;

    fn setup(
        &self,
        dev_info: &mut MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError>;

    // called in stage2 / linux only

    #[cfg(target_os = "linux")]
    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool;
}
