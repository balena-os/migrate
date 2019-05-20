use log::{error, info, trace};
use std::path::Path;

use crate::{
    common::{BootType, Config, MigError, MigErrorKind},
    defs::GRUB_MIN_VERSION,
    device::{grub_install, Device},
    linux_common::disk_info::DiskInfo,
    linux_common::{get_grub_version, is_secure_boot, migrate_info::MigrateInfo, restore_backups},
    stage2::Stage2Config,
};

pub(crate) struct IntelNuc {}

impl IntelNuc {
    pub fn new() -> IntelNuc {
        IntelNuc {}
    }

    fn setup_grub(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        grub_install(config, mig_info)
    }
}

impl<'a> Device for IntelNuc {
    fn get_device_slug(&self) -> &'static str {
        "intel-nuc"
    }

    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "IntelNuc::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );
        if let Some(ref boot_type) = mig_info.boot_type {
            match boot_type {
                BootType::GRUB => self.setup_grub(config, mig_info),
                _ => Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Invalid boot type for '{}' : {:?}'",
                        self.get_device_slug(),
                        mig_info.boot_type
                    ),
                )),
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("No boot type specified for '{}'", self.get_device_slug()),
            ))
        }
    }

    fn can_migrate(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &[
            "Ubuntu 18.04.2 LTS",
            "Ubuntu 16.04.2 LTS",
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
            mig_info.disk_info = Some(DiskInfo::new(
                false,
                &config.migrate.get_work_dir(),
                config.migrate.get_log_device(),
            )?);
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

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration for IntelNuc");
        restore_backups(root_path, config.get_boot_backups())?;
        info!("The original boot configuration was restored");
        Ok(())
    }
}
