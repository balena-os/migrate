use log::{error, info, trace};
use std::path::Path;

use crate::{
    common::{Config, MigError, MigErrorKind},
    device::{grub_install, Device, DeviceType},
    linux_common::disk_info::DiskInfo,
    linux_common::{is_secure_boot, migrate_info::MigrateInfo, restore_backups},
    stage2::Stage2Config,
    boot_manager::{BootType, BootManager, GrubBootManager, from_boot_type}
};

pub(crate) struct IntelNuc {
    boot_manager: Box<BootManager>,
}

impl IntelNuc {
    pub fn from_config(config: &Config, mig_info: &mut MigrateInfo) -> Result<IntelNuc,MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &[
            "Ubuntu 18.04.2 LTS",
            "Ubuntu 16.04.2 LTS",
            "Ubuntu 14.04.2 LTS",
            "Ubuntu 14.04.5 LTS",
        ];

        let os_name = mig_info.get_os_name();
        if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let message = format!("The OS '{}' is not supported for device type IntelNuc",os_name,);
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        // **********************************************************************
        // ** AMD64 specific initialisation/checks
        // **********************************************************************

        mig_info.secure_boot = Some(is_secure_boot()?);
        if let Some(secure_boot) = mig_info.secure_boot {
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
                return Ok(false);
            }
        }

        Ok(IntelNuc{ boot_manager: Box::new(GrubBootManager{})})
    }

    pub fn from_boot_type(boot_type: &BootType) -> IntelNuc {
        IntelNuc {
            boot_manager: from_boot_type(boot_type),
        }
    }

    fn setup_grub(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        grub_install(config, mig_info)
    }
}

impl<'a> Device for IntelNuc {
    fn get_device_slug(&self) -> &'static str {
        "intel-nuc"
    }

    fn get_device_type(&self) -> DeviceType {
        DeviceType::IntelNuc
    }

    fn get_boot_type(&self) -> BootType {
        self.boot_manager.get_boot_type()
    }

    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "IntelNuc::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        mig_info.get_boot_manager().setup(mig_info)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration for IntelNuc");
        restore_backups(root_path, config.get_boot_backups())?;
        info!("The original boot configuration was restored");
        Ok(())
    }
}
