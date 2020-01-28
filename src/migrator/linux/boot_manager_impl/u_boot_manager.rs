use chrono::Local;
use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use nix::mount::{mount, umount, MsFlags};
use regex::Regex;
use std::fs::{create_dir_all, remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::common::dir_exists;
use crate::common::file_digest::check_digest;
use crate::linux::lsblk_info::LsblkInfo;
use crate::{
    common::{
        boot_manager::BootManager,
        call,
        config::migrate_config::UEnvStrategy,
        file_exists, is_balena_file,
        migrate_info::MigrateInfo,
        path_append,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, FileInfo, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG, MIG_INITRD_NAME, MIG_KERNEL_NAME},
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
#[derive(Debug, Clone)]
enum BootFileType {
    KernelFile,
    Initramfs,
    DtbFile,
    UEnvFile,
}

// *************************************************************************************************
// Use this config instead of setting up kernel & initramfs manually
// copied from a standard uEnv uses uname_r (replace __BALENA_KERNEL_UNAME_R__) to determine what
// kernel, initramfs and dtb to boot.
// migrate kernel, initramfs and dtb's have to be saved under the corresponding names
// - vmlinuz-<uname_r>
// - initrd.img-<uname_r>
// - config-<uname_r> kernel config parameters
// - dtbs/<uname_r>/*.dtb
// __ROOT_DEV_UUID__ needs to be replaced with the root partition UUID
// __KERNEL_CMDLINE__ needs to be replaced with additional kernel cmdline parameters

const UENV_TXT1: &str = r###"
#Docs: http://elinux.org/Beagleboard:U-boot_partitioning_layout_2.0

# uname_r=3.8.13-bone71.1
uname_r=__BALENA_KERNEL_UNAME_R__
#dtb=
# add console=/dev/ttyS0 ?
cmdline=init=/lib/systemd/systemd __KERNEL_CMDLINE__

##Example
#cape_disable=capemgr.disable_partno=
#cape_enable=capemgr.enable_partno=
cape_enable=capemgr.enable_partno=BB-UART2

##Disable HDMI/eMMC
#cape_disable=capemgr.disable_partno=BB-BONELT-HDMI,BB-BONELT-HDMIN,BB-BONE-EMMC-2G

##Disable HDMI
#cape_disable=capemgr.disable_partno=BB-BONELT-HDMI,BB-BONELT-HDMIN

##Disable eMMC
#cape_disable=capemgr.disable_partno=BB-BONE-EMMC-2G

##Audio Cape (needs HDMI Audio disabled)
#cape_disable=capemgr.disable_partno=BB-BONELT-HDMI
#cape_enable=capemgr.enable_partno=BB-BONE-AUDI-02


##enable BBB: eMMC Flasher:
# cmdline=init=/opt/scripts/tools/eMMC/init-eMMC-flasher-v3.sh

__ROOT_DEV_ID__
"###;

// *************************************************************************************************
// uEnv.txt manually configuring the migrate kernel, initramfs & dtbs.
// failed to boot on a new beaglebone-green.
// setup of uboot env does not seem to support the ENV 'uenvcmd' so kernel is not started
// on that device.

const UENV_TXT2: &str = r###"
// TODO: use individual, correctly named files rather than one dedicated one
pub const MIG_DTB_NAME: &str = "balena-migrate.dtb";

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

// TODO: support multiple DTB files for different versions, copy several or just matching

pub(crate) struct UBootManager {
    // location of MLO / u-boot.img files, this is where we would like to put our uEnv.txt and
    // our kernel / initrd / dtb if sufficient space is available
    bootmgr_path: Option<PathInfo>,
    // this is where we are putting our uEnv.txt and our kernel / initrd / dtb if sufficient space
    // is NOT available above or UBOOT files where not found
    bootmgr_alt_path: Option<PathInfo>,
    // mmc device selector , typically 0 for SD card, 1 for emmc, set by device
    strategy: UEnvStrategy,
    mmc_index: u8,
    // uboot wants this in dbt-name
    dtb_name: String,
    // cached paths
    kernel_dest: Option<PathBuf>,
    initrd_dest: Option<PathBuf>,
    dtb_dest: Option<PathBuf>,
    uenv_dest: Option<PathBuf>,
}

impl UBootManager {
    pub fn for_stage2() -> UBootManager {
        UBootManager {
            bootmgr_path: None,
            bootmgr_alt_path: None,
            mmc_index: 1,
            strategy: UEnvStrategy::Manual,
            dtb_name: String::new(),
            kernel_dest: None,
            initrd_dest: None,
            dtb_dest: None,
            uenv_dest: None,
        }
    }

    pub fn new(mmc_index: u8, strategy: UEnvStrategy, dtb_name: String) -> UBootManager {
        UBootManager {
            bootmgr_path: None,
            bootmgr_alt_path: None,
            mmc_index,
            strategy,
            dtb_name,
            kernel_dest: None,
            initrd_dest: None,
            dtb_dest: None,
            uenv_dest: None,
        }
    }

    // find the UBOOT files on the given path or in a boot subdirectory
    fn find_uboot_files<P: AsRef<Path>>(base_path: P) -> Option<PathBuf> {
        const UBOOT_FILES: [&str; 3] = [MLO_FILE_NAME, UBOOT_FILE_NAME, UENV_FILE_NAME];
        let mut path_found: Option<PathBuf> = None;
        let _res = UBOOT_FILES.iter().find(|file| {
            let search_path = path_append(&base_path, BOOT_PATH);
            if file_exists(path_append(&search_path, file)) {
                path_found = Some(search_path);
                true
            } else {
                // TODO: not sure about uEnv.txt in root
                if file_exists(path_append(&base_path, file)) {
                    path_found = Some(PathBuf::from(base_path.as_ref()));
                    true
                } else {
                    false
                }
            }
        });

        path_found
    }

    // Try to find a drive containing MLO, uEnv.txt or u-boot.bin, mount it if necessary
    // and return PathInfo if found

    // Find boot manager partition - the partition where we will place our uEnv.txt
    // In U-boot boot manager drive will contain  MLO & u-boot.img and possibly uEnv.txt in the root.
    // That said MLO & u-boot.img might reside in a special partition or in the MBR and uEnv.txt is
    // not mandatory. So neither of them might be found.
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
        // this looks into /boot and /
        if let Some(bootmgr_path) = UBootManager::find_uboot_files(ROOT_PATH) {
            return Ok(Some(PathInfo::from_path(bootmgr_path)?));
        }

        lazy_static! {
            // same as ab
            static ref BOOT_DRIVE_RE: Regex = Regex::new(UBOOT_DRIVE_FILTER_REGEX).unwrap();
        }
        // now temp-mount and check other partitions
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

    fn get_target_file_name<P: AsRef<Path>>(
        &mut self,
        file_type: &BootFileType,
        boot_path: Option<&Path>,
        file: P,
    ) -> Result<&Path, MigError> {
        // TODO: cache results in object ?
        // TODO: switch BootFileType / Strategy inside out

        debug!(
            "get_target_file_name: file_type: {:?}, boot_path: {:?}, file: {:?}",
            file_type,
            boot_path,
            file.as_ref()
        );

        let base_path = if let Some(boot_path) = boot_path {
            boot_path
        } else {
            match file_type {
                BootFileType::UEnvFile => &self.bootmgr_path.as_ref().unwrap().path,
                _ => &self.bootmgr_alt_path.as_ref().unwrap().path,
            }
        };

        match &self.strategy {
            UEnvStrategy::UName(uname) => match file_type {
                BootFileType::KernelFile => {
                    if let Some(ref dest) = self.kernel_dest {
                        Ok(dest)
                    } else {
                        self.kernel_dest =
                            Some(path_append(base_path, &format!("vmlinuz-{}", uname)));
                        Ok(self.kernel_dest.as_ref().unwrap())
                    }
                }
                BootFileType::Initramfs => {
                    if let Some(ref dest) = self.initrd_dest {
                        Ok(dest)
                    } else {
                        self.initrd_dest =
                            Some(path_append(base_path, &format!("initrd.img-{}", uname)));
                        Ok(self.initrd_dest.as_ref().unwrap())
                    }
                }
                BootFileType::DtbFile => {
                    if let Some(ref dest) = self.dtb_dest {
                        Ok(dest)
                    } else {
                        self.dtb_dest = Some(path_append(
                            path_append(base_path, &format!("dtbs/{}/", uname)),
                            &self.dtb_name,
                        ));
                        Ok(self.dtb_dest.as_ref().unwrap())
                    }
                }
                BootFileType::UEnvFile => {
                    if let Some(ref dest) = self.uenv_dest {
                        Ok(dest)
                    } else {
                        self.uenv_dest = Some(path_append(base_path, &file));
                        Ok(&self.uenv_dest.as_ref().unwrap())
                    }
                }
            },
            UEnvStrategy::Manual => match file_type {
                BootFileType::KernelFile => {
                    if let Some(ref dest) = self.kernel_dest {
                        Ok(dest)
                    } else {
                        self.kernel_dest = Some(path_append(base_path, MIG_KERNEL_NAME));
                        Ok(self.kernel_dest.as_ref().unwrap())
                    }
                }
                BootFileType::Initramfs => {
                    if let Some(ref dest) = self.dtb_dest {
                        Ok(dest)
                    } else {
                        self.initrd_dest = Some(path_append(base_path, MIG_INITRD_NAME));
                        Ok(self.initrd_dest.as_ref().unwrap())
                    }
                }
                BootFileType::DtbFile => {
                    if let Some(ref dest) = self.dtb_dest {
                        Ok(dest)
                    } else {
                        self.dtb_dest = Some(path_append(base_path, file));
                        Ok(self.dtb_dest.as_ref().unwrap())
                    }
                }
                BootFileType::UEnvFile => {
                    if let Some(ref dest) = self.uenv_dest {
                        Ok(dest)
                    } else {
                        self.uenv_dest = Some(path_append(base_path, UENV_FILE_NAME));
                        Ok(self.uenv_dest.as_ref().unwrap())
                    }
                }
            },
        }
    }

    fn copy_and_check<P: AsRef<Path>>(source: &FileInfo, dest: P) -> Result<(), MigError> {
        let dest = dest.as_ref();
        std::fs::copy(&source.path, dest).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy kernel file '{}' to '{}'",
                source.path.display(),
                dest.display()
            ),
        ))?;

        if !check_digest(dest, &source.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied kernel file '{}' to {:?}",
                    dest.display(),
                    source.hash_info
                ),
            ));
        }

        Ok(())
    }

    // check the potential bootmanager path for space
    fn check_bootmgr_path(
        &mut self,
        bootmgr_path: &PathInfo,
        mig_info: &MigrateInfo,
    ) -> Result<bool, MigError> {
        debug!(
            "check_bootmgr_path: called with path: {}",
            bootmgr_path.path.display()
        );
        let mut boot_req_space: u64 = 8 * 1024; // one 8KiB extra space just in case and for uEnv.txt)
        boot_req_space += if !file_exists(self.get_target_file_name(
            &BootFileType::KernelFile,
            Some(&bootmgr_path.path),
            MIG_KERNEL_NAME,
        )?) {
            mig_info.kernel_file.size
        } else {
            0
        };

        boot_req_space += if !file_exists(self.get_target_file_name(
            &BootFileType::Initramfs,
            Some(&bootmgr_path.path),
            MIG_INITRD_NAME,
        )?) {
            mig_info.initrd_file.size
        } else {
            0
        };

        // TODO: support multiple dtb files ?
        if let Some(dtb_file) = mig_info.dtb_file.get(0) {
            boot_req_space += if !file_exists(self.get_target_file_name(
                &BootFileType::DtbFile,
                Some(&bootmgr_path.path),
                self.dtb_name.clone(),
            )?) {
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
            boot_req_space, bootmgr_path.device_info.fs_free
        );
        Ok(boot_req_space < bootmgr_path.device_info.fs_free)
    }

    // uname setup strategy for fn setup
    fn strategy_uname(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
        uname: &str,
    ) -> Result<(), MigError> {
        // **********************************************************************
        // copy new kernel & iniramfs
        // save under the corresponding names
        // - vmlinuz-<uname_r>
        // - initrd.img-<uname_r>
        // - config-<uname_r> kernel config parameters
        // - dtbs/<uname_r>/*.dtb
        // __ROOT_DEV_UUID__ needs to be replaced with the root partition UUID
        // __KERNEL_CMDLINE__ needs to be replaced with additional kernel cmdline parameters

        let uenv_path = if let Some(ref bootmgr_path) = self.bootmgr_path {
            bootmgr_path.clone()
        } else {
            self.bootmgr_alt_path.as_ref().unwrap().clone()
        };

        let kernel_dest =
            self.get_target_file_name(&BootFileType::KernelFile, None, MIG_KERNEL_NAME)?;
        UBootManager::copy_and_check(&mig_info.kernel_file, &kernel_dest)?;

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            kernel_dest.display()
        );

        call(CHMOD_CMD, &["+x", &kernel_dest.to_string_lossy()], false)?;

        let initrd_dest =
            self.get_target_file_name(&BootFileType::Initramfs, None, MIG_INITRD_NAME)?;
        UBootManager::copy_and_check(&mig_info.initrd_file, &initrd_dest)?;

        info!(
            "initramfs file: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            initrd_dest.display()
        );

        if let Some(dtb_src) = &mig_info.dtb_file.get(0) {
            let dtb_dest =
                self.get_target_file_name(&BootFileType::DtbFile, None, self.dtb_name.clone())?;
            let dtb_dir = if let Some(parent) = dtb_dest.parent() {
                parent
            } else {
                error!(
                    "Unable to get parent of target dtb file: '{}",
                    dtb_dest.display()
                );
                return Err(MigError::displayed());
            };

            if !dir_exists(&dtb_dir)? {
                create_dir_all(&dtb_dir).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to create dtb directory: '{}", dtb_dir.display()),
                ))?;
            }

            UBootManager::copy_and_check(dtb_src, &dtb_dest)?;

            info!(
                "dtb file: '{}' -> '{}'",
                dtb_src.path.display(),
                dtb_dest.display()
            );
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &"The device tree blob (dtb_file) could not be found".to_string(),
            ));
        };

        let uenv_file_path =
            self.get_target_file_name(&BootFileType::UEnvFile, None, UENV_FILE_NAME)?;

        if file_exists(&uenv_file_path) {
            // **********************************************************************
            // ** backup /uEnv.txt if exists
            if !is_balena_file(&uenv_file_path)? {
                let backup_uenv = format!(
                    "{}-{}",
                    &uenv_file_path.to_string_lossy(),
                    Local::now().format("%s")
                );

                std::fs::copy(&uenv_file_path, &backup_uenv).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "failed to file '{}' to '{}'",
                        uenv_file_path.display(),
                        &backup_uenv
                    ),
                ))?;
                info!(
                    "copied backup of '{}' to '{}'",
                    uenv_file_path.display(),
                    &backup_uenv
                );

                let mut boot_cfg_bckup: Vec<(String, String)> = Vec::new();
                boot_cfg_bckup.push((
                    String::from(&*uenv_file_path.to_string_lossy()),
                    backup_uenv,
                ));

                s2_cfg.set_boot_bckup(boot_cfg_bckup);
            }
        }

        // **********************************************************************
        // ** create new /uEnv.txt
        // convert kernel / initrd / dtb paths to mountpoint relative paths for uEnv.txt

        let mut uenv_text = String::from(BALENA_FILE_TAG);
        uenv_text.push_str(UENV_TXT1);
        uenv_text = uenv_text.replace("__BALENA_KERNEL_UNAME_R__", uname);
        // TODO: create from kernel path in kernel_dest

        uenv_text = uenv_text.replace(
            "__ROOT_DEV_ID__",
            &uenv_path.device_info.get_uboot_kernel_cmd(),
        );
        uenv_text = uenv_text.replace("__KERNEL_CMDLINE__", kernel_opts);

        debug!("writing uEnv.txt as:\n {}", uenv_text);

        let mut uenv_file = File::create(&uenv_file_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to create new '{}'", uenv_file_path.display()),
        ))?;
        uenv_file
            .write_all(uenv_text.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to write new '{}'", uenv_file_path.display()),
            ))?;
        info!("created new file in '{}'", uenv_file_path.display());
        Ok(())
    }

    // manual setup strategy for fn setup
    fn strategy_manual(
        &mut self,
        mig_info: &MigrateInfo,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
        part_num: &str,
    ) -> Result<(), MigError> {
        // **********************************************************************
        // ** copy new kernel & iniramfs
        let kernel_dest = self
            .get_target_file_name(&BootFileType::KernelFile, None, MIG_KERNEL_NAME)?
            .to_path_buf();
        UBootManager::copy_and_check(&mig_info.kernel_file, &kernel_dest)?;

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            kernel_dest.display()
        );

        call(CHMOD_CMD, &["+x", &kernel_dest.to_string_lossy()], false)?;

        let initrd_dest = self
            .get_target_file_name(&BootFileType::Initramfs, None, MIG_INITRD_NAME)?
            .to_path_buf();
        UBootManager::copy_and_check(&mig_info.initrd_file, &initrd_dest)?;

        info!(
            "initramfs file: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            initrd_dest.display()
        );

        let dtb_dest = if let Some(dtb_file) = &mig_info.dtb_file.get(0) {
            let dtb_dest = self
                .get_target_file_name(&BootFileType::DtbFile, None, self.dtb_name.clone())?
                .to_path_buf();
            UBootManager::copy_and_check(&dtb_file, &dtb_dest)?;

            info!(
                "dtb file: '{}' -> '{}'",
                dtb_file.path.display(),
                dtb_dest.display()
            );
            dtb_dest
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &"The device tree blob (dtb_file) was not defined",
            ));
        };

        let uenv_dest = self
            .get_target_file_name(&BootFileType::UEnvFile, None, UENV_FILE_NAME)?
            .to_path_buf();

        // TODO: make sure we do not copy files already modified by us
        if file_exists(&uenv_dest) {
            // **********************************************************************
            // ** backup /uEnv.txt if exists
            if !is_balena_file(&uenv_dest)? {
                let backup_uenv = format!(
                    "{}-{}",
                    &uenv_dest.to_string_lossy(),
                    Local::now().format("%s")
                );
                std::fs::copy(&uenv_dest, &backup_uenv).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "failed to file '{}' to '{}'",
                        uenv_dest.display(),
                        &backup_uenv
                    ),
                ))?;
                info!(
                    "copied backup of '{}' to '{}'",
                    uenv_dest.display(),
                    &backup_uenv
                );

                let mut boot_cfg_bckup: Vec<(String, String)> = Vec::new();
                boot_cfg_bckup.push((String::from(&*uenv_dest.to_string_lossy()), backup_uenv));

                s2_cfg.set_boot_bckup(boot_cfg_bckup);
            }
        }

        // **********************************************************************
        // ** create new /uEnv.txt
        // convert kernel / initrd / dtb paths to mountpoint relative paths for uEnv.txt
        let mut paths: Vec<PathBuf> = Vec::new();
        let result = [kernel_dest, initrd_dest, dtb_dest].iter().all(|path| {
            let mut done = false;
            if let Some(ref boot_path) = self.bootmgr_path {
                if (boot_path.device_info.mountpoint != PathBuf::from(ROOT_PATH))
                    && path.starts_with(&boot_path.device_info.mountpoint)
                {
                    match path.strip_prefix(&boot_path.device_info.mountpoint) {
                        Ok(path) => {
                            paths.push(path.to_path_buf());
                            done = true
                        }
                        Err(why) => error!(
                            "cannot remove prefix '{}' from '{}', error: {:?}",
                            path.display(),
                            boot_path.device_info.mountpoint.display(),
                            why
                        ),
                    }
                } else {
                    paths.push(path.clone());
                    done = true;
                }
            } else if let Some(ref boot_path) = self.bootmgr_alt_path {
                if (boot_path.device_info.mountpoint != PathBuf::from(ROOT_PATH))
                    && path.starts_with(&boot_path.device_info.mountpoint)
                {
                    match path.strip_prefix(&boot_path.device_info.mountpoint) {
                        Ok(path) => {
                            paths.push(path_append(ROOT_PATH, path));
                            done = true
                        }
                        Err(why) => error!(
                            "cannot remove prefix '{}' from '{}', error: {:?}",
                            path.display(),
                            boot_path.device_info.mountpoint.display(),
                            why
                        ),
                    }
                } else {
                    paths.push(path.clone());
                    done = true;
                }
            }

            if !done {
                error!(
                    "failed to strip mountpoint from path for {}",
                    path.display()
                )
            }
            done
        });

        if !result {
            // make relative from abs paths failed for some file
            return Err(MigError::displayed());
        }

        debug!("converted paths for uEnv.txt: {}", paths.len());

        let mut uenv_text = String::from(BALENA_FILE_TAG);
        uenv_text.push_str(UENV_TXT2);
        uenv_text = uenv_text.replace("__DTB_PATH__", &paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace("__INITRD_PATH__", &paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace("__KERNEL_PATH__", &paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace("__DRIVE__", &self.mmc_index.to_string());
        uenv_text = uenv_text.replace("__PARTITION__", &part_num);
        let boot_path = self.get_bootmgr_path();
        uenv_text = uenv_text.replace("__ROOT_DEV__", &boot_path.device_info.get_kernel_cmd());
        uenv_text = uenv_text.replace("__ROOT_FSTYPE__", &boot_path.device_info.fs_type);
        uenv_text = uenv_text.replace("__MISC_OPTS__", kernel_opts);

        debug!("writing uEnv.txt as:\n {}", uenv_text);

        let mut uenv_file = File::create(&uenv_dest).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to create new '{}'", uenv_dest.display()),
        ))?;
        uenv_file
            .write_all(uenv_text.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to write new '{}'", uenv_dest.display()),
            ))?;
        info!("created new file in '{}'", uenv_dest.display());
        Ok(())
    }
}

