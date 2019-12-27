use crate::{
    common::{
        boot_manager::BootManager,
        device_info::DeviceInfo,
        migrate_info::MigrateInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::BootType,
    linux::{linux_common::restore_backups, stage2::mounts::Mounts},
};

pub(crate) struct EfiBootManager {
    #[allow(dead_code)]
    msw_device: bool,
}

impl EfiBootManager {
    pub fn new(msw_device: bool) -> EfiBootManager {
        EfiBootManager { msw_device }
    }
}

impl BootManager for EfiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Efi
    }

    fn get_bootmgr_path(&self) -> DeviceInfo {
        unimplemented!()
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
        _kernel_opts: &str,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn restore(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        if self.msw_device {
            // TODO: restore boot to windows
            // a) convince syslinux to boot windows
            // b) enforce default boot to windows
        }

        restore_backups(mounts.get_boot_mountpoint(), config.get_boot_backups())
    }
}
