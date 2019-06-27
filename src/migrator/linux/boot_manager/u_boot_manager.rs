use chrono::Local;
use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use nix::mount::{mount, umount, MsFlags};
use regex::Regex;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::PathBuf;

use crate::{
    common::{
        file_exists, format_size_with_unit, is_balena_file, path_append,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG, MIG_DTB_NAME, MIG_INITRD_NAME, MIG_KERNEL_NAME},
    linux::{
        boot_manager::BootManager,
        linux_common::restore_backups,
        linux_defs::{
            BOOT_PATH, MLO_FILE_NAME, NIX_NONE, ROOT_PATH, UBOOT_FILE_NAME, UENV_FILE_NAME,
        },
        migrate_info::{path_info::PathInfo, MigrateInfo},
        stage2::mounts::Mounts,
        EnsuredCmds, CHMOD_CMD, MKTEMP_CMD,
    },
};

// TODO: this might be  a bit tight
const UBOOT_DRIVE_REGEX: &str = r#"^/dev/mmcblk\d+$"#; //p(\d+)$"#;

const UENV_TXT: &str = r###"
loadaddr=0x82000000
fdtaddr=0x88000000
rdaddr=0x88080000

initrd_high=0xffffffff
fdt_high=0xffffffff

loadximage=echo debug: [__KERNEL_PATH__] ... ; load mmc __DRIVE__:__PARTITION__ ${loadaddr} __KERNEL_PATH__
loadxfdt=echo debug: [__DTB_PATH__] ... ;load mmc __DRIVE__:__PARTITION__ ${fdtaddr} __DTB_PATH__
loadxrd=echo debug: [__INITRD_PATH__] ... ; load mmc __DRIVE__:__PARTITION__ ${rdaddr} __INITRD_PATH__; setenv rdsize ${filesize}
# loaduEnvtxt=load mmc __DRIVE__:__PARTITION__ ${loadaddr} /boot/uEnv.txt ; env import -t ${loadaddr} ${filesize};
check_uboot_overlays=if test -n ${enable_uboot_overlays}; then setenv enable_uboot_overlays ;fi;
loadall=run check_uboot_overlays; run loadximage; run loadxrd; run loadxfdt;

mmcargs=setenv bootargs console=tty0 console=${console} ${optargs} ${cape_disable} ${cape_enable} root=__ROOT_DEV__ rootfstype=${mmcrootfstype} ${cmdline}

uenvcmd=run loadall; run mmcargs; echo debug: [${bootargs}] ... ; echo debug: [bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr}] ... ; bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr};
"###;

pub(crate) struct UBootManager {
    boot_path: Option<PathInfo>,
}

impl UBootManager {
    pub fn new() -> UBootManager {
        UBootManager { boot_path: None }
    }

    // Try to find a drive containing MLO, uEnv.txt or u-boot.bin, mount it if necessarry and return PathInfo if found

    /*
    Find boot manager partition - the partition where we will place our uEnv.txt

    In U-boot boot manager drive will contain  MLO & u-boot.img and possibly uEnv.txt in the root.
    That said MLO & u-boot.img might reside in a special partition or in the MBR and uEnv.txt is
    not mandatory. So neither of them ight be found.
    So current strategy is:
        a) Look for MLO & u-boot.img on all relevant drives. Return that drive if found
           Look only on likely device types:
            /dev/mmcblk[0-9]+p[0-9]+ (avoiding mmcblk[0-9]+boot0 mmcblk[0-9]+boot1 mmcblk[0-9]+rpmb)
            /dev/sd[a-z,A-Z][0-9]+

            mmcblk[0-9]+boot0 mmcblk[0-9]+boot1 mmcblk[0-9]+rpmb indicates bootmanager devices on
            beaglebones.

        b) Use the drive with the /root partition


    */

