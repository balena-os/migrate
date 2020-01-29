use log::{debug, error, info};
use regex::Regex;

use crate::{
    common::{
        boot_manager::BootManager,
        device_info::DeviceInfo,
        migrate_info::MigrateInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType, FileType},
    linux::{
        boot_manager_impl::{from_boot_type, RaspiBootManager},
        device_impl::Device,
        linux_common::{expect_type, restore_backups},
        stage2::mounts::Mounts,
    },
};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;
const RPI2_DTB_FILES: &[&str] = &["bcm2709-rpi-2-b.dtb"];
const RPI3_DTB_FILES: &[&str] = &["bcm2710-rpi-3-b.dtb", "bcm2710-rpi-3-b-plus.dtb"];
const RPI4_64_DTB_FILES: &[&str] = &["bcm2711-rpi-4-b.dtb"];

pub(crate) fn is_rpi(
    mig_info: &MigrateInfo,
    config: &Config,
    s2_cfg: &mut Stage2ConfigBuilder,
    model_string: &str,
) -> Result<Option<Box<dyn Device>>, MigError> {
    debug!(
        "raspberrypi::is_rpi: entered with model string: '{}'",
        model_string
    );

    if let Some(captures) = Regex::new(RPI_MODEL_REGEX).unwrap().captures(model_string) {
        let pitype = captures.get(1).unwrap().as_str();
        let model = captures
            .get(2)
            .unwrap()
            .as_str()
            .trim_matches(char::from(0));

        debug!(
            "raspberrypi::is_rpi: selection entered with string: '{}'",
            pitype
        );

        match pitype {
            "2" => {
                info!("Identified RaspberryPi3: model {}", model);
                Ok(Some(Box::new(RaspberryPi2::from_config(
                    mig_info, config, s2_cfg,
                )?)))
            }
            "3" => {
                info!("Identified RaspberryPi3: model {}", model);
                Ok(Some(Box::new(RaspberryPi3::from_config(
                    mig_info, config, s2_cfg,
                )?)))
            }
            "4" => {
                info!("Identified RaspberryPi4: model {}", model);
                Ok(Some(Box::new(RaspberryPi4_64::from_config(
                    mig_info, config, s2_cfg,
                )?)))
            }
            _ => {
                let message = format!("The raspberry pi type reported by your device ('{} {}') is not supported by balena-migrate", pitype, model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        debug!("no match for Raspberry PI on: {}", model_string);
        Ok(None)
    }
}

pub(crate) struct RaspberryPi2 {
    boot_manager: Box<dyn BootManager>,
}

impl RaspberryPi2 {
    pub fn from_config(
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<RaspberryPi2, MigError> {
        const SUPPORTED_OSSES: &[&str] = &["Raspbian GNU/Linux 10 (buster)"];

        let os_name = &mig_info.os_name;

        expect_type(&mig_info.kernel_file.path, &FileType::KernelARMHF)?;

        if let Some(_n) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut dtb_files: Vec<String> = Vec::new();
            let _res = RPI2_DTB_FILES.iter().all(|f| {
                dtb_files.push(String::from(*f));
                true
            });

            let mut boot_manager = RaspiBootManager::new(BootType::Raspi, dtb_files)?;
            if boot_manager.can_migrate(mig_info, config, s2_cfg)? {
                Ok(RaspberryPi2 {
                    boot_manager: Box::new(boot_manager),
                })
            } else {
                Err(MigError::from(MigErrorKind::Displayed))
            }
        } else {
            let message = format!("The OS '{}' is not supported for RaspberryPi2", os_name,);
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }
    }

    pub fn from_boot_type(boot_type: BootType) -> RaspberryPi2 {
        RaspberryPi2 {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl Device for RaspberryPi2 {
    fn get_device_slug(&self) -> &'static str {
        "raspberry-pi2"
    }

    fn get_device_type(&self) -> DeviceType {
        DeviceType::RaspberryPi2
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
        let kernel_opts = if let Some(ref kernel_opts) = config.migrate.get_kernel_opts() {
            kernel_opts.clone()
        } else {
            String::from("")
        };

        self.boot_manager
            .setup(mig_info, config, s2_cfg, &kernel_opts)
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        info!("restoring boot configuration for Raspberry Pi 2");
        restore_backups(mounts.get_boot_mountpoint(), config.get_boot_backups())
    }

    fn get_boot_device(&self) -> DeviceInfo {
        self.boot_manager.get_bootmgr_path().device_info
    }
}

pub(crate) struct RaspberryPi3 {
    boot_manager: Box<dyn BootManager>,
}

impl RaspberryPi3 {
    pub fn from_config(
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<RaspberryPi3, MigError> {
        const SUPPORTED_OSSES: &[&str] = &[
            "Raspbian GNU/Linux 8 (jessie)",
            "Raspbian GNU/Linux 9 (stretch)",
            "Raspbian GNU/Linux 10 (buster)",
        ];

        let os_name = &mig_info.os_name;

        expect_type(&mig_info.kernel_file.path, &FileType::KernelARMHF)?;

        if let Some(_n) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut dtb_files: Vec<String> = Vec::new();
            let _res = RPI3_DTB_FILES.iter().all(|f| {
                dtb_files.push(String::from(*f));
                true
            });

            let mut boot_manager = RaspiBootManager::new(BootType::Raspi, dtb_files)?;
            if boot_manager.can_migrate(mig_info, config, s2_cfg)? {
                Ok(RaspberryPi3 {
                    boot_manager: Box::new(boot_manager),
                })
            } else {
                Err(MigError::from(MigErrorKind::Displayed))
            }
        } else {
            let message = format!("The OS '{}' is not supported for RaspberryPi3", os_name,);
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvParam, &message))
        }
    }

    pub fn from_boot_type(boot_type: BootType) -> RaspberryPi3 {
        RaspberryPi3 {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl Device for RaspberryPi3 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi3"
    }

    fn get_device_type(&self) -> DeviceType {
        DeviceType::RaspberryPi3
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
        let kernel_opts = if let Some(ref kernel_opts) = config.migrate.get_kernel_opts() {
            kernel_opts.clone()
        } else {
            String::from("")
        };

        self.boot_manager
            .setup(mig_info, config, s2_cfg, &kernel_opts)
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        info!("restoring boot configuration for Raspberry Pi 3");
        restore_backups(mounts.get_boot_mountpoint(), config.get_boot_backups())
    }

    fn get_boot_device(&self) -> DeviceInfo {
        self.boot_manager.get_bootmgr_path().device_info
    }
}

pub(crate) struct RaspberryPi4_64 {
    boot_manager: Box<dyn BootManager>,
}

impl RaspberryPi4_64 {
    pub fn from_config(
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<RaspberryPi4_64, MigError> {
        const SUPPORTED_OSSES: &[&str] = &[
            "Raspbian GNU/Linux 8 (jessie)",
            "Raspbian GNU/Linux 9 (stretch)",
            "Raspbian GNU/Linux 10 (buster)",
        ];

        let os_name = &mig_info.os_name;

        expect_type(&mig_info.kernel_file.path, &FileType::KernelAARCH64)?;

        if let Some(_n) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut dtb_files: Vec<String> = Vec::new();
            let _res = RPI4_64_DTB_FILES.iter().all(|f| {
                dtb_files.push(String::from(*f));
                true
            });

            let mut boot_manager = RaspiBootManager::new(BootType::Raspi64, dtb_files)?;
            if boot_manager.can_migrate(mig_info, config, s2_cfg)? {
                Ok(RaspberryPi4_64 {
                    boot_manager: Box::new(boot_manager),
                })
            } else {
                Err(MigError::from(MigErrorKind::Displayed))
            }
        } else {
            let message = format!("The OS '{}' is not supported for RaspberryPi4", os_name,);
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvParam, &message))
        }
    }

    pub fn from_boot_type(boot_type: BootType) -> RaspberryPi4_64 {
        RaspberryPi4_64 {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl Device for RaspberryPi4_64 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi4-64"
    }

    fn get_device_type(&self) -> DeviceType {
        DeviceType::RaspberryPi4_64
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
        let kernel_opts = if let Some(ref kernel_opts) = config.migrate.get_kernel_opts() {
            kernel_opts.clone()
        } else {
            String::from("")
        };

        self.boot_manager
            .setup(mig_info, config, s2_cfg, &kernel_opts)
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        info!("restoring boot configuration for Raspberry Pi 4");
        restore_backups(mounts.get_boot_mountpoint(), config.get_boot_backups())
    }

    fn get_boot_device(&self) -> DeviceInfo {
        self.boot_manager.get_bootmgr_path().device_info
    }
}
