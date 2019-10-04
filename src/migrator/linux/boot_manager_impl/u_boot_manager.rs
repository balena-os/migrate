use chrono::Local;
use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use nix::mount::{mount, umount, MsFlags};
use regex::Regex;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::PathBuf;

use crate::common::file_digest::check_digest;
use crate::linux::lsblk_info::LsblkInfo;
use crate::{
    common::{
        boot_manager::BootManager,
        call,
        device_info::DeviceInfo,
        file_exists, format_size_with_unit, is_balena_file,
        migrate_info::MigrateInfo,
        path_append,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG, MIG_DTB_NAME, MIG_INITRD_NAME, MIG_KERNEL_NAME},
    linux::{
        linux_common::restore_backups,
        linux_defs::{
            BOOT_PATH, MLO_FILE_NAME, NIX_NONE, ROOT_PATH, UBOOT_FILE_NAME, UENV_FILE_NAME,
        },
        linux_defs::{CHMOD_CMD, MKTEMP_CMD},
        stage2::mounts::Mounts,
    },
};

// TODO: this might be a bit of a tight fit, allow (s|h)d([a-z])(\d+) too ?
const UBOOT_DRIVE_FILTER_REGEX: &str = r#"^mmcblk\d+$"#;
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

mmcargs=setenv bootargs console=tty0 console=${console} ${optargs} ${cape_disable} ${cape_enable} root=__ROOT_DEV__ rootfstype=__ROOT_FSTYPE__ __MISC_OPTS__ ${cmdline}

uenvcmd=run loadall; run mmcargs; echo debug: [${bootargs}] ... ; echo debug: [bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr}] ... ; bootz ${loadaddr} ${rdaddr}:${rdsize} ${fdtaddr};
"###;

pub(crate) struct UBootManager {
    // location of MLO / u-boot.img files, this is where we put our uEnv.txt and our kernel / initrd / dtb
    // if sufficient space is available
    bootmgr_path: Option<PathInfo>,
    // this is an alt location for our kernel / initrd / dtb if bootmgr is too small
    boot_path: Option<PathInfo>,
}

