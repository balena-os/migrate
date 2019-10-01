use log::{error, info};

use crate::{
    common::{
        boot_manager::BootManager, config::Config, device::Device, migrate_info::MigrateInfo,
        path_info::PathInfo, stage2_config::Stage2ConfigBuilder, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType},
    mswin::{
        boot_manager_impl::efi_boot_manager::EfiBootManager, powershell::is_secure_boot,
        win_api::is_efi_boot,
    },
};

pub(crate) struct IntelNuc {
    boot_manager: Box<dyn BootManager>,
}

impl IntelNuc {
    pub fn from_config(
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<IntelNuc, MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &[
            "Ubuntu 18.04.3 LTS",
            "Ubuntu 18.04.2 LTS",
            "Ubuntu 16.04.2 LTS",
            "Ubuntu 14.04.2 LTS",
            "Ubuntu 14.04.5 LTS",
            "Ubuntu 14.04.6 LTS",
        ];

        let os_name = &mig_info.os_name;
        // TODO: find replacement for file command in windows
        //expect_type(&mig_info.kernel_file.path, &FileType::KernelAMD64)?;

        if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let message = format!(
                "The OS '{}' is not supported for device type IntelNuc",
                os_name,
            );
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        // **********************************************************************
        // ** AMD64 specific initialisation/checks
        // **********************************************************************

        // TODO: determine boot device
        // use config.migrate.flash_device
        // if EFI boot look for EFI partition
        // else look for /boot

        let secure_boot = is_secure_boot()?;
        info!(
            "Secure boot is {}enabled",
            match secure_boot {
                true => "",
                false => "not ",
            }
        );

        if secure_boot == true {
            let message = format!(
                "balena-migrate does not currently support systems with secure boot enabled."
            );
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        if is_efi_boot()? {
            let mut boot_manager = EfiBootManager::new();
            if boot_manager.can_migrate(mig_info, config, s2_cfg)? {
                Ok(IntelNuc {
                    boot_manager: Box::new(boot_manager),
                })
            } else {
                let message = format!(
                    "The boot manager '{:?}' is not able to set up your device",
                    boot_manager.get_boot_type()
                );
                error!("{}", &message);
                Err(MigError::from_remark(MigErrorKind::InvState, &message))
            }
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Only EFI booted intel systems are currently supported on '{}'",
                    mig_info.os_name
                ),
            ));
        }
    }
}

impl Device for IntelNuc {
    fn get_device_slug(&self) -> &'static str {
        unimplemented!()
    }
    fn get_device_type(&self) -> DeviceType {
        unimplemented!()
    }
    fn get_boot_type(&self) -> BootType {
        unimplemented!()
    }
    // TODO: make return reference
    // TODO: return device_info instead of path_info
    fn get_boot_device(&self) -> PathInfo {
        unimplemented!()
    }

    fn setup(
        &self,
        dev_info: &mut MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        unimplemented!()
    }
}
