use log::{error, info, trace};
use std::path::Path;

use crate::linux_common::disk_info::DiskInfo;
use crate::{
    common::{BootType, Config, MigError, MigErrorKind},
    device::Device,
    linux_common::{get_grub_version, is_secure_boot, migrate_info::MigrateInfo},
    stage2::Stage2Config,
};

const MODULE: &str = "intel_nuc";

const GRUB_MIN_VERSION: &str = "2";

pub(crate) struct IntelNuc {}

impl IntelNuc {
    pub fn new() -> IntelNuc {
        IntelNuc {}
    }
}

impl<'a> Device for IntelNuc {
    fn get_device_slug(&self) -> &'static str {
        "intel-nuc"
    }

    fn setup(&self, _config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "BeagleboneGreen::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn can_migrate(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &[
            "Ubuntu 18.04.2 LTS",
            //    "Ubuntu 16.04.2 LTS",
            "Ubuntu 14.04.2 LTS",
            "Ubuntu 14.04.5 LTS",
        ];

        let os_name = mig_info.get_os_name();
        if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            error!(
                "The OS '{}' is not supported for '{}'",
                os_name,
                self.get_device_slug()
            );
            return Ok(false);
        }

        // **********************************************************************
        // ** AMD64 specific initialisation/checks
        // **********************************************************************

        if mig_info.get_os_name().to_lowercase().starts_with("ubuntu") {
            mig_info.boot_type = Some(BootType::GRUB);
            mig_info.disk_info = Some(DiskInfo::new(false, &config.migrate.get_work_dir())?);
            mig_info.install_path = Some(mig_info.disk_info.as_ref().unwrap().root_path.clone());
        }

        info!(
            "System is booted in {:?} mode",
            mig_info.boot_type.as_ref().unwrap()
        );

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

        let grub_version = get_grub_version()?;
        info!(
            "grub-install version is {}.{}",
            grub_version.0, grub_version.1
        );

        if grub_version.0 < String::from(GRUB_MIN_VERSION) {
            error!("Your version of grub-install ({}.{}) is not supported. balena-migrate requires grub version 2 or higher.", grub_version.0, grub_version.1);
            return Ok(false);
        }

        Ok(true)
    }

    fn restore_boot(&self, _root_path: &Path, _config: &Stage2Config) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
