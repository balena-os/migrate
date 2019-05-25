use log::{error, info};
use std::path::Path;

use crate::{
    common::{Config, MigError, MigErrorKind},
    device::{Device, DeviceType},    
    linux_common::{is_secure_boot, restore_backups, device_info::DeviceInfo},
    stage2::stage2_config::{Stage2Config, Stage2ConfigBuilder},
    boot_manager::{BootType, BootManager, GrubBootManager, from_boot_type}
};

pub(crate) struct IntelNuc {
    boot_manager: Box<BootManager>,
}

impl IntelNuc {
    pub fn from_config(dev_info: &DeviceInfo, config: &Config,  s2_cfg: &mut Stage2ConfigBuilder) -> Result<IntelNuc,MigError> {
        const SUPPORTED_OSSES: &'static [&'static str] = &[
            "Ubuntu 18.04.2 LTS",
            "Ubuntu 16.04.2 LTS",
            "Ubuntu 14.04.2 LTS",
            "Ubuntu 14.04.5 LTS",
        ];

        let os_name = &dev_info.os_name;
        if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
            let message = format!("The OS '{}' is not supported for device type IntelNuc",os_name,);
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        // **********************************************************************
        // ** AMD64 specific initialisation/checks
        // **********************************************************************

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

        Ok(IntelNuc{ boot_manager: Box::new(GrubBootManager{})})
    }

    pub fn from_boot_type(boot_type: &BootType) -> IntelNuc {
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

    fn setup(&self, dev_info: &DeviceInfo, config: &Config, s2_cfg: &mut Stage2ConfigBuilder) -> Result<(), MigError> {
        self.boot_manager.setup(dev_info, config, s2_cfg)
    }

    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        self.boot_manager.restore(self.get_device_slug(), root_path, config)
    }
}