impl BootManager for UBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::UBoot
    }

    fn get_bootmgr_path(&self) -> PathInfo {
        if let Some(ref boot_path) = self.bootmgr_path {
            boot_path.clone()
        } else if let Some(ref boot_path) = self.bootmgr_alt_path {
            boot_path.clone()
        } else {
            panic!("Failed to retrieve a boot manager path");
        }
    }

    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr
        debug!("can_migrate: entered");

        // find the u-boot boot device
        // this is where uEnv.txt has to go

        let lsblk_info = LsblkInfo::new()?;
        if let Some(path) = UBootManager::find_bootmgr_path(mig_info, &lsblk_info)? {
            info!(
                "Found uboot boot manager files in '{}', device: '{}', mountpoint: '{}', fs type: {}",
                path.path.display(),
                path.device_info.device,
                path.device_info.mountpoint.display(),
                path.device_info.fs_type,
            );

            if self.check_bootmgr_path(&path, mig_info)? {
                info!(
                    "Using boot manager path '{}', device: '{}', mountpoint: '{}', fs type: {}",
                    path.path.display(),
                    path.device_info.device,
                    path.device_info.mountpoint.display(),
                    path.device_info.fs_type,
                );

                self.bootmgr_path = Some(path.clone());
                self.bootmgr_alt_path = Some(path);
                return Ok(true);
            } else {
                // Not enough space for kernel / initamfs etc where boot files where found
                match &self.strategy {
                    UEnvStrategy::UName(_uname) => {
                        // Uname strategy need kerne etc right there can't do this
                        error!(
                            "Can't_migrate with boot manager path {}",
                            path.path.display()
                        );
                        // save this anyway, gotta figure out in setup
                        return Ok(false);
                    }
                    UEnvStrategy::Manual => {
                        // manual strategy can work out alt dest
                        warn!(
                            "Can't_migrate with boot manager path {} : checking for space elsewhere",
                            path.path.display()
                        );
                        // save this anyway, gotta figure out in setup
                        self.bootmgr_path = Some(path);
                    }
                }
            }
        }

        // no uboot files found or not enough space there, try (again) in / or /boot
        let path = PathInfo::from_path(BOOT_PATH)?;
        if self.check_bootmgr_path(&path, mig_info)? {
            info!(
                "Using boot manager path '{}', device: '{}', mountpoint: '{}', fs type: {}",
                path.path.display(),
                path.device_info.device,
                path.device_info.mountpoint.display(),
                path.device_info.fs_type,
            );

            // if no uboot files were found - this is the path for all files
            if self.bootmgr_path.is_none() {
                self.bootmgr_path = Some(path.clone())
            }

            self.bootmgr_alt_path = Some(path);
            return Ok(true);
        }

        match &self.strategy {
            UEnvStrategy::UName(_uname) => {
                error!(
                    "Can't_migrate with boot manager path {}",
                    path.path.display()
                );
                // save this anyway, gotta figure out in setup
                return Ok(false);
            }
            UEnvStrategy::Manual => {
                warn!(
                    "Can't_migrate with boot manager path {} : checking for space elsewhere",
                    path.path.display()
                );
            }
        }

        let path = PathInfo::from_path(ROOT_PATH)?;
        if self.check_bootmgr_path(&path, mig_info)? {
            info!(
                "Using boot manager path '{}', device: '{}, mountpoint: '{}', fs type: {}",
                path.path.display(),
                path.device_info.device,
                path.device_info.mountpoint.display(),
                path.device_info.fs_type,
            );

            // if no uboot files were found - this is the path for all files
            if self.bootmgr_path.is_none() {
                self.bootmgr_path = Some(path.clone())
            }

            self.bootmgr_alt_path = Some(path);
            return Ok(true);
        }

        error!("Could not find a directory with sufficient space to store the migrate kernel, initramfs and dtb file.");
        Ok(false)
    }

    fn setup(
        &mut self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        // for sake of panic avoidance - later code functions relies on this
        if self.bootmgr_path.is_none() || self.bootmgr_alt_path.is_none() {
            error!(
                "setup: boot manager path are not set: bootmgr_path: {:?} bootmgr_alt_path: {:?}",
                self.bootmgr_path, self.bootmgr_alt_path
            );
            return Err(MigError::displayed());
        }

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
            let dev_name = &self.bootmgr_path.as_ref().unwrap().device_info.device;

            if let Some(captures) = Regex::new(UBOOT_DRIVE_REGEX)
                .unwrap()
                .captures(dev_name.as_str())
            {
                String::from(captures.get(1).unwrap().as_str())
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "failed to parse partition numbers from boot device name '{}'",
                        dev_name
                    ),
                ));
            }
        };

        match self.strategy {
            UEnvStrategy::UName(ref uname) => {
                let uname_str = uname.clone();
                self.strategy_uname(mig_info, config, s2_cfg, kernel_opts, &uname_str)
            }
            UEnvStrategy::Manual => self.strategy_manual(mig_info, s2_cfg, kernel_opts, &part_num),
        }
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
