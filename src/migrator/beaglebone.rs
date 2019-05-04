use chrono::Local;
use failure::ResultExt;
use log::{error, info, trace, warn};
use regex::Regex;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::Path;

use crate::common::{file_exists, is_balena_file, Config, MigErrCtx, MigError, MigErrorKind};

use crate::linux_common::{call_cmd, path_append, restore_backups, Device, MigrateInfo, CHMOD_CMD};

use crate::stage2::Stage2Config;

const BB_DRIVE_REGEX: &str = r#"^/dev/mmcblk(\d+)p(\d+)$"#;
const BB_MODEL_REGEX: &str = r#"^((\S+\s+)*\S+)\s+BeagleBone\s+(\S+)$"#;

const BBG_MIG_KERNEL_PATH: &str = "/boot/balena-migrate.zImage";
const BBG_MIG_INITRD_PATH: &str = "/boot/balena-migrate.initrd";
const BBG_UENV_PATH: &str = "/uEnv.txt";

const UENV_TXT: &str = r###"## created by balena-migrate

loadaddr=0x82000000
fdtaddr=0x88000000
rdaddr=0x88080000

initrd_high=0xffffffff
fdt_high=0xffffffff

##These are needed to be compliant with Debian 2014-05-14 u-boot.

loadximage=echo debug: [__KERNEL_PATH__] ... ; load mmc __DRIVE__:__PARTITION__ ${loadaddr} __KERNEL_PATH__
loadxfdt=echo debug: [/boot/dtbs/${uname_r}/${fdtfile}] ... ;load mmc __DRIVE__:__PARTITION__ ${fdtaddr} /boot/dtbs/${uname_r}/${fdtfile}
loadxrd=echo debug: [__INITRD_PATH__] ... ; load mmc __DRIVE__:__PARTITION__ ${rdaddr} __INITRD_PATH__; setenv rdsize ${filesize}
loaduEnvtxt=load mmc __DRIVE__:__PARTITION__ ${loadaddr} /boot/uEnv.txt ; env import -t ${loadaddr} ${filesize};
check_dtb=if test -n ${dtb}; then setenv fdtfile ${dtb};fi;
check_uboot_overlays=if test -n ${enable_uboot_overlays}; then setenv enable_uboot_overlays ;fi;
loadall=run loaduEnvtxt; run check_dtb; run check_uboot_overlays; run loadximage; run loadxrd; run loadxfdt;

mmcargs=setenv bootargs console=tty0 console=${console} ${optargs} ${cape_disable} ${cape_enable} root=__ROOT_DEV__ rootfstype=${mmcrootfstype} ${cmdline}

uenvcmd=run loadall; run mmcargs; echo debug: [${bootargs}] ... ; echo debug: [bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr}] ... ; bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr};
"###;

// TODO: create/return trait for device processing