    fn find_boot_path(
        &self,
        cmds: &EnsuredCmds,
        mig_info: &MigrateInfo,
    ) -> Result<PathInfo, MigError> {
        //lazy_static! {
        //    static ref BOOT_DRIVE_RE: Regex = Regex::new(UBOOT_DRIVE_REGEX).unwrap();
        // }

        // try our luck with /root, /boot

        if file_exists(path_append(ROOT_PATH, MLO_FILE_NAME))
            || file_exists(path_append(ROOT_PATH, UBOOT_FILE_NAME))
        {
            return Ok(PathInfo::new(cmds, ROOT_PATH, &mig_info.lsblk_info)?.unwrap());
        }

        if file_exists(path_append(BOOT_PATH, MLO_FILE_NAME))
            || file_exists(path_append(BOOT_PATH, UBOOT_FILE_NAME))
        {
            return Ok(PathInfo::new(cmds, BOOT_PATH, &mig_info.lsblk_info)?.unwrap());
        }

        let mut tmp_mountpoint: Option<PathBuf> = None;

        for blk_device in mig_info.lsblk_info.get_blk_devices() {
            /*if !BOOT_DRIVE_RE.is_match(&*blk_device.name) {
                debug!("Ignoring: '{}'", blk_device.get_path().display());
                continue;
            }*/

            debug!("Looking at: '{}'", blk_device.get_path().display());

            if let Some(ref partitions) = blk_device.children {
                for partition in partitions {
                    // establish mountpoint / temporarilly mount
                    debug!("looking at '{}' mounted on {:?}", partition.get_path().display(), partition.mountpoint);
                    let (mountpoint, mounted) = match partition.mountpoint {
                        Some(ref mountpoint) => (mountpoint, false),
                        None => {
                            // make mountpoint directory if none exists
                            if let None = tmp_mountpoint {
                                let cmd_res = cmds.call(
                                    MKTEMP_CMD,
                                    &["-d", "-p", &mig_info.work_path.path.to_string_lossy()],
                                    true,
                                )?;
                                if cmd_res.status.success() {
                                    tmp_mountpoint = Some(
                                        PathBuf::from(&cmd_res.stdout).canonicalize().context(
                                            MigErrCtx::from_remark(
                                                MigErrorKind::Upstream,
                                                &format!(
                                                "Failed to canonicalize path to mountpoint '{}'",
                                                cmd_res.stdout
                                            ),
                                            ),
                                        )?,
                                    );
                                } else {
                                    return Err(MigError::from_remark(
                                        MigErrorKind::Upstream,
                                        "Failed to create temporary mount point",
                                    ));
                                }
                            }

                            let mountpoint = tmp_mountpoint.as_ref().unwrap();

                            debug!(" mounting on '{}'", mountpoint.display());

                            let fs_type = if let Some(ref fs_type) = partition.fstype {
                                if fs_type == "vfat" || fs_type.starts_with("ext") {
                                    // expect certain fs types
                                    Some(fs_type.as_bytes())
                                } else {
                                    continue;
                                }
                            } else {
                                NIX_NONE
                            };

                            debug!("temporarilly mounting '{}' on '{}'", partition.get_path().display(), mountpoint.display());

                            mount(
                                Some(&partition.get_path()),
                                mountpoint,
                                fs_type,
                                MsFlags::empty(),
                                NIX_NONE,
                            )
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "Failed to temporarily mount drive '{}' on '{}",
                                    partition.get_path().display(),
                                    tmp_mountpoint.as_ref().unwrap().display()
                                ),
                            ))?;

                            (mountpoint, true)
                        }
                    };

                    debug!("checking '{}', mounted on {}", partition.get_path().display(), mountpoint.display());

                    if file_exists(path_append(mountpoint, MLO_FILE_NAME))
                        || file_exists(path_append(mountpoint, UBOOT_FILE_NAME))
                    {
                        return Ok(PathInfo::from_mounted(
                            cmds, mountpoint, mountpoint, blk_device, &partition,
                        )?);
                    } else {
                        if mounted {
                            umount(mountpoint).context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("Failed to unmount '{}'", mountpoint.display()),
                            ))?;
                        }
                    }
                }
            }
        }

        debug!("No u-boot boot files found, assuming '{}'", ROOT_PATH);

        Ok(PathInfo::new(cmds, ROOT_PATH, &mig_info.lsblk_info)?.unwrap())
    }
}

