use chrono::Local;
use failure::ResultExt;
use log::{debug, info, trace, warn};
use nix::mount::{mount, umount, MsFlags};
use regex::Regex;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{
    boot_manager::{BootManager, BootType},
    common::{
        file_exists, format_size_with_unit, is_balena_file, path_append, Config, MigErrCtx,
        MigError, MigErrorKind,
    },
    defs::{
        BALENA_FILE_TAG, MIG_DTB_NAME, MIG_INITRD_NAME, MIG_KERNEL_NAME, MLO_FILE_NAME, NIX_NONE,
        ROOT_PATH, UBOOT_FILE_NAME, UENV_FILE_NAME,
    },
    linux_common::{
        migrate_info::{path_info::PathInfo, MigrateInfo},
        restore_backups, CHMOD_CMD, MKTEMP_CMD,
    },
    stage2::stage2_config::{BootMgrConfig, Stage2Config, Stage2ConfigBuilder},
};

const UBOOT_DRIVE_REGEX: &str = r#"^/dev/mmcblk(\d+)p(\d+)$"#;

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

pub(crate) struct UBootManager;

impl UBootManager {
    pub fn new() -> UBootManager {
        UBootManager {}
    }

    // Try to find a drive containing MLO, uEnv.txt or u-boot.bin, mount it if necessarry and return PathInfo if found
    fn get_bootmgr_path(&self, mig_info: &MigrateInfo) -> Result<Option<PathInfo>, MigError> {
        trace!("set_bootmgr_path: entered");

        let (root_dev, root_part) = mig_info.lsblk_info.get_path_info(ROOT_PATH)?;

        let mut tmp_mountpoint: Option<PathBuf> = None;

        if let Some(ref children) = root_dev.children {
            for partition in children {
                debug!(
                    "set_bootmgr_path: checking '{}' fstype: {:?}",
                    partition.name, partition.fstype
                );
                if let Some(ref fstype) = partition.fstype {
                    if fstype == "vfat" || fstype.starts_with("ext") {
                        debug!(
                            "get_uboot_mgr_path: attempting to scan '{}' for u-boot files",
                            partition.name
                        );
                        let mut mounted = false;
                        let mountpoint = match partition.mountpoint {
                            Some(ref mountpoint) => mountpoint,
                            None => {
                                // darn ! we will have to mount it
                                debug!(
                                    "get_uboot_mgr_path: attempting to mount '{}'",
                                    partition.name
                                );
                                if let None = tmp_mountpoint {
                                    let cmd_res = mig_info.cmds.call_cmd(
                                        MKTEMP_CMD,
                                        &["-d", "-p", &mig_info.work_path.path.to_string_lossy()],
                                        true,
                                    )?;
                                    if cmd_res.status.success() {
                                        tmp_mountpoint = Some(
                                            PathBuf::from(&cmd_res.stdout)
                                                .canonicalize()
                                                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to canonicalize path to mountpoint '{}'", cmd_res.stdout)))?);
                                    } else {
                                        return Err(MigError::from_remark(
                                            MigErrorKind::Upstream,
                                            "Failed to create temporary mount point",
                                        ));
                                    }
                                }

                                let mountpoint = tmp_mountpoint.as_ref().unwrap();
                                debug!("get_uboot_mgr_path: mountpoint '{}'", mountpoint.display());

                                mount(
                                    Some(&partition.get_path()),
                                    mountpoint,
                                    Some(fstype.as_bytes()),
                                    MsFlags::empty(),
                                    NIX_NONE,
                                )
                                .context(
                                    MigErrCtx::from_remark(
                                        MigErrorKind::Upstream,
                                        &format!(
                                            "Failed to temporarilly mount drive '{}' on '{}",
                                            partition.get_path().display(),
                                            tmp_mountpoint.as_ref().unwrap().display()
                                        ),
                                    ),
                                )?;

                                mounted = true;
                                mountpoint
                            }
                        };

                        if file_exists(path_append(mountpoint, MLO_FILE_NAME))
                            || file_exists(path_append(mountpoint, UENV_FILE_NAME))
                            || file_exists(path_append(mountpoint, UBOOT_FILE_NAME))
                        {
                            debug!(
                                "get_uboot_mgr_path: found u-boot files on {}",
                                partition.name
                            );
                            return Ok(Some(PathInfo::from_mounted(
                                &mig_info.cmds,
                                mountpoint,
                                mountpoint,
                                &root_dev,
                                &partition,
                            )?));
                        }

                        if mounted {
                            umount(mountpoint).context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("Failed to unmount '{}'", mountpoint.display()),
                            ))?;
                        }

                        debug!("get_uboot_mgr_path: nothing found on {}", partition.name);
                    }
                }
            }
            debug!("get_uboot_mgr_path: nothing found");
            Ok(None)
        } else {
            panic!("root drive must have children");
        }
    }
}

impl BootManager for UBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::UBoot
    }

    fn can_migrate(
        &self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr

        Ok(false)
    }

    fn setup(
        &self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        // **********************************************************************
        // ** read drive number & partition number from boot device
        let boot_path = &mig_info.boot_path;

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

        // find the u-boot boot device
        // this is where uEnv.txt has to go

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

        mig_info
            .cmds
            .call_cmd(CHMOD_CMD, &["+x", &kernel_path.to_string_lossy()], false)?;

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

        // **********************************************************************
        // ** find relevant path for boot manager, either /boot or a separate
        // ** boot manager partition
        let bootmgr_path = if let Some(bootmgr_path) = self.get_bootmgr_path(mig_info)? {
            let path = bootmgr_path.mountpoint.clone();

            info!(
                "Found boot manager '{}', mounpoint: '{}', fs type: {}, free space: {}",
                bootmgr_path.device.display(),
                bootmgr_path.mountpoint.display(),
                bootmgr_path.fs_type,
                format_size_with_unit(bootmgr_path.fs_free)
            );

            s2_cfg.set_bootmgr_cfg(BootMgrConfig::new(
                bootmgr_path.device,
                bootmgr_path.fs_type,
                bootmgr_path.mountpoint,
            ));

            path
        } else {
            mig_info.root_path.path.clone()
        };

        let uenv_path = path_append(bootmgr_path, UENV_FILE_NAME);

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
        uenv_text = uenv_text.replace("__ROOT_DEV__", &mig_info.root_path.device.to_string_lossy());

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

    fn restore(&self, slug: &str, root_path: &Path, config: &Stage2Config) -> Result<(), MigError> {
        info!("restoring boot configuration for {}", slug);

        // TODO: restore on bootmgr device
        let uenv_file = if let Some(bootmgr) = config.get_bootmgr_config() {
            path_append(
                path_append(root_path, bootmgr.get_mountpoint()),
                UENV_FILE_NAME,
            )
        } else {
            path_append(root_path, UENV_FILE_NAME)
        };

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
}