pub(crate) fn is_bb(model_string: &str) -> Result<Box<Device>, MigError> {
    trace!(
        "Beaglebone::is_bb: entered with model string: '{}'",
        model_string
    );

    if let Some(captures) = Regex::new(BB_MODEL_REGEX).unwrap().captures(model_string) {
        let model = captures
            .get(3)
            .unwrap()
            .as_str()
            .trim_matches(char::from(0));

        match model {
            "Green" => Ok(Box::new(BeagleboneGreen {})),
            _ => {
                let message = format!("The beaglebone model reported by your device ('{}') is not supported by balena-migrate", model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        warn!("no match for beaglebone on: {}", model_string);
        Err(MigError::from(MigErrorKind::NoMatch))
    }
}

pub(crate) struct BeagleboneGreen {}

impl BeagleboneGreen {
    pub(crate) fn new() -> BeagleboneGreen {
        BeagleboneGreen {}
    }
}

impl<'a> Device for BeagleboneGreen {
    fn get_device_slug(&self) -> &'static str {
        "beaglebone-green"
    }

    fn setup(&self, _config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "BeagleboneGreen::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        // **********************************************************************
        // ** read drive number & partition number from boot device
        let drive_num = {
            let dev_name = mig_info.get_boot_device();

            if let Some(captures) = Regex::new(BB_DRIVE_REGEX)
                .unwrap()
                .captures(&dev_name.to_string_lossy())
            {
                (
                    String::from(captures.get(1).unwrap().as_str()),
                    String::from(captures.get(2).unwrap().as_str()),
                )
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "failed to parse drive & partition numbers from boot device name '{}'",
                        dev_name.display()
                    ),
                ));
            }
        };

        // **********************************************************************
        // ** copy new kernel & iniramfs
        if let Some(ref file_info) = mig_info.kernel_info {
            std::fs::copy(&file_info.path, BBG_MIG_KERNEL_PATH).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to copy kernel file '{}' to '{}'",
                    file_info.path.display(),
                    BBG_MIG_KERNEL_PATH
                ),
            ))?;
            info!(
                "copied kernel: '{}' -> '{}'",
                file_info.path.display(),
                BBG_MIG_KERNEL_PATH
            );
            call_cmd(CHMOD_CMD, &["+x", BBG_MIG_KERNEL_PATH], false)?;
        } else {
            panic!("no kernel file info found");
        }

        if let Some(ref file_info) = mig_info.initrd_info {
            std::fs::copy(&file_info.path, BBG_MIG_INITRD_PATH).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to copy initrd file '{}' to '{}'",
                    file_info.path.display(),
                    BBG_MIG_KERNEL_PATH
                ),
            ))?;
            info!(
                "copied initrd: '{}' -> '{}'",
                file_info.path.display(),
                BBG_MIG_INITRD_PATH
            );
        } else {
            panic!("no initrd file info found");
        }

        // **********************************************************************
        // ** backup /uEnv.txt if exists

        if file_exists(BBG_UENV_PATH) {
            // TODO: make sure we do not backup our own files
            if !is_balena_file(BBG_UENV_PATH)? {
                let backup_uenv = format!("{}-{}", BBG_UENV_PATH, Local::now().format("%s"));
                std::fs::copy(BBG_UENV_PATH, &backup_uenv).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to file '{}' to '{}'", BBG_UENV_PATH, &backup_uenv),
                ))?;
                info!("copied backup of '{}' to '{}'", BBG_UENV_PATH, &backup_uenv);
                mig_info
                    .boot_cfg_bckup
                    .push((String::from(BBG_UENV_PATH), backup_uenv));
            }
        }

        // **********************************************************************
        // ** create new /uEnv.txt
        let mut uenv_text = UENV_TXT.replace("__KERNEL_PATH__", BBG_MIG_KERNEL_PATH);
        uenv_text = uenv_text.replace("__INITRD_PATH__", BBG_MIG_INITRD_PATH);
        uenv_text = uenv_text.replace("__DRIVE__", &drive_num.0);
        uenv_text = uenv_text.replace("__PARTITION__", &drive_num.1);
        uenv_text = uenv_text.replace(
            "__ROOT_DEV__",
            &mig_info.get_root_device().to_string_lossy(),
        );

        let mut uenv_file = File::create(BBG_UENV_PATH).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to create new '{}'", BBG_UENV_PATH),
        ))?;
        uenv_file
            .write_all(uenv_text.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to write new '{}'", BBG_UENV_PATH),
            ))?;
        info!("created new file in '{}'", BBG_UENV_PATH);
        Ok(())
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration for Beaglebone Green");

        let uenv_file = path_append(root_path, BBG_UENV_PATH);

        if file_exists(&uenv_file) && is_balena_file(&uenv_file)? {
            remove_file(&uenv_file).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to remove migrate boot config file {}",
                    uenv_file.display()
                ),
            ))?;
            info!("Removed balena boot config file '{}'", &uenv_file.display());
            restore_backups(root_path, config.get_backups())?;
        } else {
            warn!(
                "balena boot config file not found in '{}'",
                &uenv_file.display()
            );
        }

        info!("The original boot configuration was restored");

        Ok(())
    }
}
