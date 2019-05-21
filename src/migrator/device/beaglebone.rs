use chrono::Local;
use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::Path;

use crate::device::u_boot_valid;
use crate::{
    common::{
        file_exists, is_balena_file, path_append, BootType, Config, MigErrCtx, MigError,
        MigErrorKind,
    },
    defs::{BALENA_FILE_TAG, MIG_DTB_NAME, MIG_INITRD_NAME, MIG_KERNEL_NAME},
    device::{grub_install, Device},
    linux_common::{
        call_cmd, disk_info::DiskInfo, migrate_info::MigrateInfo, restore_backups, CHMOD_CMD,
    },
    stage2::Stage2Config,
};

const BB_DRIVE_REGEX: &str = r#"^/dev/mmcblk(\d+)p(\d+)$"#;

// Supported models
// TI OMAP3 BeagleBoard xM
const BB_MODEL_REGEX: &str = r#"^((\S+\s+)*\S+)\s+Beagle(Bone|Board)\s+(\S+)$"#;

const BBG_UENV_PATH: &str = "/uEnv.txt";

const UENV_TXT: &str = r###"
loadaddr=0x82000000
fdtaddr=0x88000000
rdaddr=0x88080000

initrd_high=0xffffffff
fdt_high=0xffffffff

##These are needed to be compliant with Debian 2014-05-14 u-boot.

loadximage=echo debug: [__KERNEL_PATH__] ... ; load mmc __DRIVE__:__PARTITION__ ${loadaddr} __KERNEL_PATH__
loadxfdt=echo debug: [__DTB_PATH__] ... ;load mmc __DRIVE__:__PARTITION__ ${fdtaddr} __DTB_PATH__
loadxrd=echo debug: [__INITRD_PATH__] ... ; load mmc __DRIVE__:__PARTITION__ ${rdaddr} __INITRD_PATH__; setenv rdsize ${filesize}
loaduEnvtxt=load mmc __DRIVE__:__PARTITION__ ${loadaddr} /boot/uEnv.txt ; env import -t ${loadaddr} ${filesize};
check_uboot_overlays=if test -n ${enable_uboot_overlays}; then setenv enable_uboot_overlays ;fi;
# loadall=run loaduEnvtxt; run check_dtb; run check_uboot_overlays; run loadximage; run loadxrd; run loadxfdt;
loadall=run run check_dtb; run check_uboot_overlays; run loadximage; run loadxrd; run loadxfdt;

mmcargs=setenv bootargs console=tty0 console=${console} ${optargs} ${cape_disable} ${cape_enable} root=__ROOT_DEV__ rootfstype=${mmcrootfstype} ${cmdline}

uenvcmd=run loadall; run mmcargs; echo debug: [${bootargs}] ... ; echo debug: [bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr}] ... ; bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr};
"###;

// TODO: check location of uEnv.txt or other files files to improve reliability

pub(crate) fn is_bb(model_string: &str) -> Result<Box<Device>, MigError> {
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
                Ok(Box::new(BeagleboardXM {}))
            }
            "Green" => {
                debug!("match found for BeagleboardGreen");
                Ok(Box::new(BeagleboneGreen {}))
            }
            _ => {
                let message = format!("The beaglebone model reported by your device ('{}') is not supported by balena-migrate", model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        debug!("no match for beaglebone on: {}", model_string);
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

    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        def_setup(self.get_device_slug(), config, mig_info)
    }

    fn can_migrate(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        def_can_migrate(self.get_device_slug(), config, mig_info)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        def_restore_boot(self.get_device_slug(), root_path, config)
    }
}

pub(crate) struct BeagleboardXM {}

impl BeagleboardXM {
    pub(crate) fn new() -> BeagleboardXM {
        BeagleboardXM {}
    }
}

impl<'a> Device for BeagleboardXM {
    fn get_device_slug(&self) -> &'static str {
        // TODO: check if that is true
        "beagleboard-xm"
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        def_restore_boot(self.get_device_slug(), root_path, config)
    }

    fn can_migrate(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        def_can_migrate(self.get_device_slug(), config, mig_info)
    }

    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        def_setup(self.get_device_slug(), config, mig_info)
    }
}

