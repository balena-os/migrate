use log::{debug, error, trace};
use regex::Regex;
use std::path::Path;

use crate::{
    common::{
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType},
    linux_migrator::{
        boot_manager::{from_boot_type, BootManager, UBootManager},
        device::Device,
        linux_common::{migrate_info::MigrateInfo, EnsuredCommands},
    },
};

const SUPPORTED_OSSES: [&str; 2] = ["Ubuntu 18.04.2 LTS", "Ubuntu 14.04.1 LTS"];

// Supported models
// TI OMAP3 BeagleBoard xM
const BB_MODEL_REGEX: &str = r#"^((\S+\s+)*\S+)\s+Beagle(Bone|Board)\s+(\S+)$"#;

// TODO: check location of uEnv.txt or other files files to improve reliability

pub(crate) fn is_bb(
    cmds: &mut EnsuredCommands,
    dev_info: &MigrateInfo,
    config: &Config,
    s2_cfg: &mut Stage2ConfigBuilder,
    model_string: &str,
) -> Result<Option<Box<Device>>, MigError> {
    trace!(
        "Beaglebone::is_bb: entered with model string: '{}'",
        model_string
    );

    if let Some(captures) = Regex::new(BB_MODEL_REGEX).unwrap().captures(model_string) {
        let model = captures
            .get(4)
            .unwrap()
            .as_str()
            .trim_matches(char::from(0));

        match model {
            "xM" => {
                debug!("match found for BeagleboardXM");
                Ok(Some(Box::new(BeagleboardXM::from_config(
                    cmds, dev_info, config, s2_cfg,
                )?)))
            }
            "Green" => {
                debug!("match found for BeagleboneGreen");
                Ok(Some(Box::new(BeagleboneGreen::from_config(
                    cmds, dev_info, config, s2_cfg,
                )?)))
            }
            "Black" => {
                debug!("match found for BeagleboneBlack");
                Ok(Some(Box::new(BeagleboneBlack::from_config(
                    cmds, dev_info, config, s2_cfg,
                )?)))
            }
            _ => {
                let message = format!("The beaglebone model reported by your device ('{}') is not supported by balena-migrate", model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        debug!("no match for beaglebone on: {}", model_string);
        Ok(None)
    }
}

pub(crate) struct BeagleboneBlack {
    boot_manager: Box<BootManager>,
}

impl BeagleboneGreen {
    // this is used in stage1
    fn from_config(
        cmds: &mut EnsuredCommands,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<BeagleboneGreen, MigError> {
        let os_name = &mig_info.os_name;

        if let Some(_idx) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let boot_manager = UBootManager {};

            if boot_manager.can_migrate(cmds, mig_info, config, s2_cfg)? {
                Ok(BeagleboneGreen {
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
            let message = format!(
                "The OS '{}' is not supported for the device type BeagleboneGreen",
                os_name
            );
            error!("{}", &message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
    }

    // this is used in stage2
    pub fn from_boot_type(boot_type: &BootType) -> BeagleboneGreen {
        BeagleboneGreen {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl<'a> Device for BeagleboneGreen {
    fn get_device_type(&self) -> DeviceType {
        DeviceType::BeagleboneGreen
    }

    fn get_device_slug(&self) -> &'static str {
        "beaglebone-green"
    }

    fn get_boot_type(&self) -> BootType {
        self.boot_manager.get_boot_type()
    }

    fn setup(
        &self,
        cmds: &EnsuredCommands,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        self.boot_manager.setup(cmds, dev_info, config, s2_cfg)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        self.boot_manager
            .restore(self.get_device_slug(), root_path, config)
    }
}

pub(crate) struct BeagleboneGreen {
    boot_manager: Box<BootManager>,
}

impl BeagleboneBlack {
    // this is used in stage1
    fn from_config(
        cmds: &mut EnsuredCommands,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<BeagleboneBlack, MigError> {
        let os_name = &dev_info.os_name;

        if let Some(_idx) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            Ok(BeagleboneBlack {
                boot_manager: Box::new(UBootManager {}),
            })
        } else {
            let message = format!(
                "The OS '{}' is not supported for the device type BeagleboneBlack",
                os_name
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
    }

    // this is used in stage2
    pub fn from_boot_type(boot_type: &BootType) -> BeagleboneBlack {
        BeagleboneBlack {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl<'a> Device for BeagleboneBlack {
    fn get_device_type(&self) -> DeviceType {
        DeviceType::BeagleboneBlack
    }

    fn get_device_slug(&self) -> &'static str {
        "beaglebone-black"
    }

    fn get_boot_type(&self) -> BootType {
        self.boot_manager.get_boot_type()
    }

    fn setup(
        &self,
        cmds: &EnsuredCommands,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        self.boot_manager.setup(cmds, dev_info, config, s2_cfg)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        self.boot_manager
            .restore(self.get_device_slug(), root_path, config)
    }
}

pub(crate) struct BeagleboardXM {
    boot_manager: Box<BootManager>,
}

impl BeagleboardXM {
    // this is used in stage1
    fn from_config(
        cmds: &mut EnsuredCommands,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<BeagleboardXM, MigError> {
        let os_name = &dev_info.os_name;

        if let Some(_idx) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            Ok(BeagleboardXM {
                boot_manager: Box::new(UBootManager {}),
            })
        } else {
            let message = format!(
                "The OS '{}' is not supported for the device type BeagleboardXM",
                os_name
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
    }

    // this is used in stage2
    pub fn from_boot_type(boot_type: &BootType) -> BeagleboardXM {
        BeagleboardXM {
            boot_manager: from_boot_type(boot_type),
        }
    }
}

impl<'a> Device for BeagleboardXM {
    fn get_device_type(&self) -> DeviceType {
        DeviceType::BeagleboardXM
    }

    fn get_device_slug(&self) -> &'static str {
        // beagleboard xM masquerades as beaglebone-black
        "beaglebone-black"
    }

    fn get_boot_type(&self) -> BootType {
        self.boot_manager.get_boot_type()
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        self.boot_manager
            .restore(self.get_device_slug(), root_path, config)
    }

    fn setup(
        &self,
        cmds: &EnsuredCommands,
        dev_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        self.boot_manager.setup(cmds, dev_info, config, s2_cfg)
    }
}