impl<'a> BootManager for UBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::UBoot
    }

    fn get_boot_path(&self) -> PathInfo {
        self.boot_path.as_ref().unwrap().clone()
    }

    fn can_migrate(
        &mut self,
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr
        trace!("can_migrate: entered");

        // find the u-boot boot device
        // this is where uEnv.txt has to go

        let boot_path = self.find_boot_path(cmds, mig_info)?;
        info!(
            "Found boot manager '{}', mounpoint: '{}', fs type: {}, free space: {}",
            boot_path.device.display(),
            boot_path.mountpoint.display(),
            boot_path.fs_type,
            format_size_with_unit(boot_path.fs_free)
        );

        /*
                    s2_cfg.set_bootmgr_cfg(MountConfig::new(
                        boot_path.device,
                        boot_path.fs_type,
                        boot_path.mountpoint,
                    ));
        */

        let mut boot_req_space = if !file_exists(path_append(&boot_path.path, MIG_KERNEL_NAME)) {
            mig_info.kernel_file.size
        } else {
            0
        };

        boot_req_space += if !file_exists(path_append(&boot_path.path, MIG_INITRD_NAME)) {
            mig_info.initrd_file.size
        } else {
            0
        };

        if let Some(ref dtb_file) = mig_info.dtb_file {
            boot_req_space += if !file_exists(path_append(&boot_path.path, MIG_DTB_NAME)) {
                dtb_file.size
            } else {
                0
            };
        } else {
            error!("The device tree blob file required for u-boot is not defined.");
            return Ok(false);
        }

        if boot_path.fs_free < boot_req_space {
            error!("The boot directory '{}' does not have enough space to store the migrate kernel, initramfs and dtb file. Required space is {}",
                   boot_path.path.display(), format_size_with_unit(boot_req_space));
            return Ok(false);
        }

        self.boot_path = Some(boot_path);

        Ok(true)
    }

    fn setup(
        &self,
        cmds: &EnsuredCmds,
        _mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        // **********************************************************************
        // ** read drive number & partition number from boot device
        let boot_path = self.boot_path.as_ref().unwrap();

        let drive_num = {
            let dev_name = &boot_path.device;

            if let Some(captures) = Regex::new(UBOOT_DRIVE_REGEX)
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

        let source_path = config.migrate.get_kernel_path();
        let kernel_path = path_append(&boot_path.path, MIG_KERNEL_NAME);
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

        cmds.call(CHMOD_CMD, &["+x", &kernel_path.to_string_lossy()], false)?;

        let source_path = config.migrate.get_initrd_path();
        let initrd_path = path_append(&boot_path.path, MIG_INITRD_NAME);
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

        let dtb_path = if let Some(source_path) = config.migrate.get_dtb_path() {
            let dtb_path = path_append(&boot_path.path, MIG_DTB_NAME);
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
            dtb_path
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("The device tree blob (dtb_file) could not be found"),
            ));
        };

        let uenv_path = path_append(&boot_path.path, UENV_FILE_NAME);

        if file_exists(&uenv_path) {
            // **********************************************************************
            // ** backup /uEnv.txt if exists
            if !is_balena_file(&uenv_path)? {
                let backup_uenv = format!(
                    "{}-{}",
                    &uenv_path.to_string_lossy(),
                    Local::now().format("%s")
                );
                std::fs::copy(&uenv_path, &backup_uenv).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "failed to file '{}' to '{}'",
                        uenv_path.display(),
                        &backup_uenv
                    ),
                ))?;
                info!(
                    "copied backup of '{}' to '{}'",
                    uenv_path.display(),
                    &backup_uenv
                );

                let mut boot_cfg_bckup: Vec<(String, String)> = Vec::new();
                boot_cfg_bckup.push((String::from(&*uenv_path.to_string_lossy()), backup_uenv));

                s2_cfg.set_boot_bckup(boot_cfg_bckup);
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
        uenv_text = uenv_text.replace("__ROOT_DEV__", &boot_path.get_kernel_cmd());

        debug!("writing uEnv.txt as:\n {}", uenv_text);

        let mut uenv_file = File::create(&uenv_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to create new '{}'", uenv_path.display()),
        ))?;
        uenv_file
            .write_all(uenv_text.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to write new '{}'", uenv_path.display()),
            ))?;
        info!("created new file in '{}'", uenv_path.display());
        Ok(())
    }

    fn restore(&self, mounts: &Mounts, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration",);

        // TODO: restore on bootmgr device

        let uenv_file = path_append(mounts.get_boot_mountpoint(), UENV_FILE_NAME);

        if file_exists(&uenv_file) && is_balena_file(&uenv_file)? {
            remove_file(&uenv_file).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to remove migrate boot config file {}",
                    uenv_file.display()
                ),
            ))?;
            info!("Removed balena boot config file '{}'", &uenv_file.display());

            restore_backups(mounts.get_boot_mountpoint(), config.get_boot_backups())?;
        } else {
            warn!(
                "balena boot config file not found in '{}'",
                &uenv_file.display()
            );
        }

        // TODO: remove kernel & initramfs, dtb  too

        info!("The original boot configuration was restored");

        Ok(())
    }
}
