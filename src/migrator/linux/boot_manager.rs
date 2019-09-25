use crate::{
    common::{
        migrate_info::MigrateInfo,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::BootType,
    linux::stage2::mounts::Mounts,
};

pub(crate) mod u_boot_manager;
pub(crate) use u_boot_manager::UBootManager;
pub(crate) mod grub_boot_manager;
pub(crate) use grub_boot_manager::GrubBootManager;
pub(crate) mod raspi_boot_manager;
pub(crate) use raspi_boot_manager::RaspiBootManager;

pub(crate) fn from_boot_type(boot_type: &BootType) -> Box<dyn BootManager> {
    match boot_type {
        BootType::UBoot => Box::new(UBootManager::new()),
        BootType::Grub => Box::new(GrubBootManager::new()),
        BootType::Efi => Box::new(EfiBootManager::new(false)),
        BootType::MSWEfi => Box::new(EfiBootManager::new(true)),
        BootType::Raspi => Box::new(RaspiBootManager::new(boot_type.clone()).unwrap()),
        BootType::Raspi64 => Box::new(RaspiBootManager::new(boot_type.clone()).unwrap()),
        BootType::MSWBootMgr => panic!("BootType::MSWBootMgr is not implemented"),
    }
}

// TODO: support configured / device specific command line options

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

    fn restore(&self, mounts: &Mounts, config: &Stage2Config) -> bool;
    // TODO: make return reference
    fn get_bootmgr_path(&self) -> PathInfo;
    fn get_boot_path(&self) -> PathInfo;
}

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
    fn get_bootmgr_path(&self) -> PathInfo {
        unimplemented!()
    }
    fn get_boot_path(&self) -> PathInfo {
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
        _s2_cfg: &mut Stage2ConfigBuilder,
        _kernel_opts: &str,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, _mounts: &Mounts, _config: &Stage2Config) -> bool {
        unimplemented!()
    }
}