fn def_can_migrate(
    slug: &str,
    config: &Config,
    mig_info: &mut MigrateInfo,
) -> Result<bool, MigError> {
    const SUPPORTED_OSSES: &'static [&'static str] = &["Ubuntu 18.04.2 LTS"];

    let os_name = mig_info.get_os_name();

    if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
        error!("The OS '{}' is not supported for '{}'", os_name, slug);
        return Ok(false);
    }

    if !u_boot_valid(mig_info)? {
        return Ok(false);
    }

    if let None = mig_info.get_dtb_path() {
        error!("The device tree blob file (dtb_file) required for uboot was not found");
        return Ok(false);
    }

    mig_info.boot_type = Some(BootType::UBoot);

    mig_info.disk_info = Some(DiskInfo::new(
        false,
        &config.migrate.get_work_dir(),
        config.migrate.get_log_device(),
    )?);

    if mig_info.get_boot_path().drive != mig_info.get_root_path().drive {
        error!(
            "The partition layout is not supported, /boot and / are required to be on the same harddrive",
        );
        return Ok(false);
    }

    mig_info.install_path = Some(mig_info.get_root_path().clone());

    Ok(true)
}

fn def_setup(slug: &str, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
    trace!("setup: entered with type: '{}'", slug);

    if let Some(ref boot_type) = mig_info.boot_type {
        match boot_type {
            BootType::UBoot => setup_uboot(mig_info),
            BootType::GRUB => grub_install(config, mig_info),
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Invalid boot type for '{}' : {:?}'",
                    slug, mig_info.boot_type
                ),
            )),
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("No boot type specified for '{}'", slug),
        ))
    }
}

fn setup_uboot(mig_info: &mut MigrateInfo) -> Result<(), MigError> {
    // **********************************************************************
    // ** read drive number & partition number from boot device
    let drive_num = {
        let dev_name = &mig_info.get_boot_path().device;

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

    let source_path = mig_info.get_kernel_path();
    let kernel_path = path_append(&mig_info.get_boot_path().path, MIG_KERNEL_NAME);
    std::fs::copy(&source_path, &kernel_path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "failed to copy kernel file '{}' to '{}'",
            source_path.display(),
            kernel_path.display()
        ),
    ))?;
    info!(
        "copied kernel: '{}' -> '{}'",
        source_path.display(),
        kernel_path.display()
    );

    call_cmd(CHMOD_CMD, &["+x", &kernel_path.to_string_lossy()], false)?;

    let source_path = mig_info.get_initrd_path();
    let initrd_path = path_append(&mig_info.get_boot_path().path, MIG_INITRD_NAME);
    std::fs::copy(&source_path, &initrd_path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "failed to copy initrd file '{}' to '{}'",
            source_path.display(),
            initrd_path.display()
        ),
    ))?;
    info!(
        "initramfs file: '{}' -> '{}'",
        source_path.display(),
        initrd_path.display()
    );

    let dtb_path = path_append(&mig_info.get_boot_path().path, MIG_DTB_NAME);
    if let Some(source_path) = mig_info.get_dtb_path() {
        std::fs::copy(&source_path, &dtb_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy dtb file '{}' to '{}'",
                source_path.display(),
                dtb_path.display()
            ),
        ))?;
        info!(
            "dtb file: '{}' -> '{}'",
            source_path.display(),
            dtb_path.display()
        );
    } else {
        return Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("The device tree blob (dtb_file) could not be found"),
        ));
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
    let mut uenv_text = String::from(BALENA_FILE_TAG);
    uenv_text.push_str(UENV_TXT);
    uenv_text = uenv_text.replace("__KERNEL_PATH__", &kernel_path.to_string_lossy());
    uenv_text = uenv_text.replace("__INITRD_PATH__", &initrd_path.to_string_lossy());
    uenv_text = uenv_text.replace("__DTB_PATH__", &dtb_path.to_string_lossy());
    uenv_text = uenv_text.replace("__DRIVE__", &drive_num.0);
    uenv_text = uenv_text.replace("__PARTITION__", &drive_num.1);
    uenv_text = uenv_text.replace(
        "__ROOT_DEV__",
        &mig_info.get_root_path().device.to_string_lossy(),
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

fn def_restore_boot(slug: &str, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
    info!("restoring boot configuration for {}", slug);

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
        restore_backups(root_path, config.get_boot_backups())?;
    } else {
        warn!(
            "balena boot config file not found in '{}'",
            &uenv_file.display()
        );
    }

    info!("The original boot configuration was restored");

    Ok(())
}