impl UBootManager {
    pub fn new() -> UBootManager {
        UBootManager {
            bootmgr_path: None,
            boot_path: None,
        }
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

    fn find_bootmgr_path(
        &self,
        mig_info: &MigrateInfo,
        lsblk_info: &LsblkInfo,
    ) -> Result<PathInfo, MigError> {
        lazy_static! {
            // same as ab
            static ref BOOT_DRIVE_RE: Regex = Regex::new(UBOOT_DRIVE_FILTER_REGEX).unwrap();
        }

        // try our luck with /root, /boot

        if file_exists(path_append(ROOT_PATH, MLO_FILE_NAME))
            || file_exists(path_append(ROOT_PATH, UBOOT_FILE_NAME))
        {
            return Ok(PathInfo::from_path(ROOT_PATH)?);
        }

        if file_exists(path_append(BOOT_PATH, MLO_FILE_NAME))
            || file_exists(path_append(BOOT_PATH, UBOOT_FILE_NAME))
        {
            return Ok(PathInfo::from_path(BOOT_PATH)?);
        }

        let mut tmp_mountpoint: Option<PathBuf> = None;

        for blk_device in lsblk_info.get_blk_devices() {
            if !BOOT_DRIVE_RE.is_match(&*blk_device.name) {
                debug!("Ignoring: '{}'", blk_device.get_path().display());
                continue;
            }

            debug!("Looking at: '{}'", blk_device.get_path().display());

            if let Some(ref partitions) = blk_device.children {
                for partition in partitions {
                    // establish mountpoint / temporarilly mount
                    debug!(
                        "looking at '{}' mounted on {:?}",
                        partition.get_path().display(),
                        partition.mountpoint
                    );
                    let (mountpoint, mounted) = match partition.mountpoint {
                        Some(ref mountpoint) => (mountpoint, false),
                        None => {
                            // make mountpoint directory if none exists
                            if let None = tmp_mountpoint {
                                debug!("creating mountpoint");
                                let cmd_res = call(
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

                            debug!(
                                "temporarilly mounting '{}' on '{}'",
                                partition.get_path().display(),
                                mountpoint.display()
                            );

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
                                    mountpoint.display()
                                ),
                            ))?;

                            (mountpoint, true)
                        }
                    };

                    debug!(
                        "checking '{}', mounted on {}",
                        partition.get_path().display(),
                        mountpoint.display()
                    );

                    if file_exists(path_append(mountpoint, MLO_FILE_NAME))
                        || file_exists(path_append(mountpoint, UBOOT_FILE_NAME))
                    {
                        return Ok(PathInfo::from_mounted(
                            mountpoint, mountpoint, blk_device, &partition,
                        )?);
                    } else {
                        if mounted {
                            debug!(
                                "unmouting '{}', from {}",
                                partition.get_path().display(),
                                mountpoint.display()
                            );
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

        Ok(PathInfo::from_path(ROOT_PATH)?)
    }
}

impl<'a> BootManager for UBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::UBoot
    }

    fn get_bootmgr_path(&self) -> DeviceInfo {
        self.bootmgr_path.as_ref().unwrap().device_info.clone()
    }

    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr
        trace!("can_migrate: entered");

        // find the u-boot boot device
        // this is where uEnv.txt has to go
        let lsblk_info = LsblkInfo::all()?;

        let bootmgr_path = self.find_bootmgr_path(mig_info, &lsblk_info)?;
        info!(
            "Found boot manager '{}', mounpoint: '{}', fs type: {}, free space: {}",
            bootmgr_path.device_info.device,
            bootmgr_path.device_info.mountpoint.display(),
            bootmgr_path.device_info.fs_type,
            format_size_with_unit(bootmgr_path.fs_free)
        );

        let mut boot_req_space: u64 = 8 * 1024; // one 8KiB extra space just in case and for uEnv.txt)
        boot_req_space += if !file_exists(path_append(&bootmgr_path.path, MIG_KERNEL_NAME)) {
            mig_info.kernel_file.size
        } else {
            0
        };

        boot_req_space += if !file_exists(path_append(&bootmgr_path.path, MIG_INITRD_NAME)) {
            mig_info.initrd_file.size
        } else {
            0
        };

        // TODO: support multiple dtb files ?
        if let Some(dtb_file) = mig_info.dtb_file.get(0) {
            boot_req_space += if !file_exists(path_append(&bootmgr_path.path, MIG_DTB_NAME)) {
                dtb_file.size
            } else {
                0
            };
        } else {
            error!("The device tree blob file required for u-boot is not defined.");
            return Ok(false);
        }

        if bootmgr_path.fs_free < boot_req_space {
            // find alt location for boot config

            if let Ok(boot_path) = PathInfo::from_path(BOOT_PATH) {
                if boot_path.fs_free > boot_req_space {
                    info!(
                        "Found boot '{}', mounpoint: '{}', fs type: {}, free space: {}",
                        boot_path.device_info.device,
                        boot_path.device_info.mountpoint.display(),
                        boot_path.device_info.fs_type,
                        format_size_with_unit(boot_path.fs_free)
                    );

                    self.bootmgr_path = Some(bootmgr_path);
                    self.boot_path = Some(boot_path);
                }
            } else {
                if let Ok(boot_path) = PathInfo::from_path(ROOT_PATH) {
                    if boot_path.fs_free > boot_req_space {
                        info!(
                            "Found boot '{}', mounpoint: '{}', fs type: {}, free space: {}",
                            boot_path.device_info.device,
                            boot_path.device_info.mountpoint.display(),
                            boot_path.device_info.fs_type,
                            format_size_with_unit(boot_path.fs_free)
                        );

                        self.bootmgr_path = Some(bootmgr_path);
                        self.boot_path = Some(boot_path);
                    } else {
                        error!("Could not find a directory with sufficient space to store the migrate kernel, initramfs and dtb file. Required space is {}",
                               format_size_with_unit(boot_req_space));
                        return Ok(false);
                    }
                } else {
                    error!("Could not find a directory with sufficient space to store the migrate kernel, initramfs and dtb file. Required space is {}",
                           format_size_with_unit(boot_req_space));
                    return Ok(false);
                }
            }
        } else {
            self.bootmgr_path = Some(bootmgr_path);
        }

        Ok(true)
    }

    fn setup(
        &self,
        mig_info: &MigrateInfo,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        // **********************************************************************
        // ** read drive number & partition number from boot device
        let bootmgr_path = self.bootmgr_path.as_ref().unwrap();
        let boot_path = if let Some(ref boot_path) = self.boot_path {
            // TODO: save to s2_cfg - only really needed if we want to delete it
            boot_path
        } else {
            bootmgr_path
        };

        let drive_num = {
            let dev_name = &boot_path.device_info.device;

            if let Some(captures) = Regex::new(UBOOT_DRIVE_REGEX).unwrap().captures(&dev_name) {
                (
                    String::from(captures.get(1).unwrap().as_str()),
                    String::from(captures.get(2).unwrap().as_str()),
                )
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "failed to parse drive & partition numbers from boot device name '{}'",
                        dev_name
                    ),
                ));
            }
        };

        // **********************************************************************
        // ** copy new kernel & iniramfs

