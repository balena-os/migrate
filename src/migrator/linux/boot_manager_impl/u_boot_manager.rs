use chrono::Local;
use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use nix::mount::{mount, umount, MsFlags};
use regex::Regex;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::common::file_digest::check_digest;
use crate::linux::lsblk_info::LsblkInfo;
use crate::{
    common::{
        boot_manager::BootManager,
        call, file_exists, is_balena_file,
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
const UBOOT_DRIVE_REGEX: &str = r#"^/dev/mmcblk\d+p(\d+)$"#;

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
    // location of MLO / u-boot.img files, this is where we would like to put our uEnv.txt and
    // our kernel / initrd / dtb if sufficient space is available
    bootmgr_path: Option<PathInfo>,
    // this is where we are putting our uEnv.txt and our kernel / initrd / dtb if sufficient space
    // is NOT available above or UBOOT files where not found
    bootmgr_alt_path: Option<PathInfo>,
    // mmc device selector , typically 0 for SD card, 1 for emmc, set by device
    mmc_index: u8,
}

impl UBootManager {
    pub fn new(mmc_index: u8) -> UBootManager {
        UBootManager {
            bootmgr_path: None,
            bootmgr_alt_path: None,
            mmc_index,
        }
    }

    // find the UBOOT files on the given path or in a boot subdirectory
    fn find_uboot_files<P: AsRef<Path>>(base_path: P) -> Option<PathBuf> {
        const UBOOT_FILES: [&str; 3] = [MLO_FILE_NAME, UBOOT_FILE_NAME, UENV_FILE_NAME];
        let mut path_found: Option<PathBuf> = None;
        if let Some(_) = UBOOT_FILES.iter().find(|file| {
            let search_path = path_append(&base_path, BOOT_PATH);
            if file_exists(path_append(&search_path, file)) {
                path_found = Some(search_path);
                true
            } else {
                // TODO: not sure about uEnv.txt in /

                if file_exists(path_append(&base_path, file)) {
                    path_found = Some(PathBuf::from(base_path.as_ref()));
                    true
                } else {
                    false
                }
            }
        }) {
            path_found
        } else {
            None
        }
    }

    // Try to find a drive containing MLO, uEnv.txt or u-boot.bin, mount it if necessary
    // and return PathInfo if found

    // Find boot manager partition - the partition where we will place our uEnv.txt
    // In U-boot boot manager drive will contain  MLO & u-boot.img and possibly uEnv.txt in the root.
    // That said MLO & u-boot.img might reside in a special partition or in the MBR and uEnv.txt is
    // not mandatory. So neither of them ight be found.
    // So current strategy is:
    //    a) Look for MLO & u-boot.img on all relevant drives. Return that drive if found
    //       Look only on likely device types:
    //        /dev/mmcblk[0-9]+p[0-9]+ (avoiding mmcblk[0-9]+boot0 mmcblk[0-9]+boot1 mmcblk[0-9]+rpmb)
    //        /dev/sd[a-z,A-Z][0-9]+
    //        mmcblk[0-9]+boot0 mmcblk[0-9]+boot1 mmcblk[0-9]+rpmb indicates bootmanager devices on
    //        beaglebones.
    //    b) Use the drive with the /root partition

    fn find_bootmgr_path(
        mig_info: &MigrateInfo,
        lsblk_info: &LsblkInfo,
    ) -> Result<Option<PathInfo>, MigError> {
        lazy_static! {
            // same as ab
            static ref BOOT_DRIVE_RE: Regex = Regex::new(UBOOT_DRIVE_FILTER_REGEX).unwrap();
        }

        if let Some(bootmgr_path) = UBootManager::find_uboot_files(ROOT_PATH) {
            return Ok(Some(
                PathInfo::from_path(bootmgr_path, lsblk_info)?.unwrap(),
            ));
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
                            if tmp_mountpoint.is_none() {
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

                    if let Some(found_path) = UBootManager::find_uboot_files(mountpoint) {
                        // leave mounted
                        return Ok(Some(PathInfo::from_mounted(
                            found_path, mountpoint, blk_device, &partition,
                        )?));
                    } else if mounted {
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

        warn!("No u-boot boot files found",);
        Ok(None)
    }

    // check the potential bootmgr path for space
    fn check_bootmgr_path(
        bootmgr_path: &PathInfo,
        mig_info: &MigrateInfo,
    ) -> Result<bool, MigError> {
        debug!(
            "check_bootmgr_path: called with path: {}",
            bootmgr_path.path.display()
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
            return Err(MigError::displayed());
        }

        debug!(
            "check_bootmgr_path: required: {}, available: {}",
            boot_req_space, bootmgr_path.fs_free
        );
        Ok(boot_req_space < bootmgr_path.fs_free)
    }
}

impl<'a> BootManager for UBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::UBoot
    }

    fn get_bootmgr_path(&self) -> PathInfo {
        self.bootmgr_path.as_ref().unwrap().clone()
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
        if let Some(path) = UBootManager::find_bootmgr_path(mig_info, &lsblk_info)? {
            info!(
                "Found uboot boot manager files in '{}', device: '{}', mountpoint: '{}', fs type: {}",
                path.path.display(),
                path.device_info.device.display(),
                path.mountpoint.display(),
                path.device_info.fs_type,
            );

            if UBootManager::check_bootmgr_path(&path, mig_info)? {
                info!(
                    "Using boot manager path '{}', device: '{}', mountpoint: '{}', fs type: {}",
                    path.path.display(),
                    path.device_info.device.display(),
                    path.mountpoint.display(),
                    path.device_info.fs_type,
                );

                self.bootmgr_path = Some(path);
                return Ok(true);
            } else {
                warn!(
                    "Can't_migrate with boot manager path {} : checking for space elsewhere",
                    path.path.display()
                );
                // save this anyway, gotta figure out in setup
                self.bootmgr_path = Some(path);
            }
        }

        // no uboot files found or not enough space there, try (again) in / or /boot
        if let Some(path) = PathInfo::from_path(BOOT_PATH, &lsblk_info)? {
            if UBootManager::check_bootmgr_path(&path, mig_info)? {
                info!(
                    "Using boot manager path '{}', device: '{}', mountpoint: '{}', fs type: {}",
                    path.path.display(),
                    path.device_info.device.display(),
                    path.mountpoint.display(),
                    path.device_info.fs_type,
                );

                self.bootmgr_alt_path = Some(path);
                return Ok(true);
            }

            warn!(
                "Can't_migrate with boot manager path {} : checking space elsewhere",
                path.path.display()
            );
        }

        if let Some(path) = PathInfo::from_path(ROOT_PATH, &lsblk_info)? {
            if UBootManager::check_bootmgr_path(&path, mig_info)? {
                info!(
                    "Using boot manager path '{}', device: '{}, mountpoint: '{}', fs type: {}",
                    path.path.display(),
                    path.device_info.device.display(),
                    path.mountpoint.display(),
                    path.device_info.fs_type,
                );

                self.bootmgr_alt_path = Some(path);
                return Ok(true);
            }
        }

        error!("Could not find a directory with sufficient space to store the migrate kernel, initramfs and dtb file.");
        Ok(false)
    }

    fn setup(
        &self,
        mig_info: &MigrateInfo,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        if self.bootmgr_path.is_none() && self.bootmgr_alt_path.is_none() {
            error!("setup: no boot manager path was set.");
            return Err(MigError::displayed());
        }

        // this is where the UBOOT files have been found, the preferred location for uEnv.txt
        let bootmgr_path = if let Some(ref bootmgr_path) = self.bootmgr_path {
            bootmgr_path
        } else {
            self.bootmgr_alt_path.as_ref().unwrap()
        };

        // this is where we put the kernel and initramfs. Might differ from bootmgr_path if there
        // was not enough disk space in that location. uEnv.txt will point to this location
        // if no UBOOT files were found uEnv.txt goes here too
        let boot_path = if let Some(ref boot_path) = self.bootmgr_alt_path {
            // TODO: save to s2_cfg - only really needed if we want to delete it
            boot_path
        } else {
            bootmgr_path
        };

        // TODO: allow devices other than mmcblk
        // **********************************************************************
        // read drive number & partition number from kernel / initramfs location
        // in uboot drive numbers do not appear to be consistent with what we see here
        // sd-card appears to be always mmc 0 and internal emmc appears to always be mmc 1
        // so depending on if we want to migrate an SD card or the emmc we need to select 0 or1
        // TODO: distinguish what we are booting from SD-card or emmc
        // current workaround:
        // a) make it configurable, else
        // b) let device decide -
        //     - beagleboardXM - no emmc use mmc 0
        //     - beaglebone-green - has emc - use mmc 1 by default

        let part_num = {
            let dev_name = &boot_path.device_info.device;

            if let Some(captures) = Regex::new(UBOOT_DRIVE_REGEX)
                .unwrap()
                .captures(&dev_name.to_string_lossy())
            {
                String::from(captures.get(1).unwrap().as_str())
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "failed to parse partition numbers from boot device name '{}'",
                        dev_name.display()
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
                &"The device tree blob (dtb_file) could not be found".to_string(),
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
        let mut paths: Vec<PathBuf> = Vec::new();
        if ![kernel_path, initrd_path, dtb_path].iter().all(|path| {
            if boot_path.mountpoint == PathBuf::from(ROOT_PATH) {
                paths.push(path.clone());
                true
            } else {
                match path.strip_prefix(&boot_path.mountpoint) {
                    Ok(path) => {
                        paths.push(path_append(ROOT_PATH, path));
                        true
                    }
                    Err(why) => {
                        error!(
                            "cannot remove prefix '{}' from '{}', error: {:?}",
                            path.display(),
                            boot_path.mountpoint.display(),
                            why
                        );
                        false
                    }
                }
            }
        }) {
            return Err(MigError::displayed());
        }

        let mut uenv_text = String::from(BALENA_FILE_TAG);
        uenv_text.push_str(UENV_TXT);
        uenv_text = uenv_text.replace("__DTB_PATH__", &paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace("__INITRD_PATH__", &paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace("__KERNEL_PATH__", &paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace("__DRIVE__", &self.mmc_index.to_string());
        uenv_text = uenv_text.replace("__PARTITION__", &part_num);
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