/*

pub(crate) fn grub_valid(_config: &Config, _mig_info: &MigrateInfo) -> Result<bool, MigError> {
    let grub_version = match get_grub_version() {
        Ok(version) => version,
        Err(why) => match why.kind() {
            MigErrorKind::NotFound => {
                warn!("The grub version could not be established, grub does not appear to be installed");
                return Ok(false);
            }
            _ => return Err(why),
        },
    };

    // TODO: check more indications of a valid grub installation
    // TODO: really expect versions > 2 to be downwards compatible ?
    debug!("found update-grub version: '{:?}'", grub_version);
    Ok(grub_version
        .0
        .parse::<u8>()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to establish grub version from '{}'", grub_version.0),
        ))?
        >= 2)
}

pub(crate) fn grub_install(_config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
    // TODO: implement
    // a) look for grub, ensure version
    // b) create a boot config for balena migration
    // c) call grub-reboot to enable boot once to migrate env

    // let install_drive = mig_info.get_installPath().drive;
    let boot_path = mig_info.get_boot_pi();
    let root_path = mig_info.get_root_pi();

    /*
        let grub_root = if Some(uuid) = root_path.uuid {
            format!("root=UUID={}", uuid)
        } else {
            if let Some(uuid) = root_path.part_uuid {
                format!("root=PARTUUID={}", uuid)
            } else {
                format!("root={}", &root_path.path.to_string_lossy());
            }
        };
    */

    let grub_boot = if boot_path.device == root_path.device {
        PathBuf::from(BOOT_PATH)
    } else {
        if boot_path.mountpoint == Path::new(BOOT_PATH) {
            PathBuf::from(ROOT_PATH)
        } else {
            // TODO: create appropriate path
            panic!("boot partition mus be mounted in /boot for now");
        }
    };

    let part_type = match LabelType::from_device(&boot_path.drive)? {
        LabelType::GPT => "gpt",
        LabelType::DOS => "msdos",
        _ => {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("Invalid partition type for '{}'", boot_path.drive.display()),
            ));
        }
    };

    let part_mod = format!("part_{}", part_type);

    info!(
        "Boot partition type is '{}' is type '{}'",
        boot_path.drive.display(),
        part_mod
    );

    let root_cmd = if let Some(ref uuid) = boot_path.uuid {
        // TODO: try partuuid too ?local setRootA="set root='${GRUB_BOOT_DEV},msdos${ROOT_PART_NO}'"
        format!("search --no-floppy --fs-uuid --set=root {}", uuid)
    } else {
        format!(
            "search --no-floppy --fs-uuid --set=root {},{}{}",
            boot_path.drive.to_string_lossy(),
            part_type,
            boot_path.index
        )
    };

    debug!("root set to '{}", root_cmd);

    let fstype_mod = match boot_path.fs_type.as_str() {
        "ext2" | "ext3" | "ext4" => "ext2",
        "vfat" => "fat",
        _ => {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Cannot determine grub mod for boot fs type '{}'",
                    boot_path.fs_type
                ),
            ));
        }
    };

    let mut linux = String::from(path_append(&grub_boot, MIG_KERNEL_NAME).to_string_lossy());

    // filter some bullshit out of commandline, else leave it as is

    for word in read_to_string(KERNEL_CMDLINE_PATH)
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Unable to read kernel command line from '{}'",
                KERNEL_CMDLINE_PATH
            ),
        ))?
        .split_whitespace()
        {
            let word_lc = word.to_lowercase();
            if word_lc.starts_with("boot_image=") {
                continue;
            }

            if word.to_lowercase() == "debug" {
                continue;
            }

            if word.starts_with("rootfstype=") {
                continue;
            }

            linux.push_str(&format!(" {}", word));
        }

    linux.push_str(&format!(" rootfstype={} debug", root_path.fs_type));

    let mut grub_cfg = String::from(GRUB_CFG_TEMPLATE);

    grub_cfg = grub_cfg.replace("__PART_MOD__", &part_mod);
    grub_cfg = grub_cfg.replace("__FSTYPE_MOD__", &fstype_mod);
    grub_cfg = grub_cfg.replace("__ROOT_CMD__", &root_cmd);
    grub_cfg = grub_cfg.replace("__LINUX__", &linux);
    grub_cfg = grub_cfg.replace(
        "__INITRD_NAME__",
        &path_append(&grub_boot, MIG_INITRD_NAME).to_string_lossy(),
    );

    debug!("grub config: {}", grub_cfg);

    // let mut grub_cfg_file =
    File::create(GRUB_CONF_PATH)
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to create grub config file '{}'", GRUB_CONF_PATH),
        ))?
        .write(grub_cfg.as_bytes())
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to write to grub config file '{}'", GRUB_CONF_PATH),
        ))?;

    let cmd_res = call_cmd(CHMOD_CMD, &["+x", GRUB_CONF_PATH], true)?;
    if !cmd_res.status.success() {
        return Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!("Failure from '{}': {:?}", CHMOD_CMD, cmd_res),
        ));
    }

    info!("Grub config written to '{}'", GRUB_CONF_PATH);

    // **********************************************************************
    // ** copy new kernel & iniramfs

    let source_path = mig_info.get_kernel_path();
    let kernel_path = path_append(&mig_info.get_boot_pi().path, MIG_KERNEL_NAME);
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
    let initrd_path = path_append(&mig_info.get_boot_pi().path, MIG_INITRD_NAME);
    std::fs::copy(&source_path, &initrd_path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "failed to copy initrd file '{}' to '{}'",
            source_path.display(),
            initrd_path.display()
        ),
    ))?;
    info!(
        "initramfs kernel: '{}' -> '{}'",
        source_path.display(),
        initrd_path.display()
    );

    let grub_path = match whereis(GRUB_UPDT_CMD) {
        Ok(path) => path,
        Err(why) => {
            warn!(
                "The grub rupdate command '{}' could not be found",
                GRUB_UPDT_CMD
            );
            return Err(MigError::from(why.context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to find command {}", GRUB_UPDT_CMD),
            ))));
        }
    };

    let grub_args = [];
    let cmd_res = call(&grub_path, &grub_args, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        "Failed to set up boot configuration'",
    ))?;

    if !cmd_res.status.success() {
        return Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!("Failure from '{}': {:?}", GRUB_UPDT_CMD, cmd_res),
        ));
    }

    let grub_path = match whereis(GRUB_REBOOT_CMD) {
        Ok(path) => path,
        Err(why) => {
            warn!(
                "The grub reboot update command '{}' could not be found",
                GRUB_REBOOT_CMD
            );
            return Err(MigError::from(why.context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to find command {}", GRUB_REBOOT_CMD),
            ))));
        }
    };

    let grub_args = ["balena-migrate"];
    let cmd_res = call(&grub_path, &grub_args, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "Failed to activate boot configuration using '{}'",
            GRUB_REBOOT_CMD,
        ),
    ))?;

    if !cmd_res.status.success() {
        return Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "Failed to activate boot configuration using '{}': {:?}",
                GRUB_REBOOT_CMD, cmd_res
            ),
        ));
    }

    Ok(())
}
*/