        let kernel_path = path_append(&boot_path.path, MIG_KERNEL_NAME);
        std::fs::copy(&mig_info.kernel_file.path, &kernel_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy kernel file '{}' to '{}'",
                mig_info.kernel_file.path.display(),
                kernel_path.display()
            ),
        ))?;

        if !check_digest(&kernel_path, &mig_info.kernel_file.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied kernel file '{}' to {:?}",
                    kernel_path.display(),
                    mig_info.kernel_file.hash_info
                ),
            ));
        }

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            kernel_path.display()
        );

        call(CHMOD_CMD, &["+x", &kernel_path.to_string_lossy()], false)?;

        let initrd_path = path_append(&boot_path.path, MIG_INITRD_NAME);
        std::fs::copy(&mig_info.initrd_file.path, &initrd_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy initrd file '{}' to '{}'",
                mig_info.initrd_file.path.display(),
                initrd_path.display()
            ),
        ))?;

        if !check_digest(&initrd_path, &mig_info.initrd_file.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied initrd file '{}' to {:?}",
                    initrd_path.display(),
                    mig_info.initrd_file.hash_info
                ),
            ));
        }

        info!(
            "initramfs file: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            initrd_path.display()
        );

        let dtb_path = if let Some(dtb_file) = &mig_info.dtb_file.get(0) {
            let dtb_path = path_append(&boot_path.path, MIG_DTB_NAME);
            std::fs::copy(&dtb_file.path, &dtb_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to copy dtb file '{}' to '{}'",
                    dtb_file.path.display(),
                    dtb_path.display()
                ),
            ))?;

            if !check_digest(&dtb_path, &dtb_file.hash_info)? {
                return Err(MigError::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to check digest on copied dtb file '{}' to {:?}",
                        dtb_path.display(),
                        dtb_file.hash_info
                    ),
                ));
            }

            info!(
                "dtb file: '{}' -> '{}'",
                dtb_file.path.display(),
                dtb_path.display()
            );
            dtb_path
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("The device tree blob (dtb_file) could not be found"),
            ));
        };

        let uenv_path = path_append(&bootmgr_path.path, UENV_FILE_NAME);

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

        // convert kernel / initrd / dtb paths to mountpoint relative paths for uEnv.txt
        let (kernel_path, initrd_path, dtb_path) =
            if boot_path.device_info.mountpoint != PathBuf::from(ROOT_PATH) {
                (
                    path_append(
                        ROOT_PATH,
                        kernel_path
                            .strip_prefix(&boot_path.device_info.mountpoint)
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "cannot remove prefix '{}' from '{}'",
                                    kernel_path.display(),
                                    boot_path.device_info.mountpoint.display()
                                ),
                            ))?,
                    ),
                    path_append(
                        ROOT_PATH,
                        initrd_path
                            .strip_prefix(&boot_path.device_info.mountpoint)
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "cannot remove prefix '{}' from '{}'",
                                    initrd_path.display(),
                                    boot_path.device_info.mountpoint.display()
                                ),
                            ))?,
                    ),
                    path_append(
                        ROOT_PATH,
                        dtb_path
                            .strip_prefix(&boot_path.device_info.mountpoint)
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "cannot remove prefix '{}' from '{}'",
                                    dtb_path.display(),
                                    boot_path.device_info.mountpoint.display()
                                ),
                            ))?,
                    ),
                )
            } else {
                (kernel_path, initrd_path, dtb_path)
            };

        let mut uenv_text = String::from(BALENA_FILE_TAG);
        uenv_text.push_str(UENV_TXT);
        uenv_text = uenv_text.replace("__KERNEL_PATH__", &kernel_path.to_string_lossy());
        uenv_text = uenv_text.replace("__INITRD_PATH__", &initrd_path.to_string_lossy());
        uenv_text = uenv_text.replace("__DTB_PATH__", &dtb_path.to_string_lossy());
        uenv_text = uenv_text.replace("__DRIVE__", &drive_num.0);
        uenv_text = uenv_text.replace("__PARTITION__", &drive_num.1);
        uenv_text = uenv_text.replace("__ROOT_DEV__", &bootmgr_path.device_info.get_kernel_cmd());
        uenv_text = uenv_text.replace("__ROOT_FSTYPE__", &bootmgr_path.device_info.fs_type);
        uenv_text = uenv_text.replace("__MISC_OPTS__", kernel_opts);

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

    fn restore(&self, mounts: &Mounts, config: &Stage2Config) -> bool {
        info!("restoring boot configuration",);

        // TODO: restore on bootmgr device
        let mut res = true;

        let uenv_file = path_append(mounts.get_boot_mountpoint(), UENV_FILE_NAME);

        let balena_file = match is_balena_file(&uenv_file) {
            Ok(res) => res,
            Err(why) => {
                warn!(
                    "Failed to get file status for '{}', error: {:?}",
                    uenv_file.display(),
                    why
                );
                false
            }
        };

        if file_exists(&uenv_file) && balena_file {
            if let Err(why) = remove_file(&uenv_file) {
                error!(
                    "failed to remove migrate boot config file '{}' error: {:?}",
                    uenv_file.display(),
                    why
                )
            } else {
                info!("Removed balena boot config file '{}'", &uenv_file.display());
            }
        } else {
            warn!(
                "balena boot config file not found in '{}'",
                &uenv_file.display()
            );
            res = false;
        }

        if !restore_backups(mounts.get_boot_mountpoint(), config.get_boot_backups()) {
            res = false;
        }

        // TODO: remove kernel & initramfs, dtb  too
        res
    }
}
