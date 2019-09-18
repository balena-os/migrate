use log::{debug, error, trace};
use regex::Regex;

use crate::{
    common::{
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigError, MigErrorKind,
    },
    defs::{BootType, DeviceType},
    linux::{
        boot_manager::{from_boot_type, BootManager, UBootManager},
        device::Device,
        migrate_info::PathInfo,
        stage2::mounts::Mounts,
        EnsuredCmds, MigrateInfo,
    },
};

const SUPPORTED_OSSES: [&str; 3] = [
    "Ubuntu 18.04.2 LTS",
    "Ubuntu 14.04.1 LTS",
    "Debian GNU/Linux 9 (stretch)",
];

// add some of this to balena bb XM command line:
// mtdparts=omap2-nand.0:512k(spl),1920k(u-boot),128k(u-boot-env),128k(dtb),6m(kernel),-(rootfs)
// mpurate=auto
// buddy=none
// camera=none
// vram=12M
// omapfb.mode=dvi:640x480MR-16@60 omapdss.def_disp=dvi
// rootwait

// const BBXM_KOPTS: &str ="mtdparts=omap2-nand.0:512k(spl),1920k(u-boot),128k(u-boot-env),128k(dtb),6m(kernel),-(rootfs) mpurate=auto buddy=none camera=none vram=12M omapfb.mode=dvi:640x480MR-16@60 omapdss.def_disp=dvi";
const BBXM_KOPTS: &str = "";

const BBG_KOPTS: &str = "";

const BBB_KOPTS: &str = "";

// Supported models
// TI OMAP3 BeagleBoard xM
const BB_MODEL_REGEX: &str = r#"^((\S+\s+)*\S+)\s+Beagle(Bone|Board)\s+(\S+)$"#;

// TODO: check location of uEnv.txt or other files files to improve reliability

pub(crate) fn is_bb(
    cmds: &mut EnsuredCmds,
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
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<BeagleboneGreen, MigError> {
        let os_name = &mig_info.os_name;

        if let Some(_idx) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut boot_manager = UBootManager::new();

            // TODO: determine boot device
            // use config.migrate.flash_device
            //

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

impl Device for BeagleboneGreen {
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
        cmds: &EnsuredCmds,
        dev_info: &mut MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        let kernel_opts = if let Some(ref kopts) = config.migrate.get_kernel_opts() {
            let mut new_opts: String = kopts.clone();
            new_opts.push(' ');
            new_opts.push_str(BBG_KOPTS);
            new_opts
        } else {
            String::from(BBG_KOPTS)
        };

        self.boot_manager
            .setup(cmds, dev_info, s2_cfg, &kernel_opts)
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        self.boot_manager.restore(mounts, config)
    }

    fn get_boot_device(&self) -> PathInfo {
        self.boot_manager.get_bootmgr_path()
    }
}

pub(crate) struct BeagleboneGreen {
    boot_manager: Box<BootManager>,
}

impl BeagleboneBlack {
    // this is used in stage1
    fn from_config(
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<BeagleboneBlack, MigError> {
        let os_name = &mig_info.os_name;

        if let Some(_idx) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut boot_manager = UBootManager::new();

            if boot_manager.can_migrate(cmds, mig_info, config, s2_cfg)? {
                Ok(BeagleboneBlack {
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

impl Device for BeagleboneBlack {
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
        cmds: &EnsuredCmds,
        dev_info: &mut MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        let kernel_opts = if let Some(ref kopts) = config.migrate.get_kernel_opts() {
            let mut new_opts: String = kopts.clone();
            new_opts.push(' ');
            new_opts.push_str(BBB_KOPTS);
            new_opts
        } else {
            String::from(BBB_KOPTS)
        };

        self.boot_manager
            .setup(cmds, dev_info, s2_cfg, &kernel_opts)
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        self.boot_manager.restore(mounts, config)
    }

    fn get_boot_device(&self) -> PathInfo {
        self.boot_manager.get_bootmgr_path()
    }
}

pub(crate) struct BeagleboardXM {
    boot_manager: Box<BootManager>,
}

impl BeagleboardXM {
    // this is used in stage1

    fn from_config(
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<BeagleboardXM, MigError> {
        let os_name = &mig_info.os_name;

        if let Some(_idx) = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let mut boot_manager = UBootManager::new();

            /*
                        if let None = config.balena.get_uboot_env() {
                            let msg = String::from("Device type beagleboard xM requires the u-boot env to be set up to migrate successfully");
                            error!("{}", &msg);
                            return Err(MigError::from_remark(MigErrorKind::InvState, &msg));
                        }
            */
            if boot_manager.can_migrate(cmds, mig_info, config, s2_cfg)? {
                Ok(BeagleboardXM {
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
        "beagleboard-xm"
    }

    fn get_boot_type(&self) -> BootType {
        self.boot_manager.get_boot_type()
    }

    fn restore_boot(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        self.boot_manager.restore(mounts, config)
    }

    fn setup(
        &self,
        cmds: &EnsuredCmds,
        mig_info: &mut MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        let kernel_opts = if let Some(ref kopts) = config.migrate.get_kernel_opts() {
            let mut new_opts: String = kopts.clone();
            new_opts.push(' ');
            new_opts.push_str(BBXM_KOPTS);
            new_opts
        } else {
            String::from(BBXM_KOPTS)
        };

        self.boot_manager
            .setup(cmds, mig_info, s2_cfg, &kernel_opts)
    }

    fn get_boot_device(&self) -> PathInfo {
        self.boot_manager.get_bootmgr_path()
    }
}
