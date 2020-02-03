use log::{error, info, trace};

use crate::common::path_append;
use crate::defs::{MIG_INITRD_NAME, MIG_KERNEL_NAME};
use crate::{
    common::{
        boot_manager::BootManager,
        device::Device,
        device_info::DeviceInfo,
        migrate_info::MigrateInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType, FileType},
    linux::{
        boot_manager_impl::{from_boot_type, GrubBootManager},
        linux_common::{expect_type, is_secure_boot},
        stage2::mounts::Mounts,
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
        const SUPPORTED_OSSES: &[&str] = &[
            "Ubuntu 18.04.3 LTS",
            "Ubuntu 18.04.2 LTS",
            "Ubuntu 16.04.2 LTS",
            "Ubuntu 14.04.2 LTS",
            "Ubuntu 14.04.5 LTS",
            "Ubuntu 14.04.6 LTS",
            "Manjaro Linux",
        ];

        let os_name = &mig_info.os_name;

        expect_type(
            &path_append(&mig_info.work_path.path, MIG_KERNEL_NAME),
            &FileType::KernelAMD64,
        )?;
        expect_type(
            &path_append(&mig_info.work_path.path, MIG_INITRD_NAME),
            &FileType::InitRD,
        )?;

        if SUPPORTED_OSSES.iter().position(|&r| r == os_name).is_none() {
            let message = format!(
                "The OS '{}' is not supported for device type IntelNuc",
                os_name,
            );
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        // **********************************************************************
        // ** AMD64 specific initialisation/checksget_
        // **********************************************************************

        // TODO: determine boot device
        // use config.migrate.flash_device
        // if EFI boot look for EFI partition
        // else look for /boot

        let secure_boot = is_secure_boot()?;
        info!(
            "Secure boot is {}enabled",
            if secure_boot { "" } else { "not " }
        );

        if secure_boot {
            let message =
                "balena-migrate does not currently support systems with secure boot enabled."
                    .to_string();
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        let mut boot_manager = GrubBootManager::new();
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
    }

    pub fn from_boot_type(boot_type: BootType) -> IntelNuc {
        IntelNuc {
            boot_manager: from_boot_type(boot_type),
        }
    }

    /*    fn setup_grub(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
            grub_install(config, mig_info)
        }
    */
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

    fn setup(
        &mut self,
        mig_info: &mut MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        trace!("setup: entered");

        let kernel_opts = if let Some(ref kernel_opts) = config.migrate.get_kernel_opts() {
            kernel_opts.clone()
        } else {
            String::from("")
        };

        self.boot_manager
            .setup(mig_info, config, s2_cfg, &kernel_opts)
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        self.boot_manager.restore(mounts, config)
    }

    fn get_boot_device(&self) -> DeviceInfo {
        self.boot_manager.get_bootmgr_path().device_info
    }
}
