use chrono::Local;
use failure::ResultExt;
use log::{debug, error, info, warn};
use regex::Regex;
use std::fs::{self, create_dir_all, remove_file, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::common::dir_exists;
use crate::common::file_digest::check_digest;
use crate::common::stage2_config::BackupCfg;
use crate::linux::disk_util::{Disk, PartitionIterator};
use crate::linux::lsblk_info::partition::Partition;
use crate::linux::stage2::mounts::MOUNT_DIR;
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
    defs::{BootType, BALENA_FILE_TAG},
    linux::{
        linux_common::{get_kernel_root_info, restore_backups, tmp_mount},
        linux_defs::CHMOD_CMD,
        linux_defs::{BOOT_PATH, MLO_FILE_NAME, ROOT_PATH, UBOOT_FILE_NAME, UENV_FILE_NAME},
        lsblk_info::block_device::BlockDevice,
        stage2::mounts::Mounts,
    },
};

// TODO: copy / flash, backup  & restore u-boot bootmanager files
// TODO: fail on more than one DTB file for manual config
// TODO: drop manual config mode if sure it will not be needed ?
// TODO: enable hash checking for dtb's & u-boot boot manager files or disable config for all boot files

const UBOOT_DRIVE_REGEX: &str = r#"^/dev/mmcblk\d+p(\d+)$"#;
#[derive(Debug, Clone)]
enum BootFileType {
    KernelFile,
    Initramfs,
    DtbFile,
}

const UBOOT_DEV_OFFSET: u64 = 0x60000;
const UBOOT_MAGIC_WORD: u32 = 0x2705_1956;

const BALENA_UBOOT_UNAME: &str = "balena-migrate";

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

uname_r=__BALENA_KERNEL_UNAME_R__
#dtb=
cmdline=init=/lib/systemd/systemd __KERNEL_CMDLINE__
__ROOT_DEV_ID__
"###;

// *************************************************************************************************
// uEnv.txt manually configuring the migrate kernel, initramfs & dtbs.
// failed to boot on a new beaglebone-green.
// setup of uboot env does not seem to support the ENV 'uenvcmd' so kernel is not started
// on that device.

const UENV_TXT2: &str = r###"
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

#[derive(Debug)]
struct UBootInfo {
    // which device to flash
    flash_device: BlockDevice,
    // MLO is installed in MBR
    in_mbr: bool,
    // where to install uboot boot manager (if not in MBR)
    mlo_path: Option<PathInfo>,
    // where to install kernel
    install_path: Option<PathInfo>,
    // paths of uEnv.txt found
    uenv_path: Vec<PathBuf>,
}

pub(crate) struct UBootManager {
    // where can_migrate has found the flash_device, uboot boot manager, uEnv.txt files etc.
    uboot_info: Option<UBootInfo>,
    strategy: UEnvStrategy,
    mmc_index: u8,
    // uboot wants this in dbt-name
    dtb_names: Vec<String>,
}

impl UBootManager {
    pub fn new(mmc_index: u8, strategy: UEnvStrategy, dtb_names: Vec<String>) -> UBootManager {
        UBootManager {
            uboot_info: None,
            mmc_index,
            strategy,
            dtb_names,
        }
    }

    pub fn for_stage2() -> UBootManager {
        UBootManager {
            uboot_info: None,
            mmc_index: 0,
            strategy: UEnvStrategy::Manual,
            dtb_names: Vec::new(),
        }
    }

    fn u32_from_big_endian(buffer: &[u8], offset: usize) -> u32 {
        let mut res: u32 = 0;
        for i in buffer.iter().skip(offset).take(4) {
            res = res * 0x100 + (buffer[*i as usize] as u32);
        }
        res
    }

    // get correct u_name style filename for boot file
    // will return target dtb directory name if n dtb filename is given
    fn get_target_file_name(
        file_type: BootFileType,
        base_path: &Path,
        file: Option<&str>,
    ) -> PathBuf {
        match file_type {
            BootFileType::KernelFile => {
                path_append(base_path, &format!("vmlinuz-{}", BALENA_UBOOT_UNAME))
            }
            BootFileType::Initramfs => {
                path_append(base_path, &format!("initrd.img-{}", BALENA_UBOOT_UNAME))
            }
            BootFileType::DtbFile => {
                let dtb_dir = path_append(base_path, &format!("dtbs/{}/", BALENA_UBOOT_UNAME));
                if let Some(file_name) = file {
                    path_append(dtb_dir, file_name)
                } else {
                    dtb_dir
                }
            }
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
    fn check_boot_req_space(
        &self,
        path: &PathInfo,
        mig_info: &MigrateInfo,
    ) -> Result<bool, MigError> {
        debug!(
            "check_bootmgr_path: called with path: {}",
            path.path.display()
        );
        let mut boot_req_space: u64 = 8 * 1024; // one 8KiB extra space just in case and for uEnv.txt)

        boot_req_space += if !file_exists(UBootManager::get_target_file_name(
            BootFileType::KernelFile,
            &path.path.as_path(),
            None,
        )) {
            mig_info.kernel_file.size
        } else {
            0
        };

        boot_req_space += if !file_exists(UBootManager::get_target_file_name(
            BootFileType::Initramfs,
            &path.path.as_path(),
            None,
        )) {
            mig_info.initrd_file.size
        } else {
            0
        };

        // TODO: support multiple dtb files ?
        for dtb_name in &self.dtb_names {
            let cfg_dtb_name = path_append(&mig_info.work_path.path, &dtb_name);
            if cfg_dtb_name.exists() {
                boot_req_space += if !file_exists(UBootManager::get_target_file_name(
                    BootFileType::DtbFile,
                    &path.path.as_path(),
                    Some(dtb_name.as_str()),
                )) {
                    fs::metadata(&cfg_dtb_name)
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "unable to retrieve size for file '{}'",
                                cfg_dtb_name.display()
                            ),
                        ))?
                        .len()
                } else {
                    0
                };
            } else {
                error!("The device tree blob file required for u-boot could not be found.");
                return Err(MigError::displayed());
            }
        }

        debug!(
            "check_bootmgr_path: required: {}, available: {}",
            boot_req_space, path.device_info.fs_free
        );
        Ok(boot_req_space < path.device_info.fs_free)
    }

    fn backup_uenv(
        uboot_info: &UBootInfo,
        backup_cfg: &mut Vec<BackupCfg>,
    ) -> Result<(), MigError> {
        // backup all found uEnv.txt files
        // TODO: this will not work for files in different drives from install_path.

        for uenv_path in &uboot_info.uenv_path {
            if !is_balena_file(&uenv_path)? {
                let backup_uenv = format!(
                    "{}-{}",
                    &uenv_path.to_string_lossy(),
                    Local::now().format("%s")
                );

                std::fs::rename(&uenv_path, &backup_uenv).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "failed to rename file '{}' to '{}'",
                        uenv_path.display(),
                        &backup_uenv
                    ),
                ))?;
                info!(
                    "renamed old uEnv.txt '{}' to '{}'",
                    uenv_path.display(),
                    backup_uenv
                );

                backup_cfg.push(BackupCfg::from_partition(
                    Partition::from_path(&uenv_path)?,
                    uenv_path.as_path(),
                    backup_uenv.as_ref(),
                ));
            } else {
                // TODO: is this really safe ?
                fs::remove_file(uenv_path).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to remove file '{}'", uenv_path.display(),),
                ))?;
                info!("Removed old balena uEnv.txt '{}'", uenv_path.display());
            }
        }
        Ok(())
    }

    // uname setup strategy for fn setup
    fn strategy_uname(
        &self,
        uboot_info: &UBootInfo,
        mig_info: &MigrateInfo,
        _config: &Config,
        kernel_opts: &str,
        boot_cfg_bckup: &mut Vec<BackupCfg>,
    ) -> Result<(), MigError> {
        // **********************************************************************
        // copy new kernel & initramfs
        // save under the corresponding names
        // - vmlinuz-<uname_r>
        // - initrd.img-<uname_r>
        // - config-<uname_r> kernel config parameters
        // - dtbs/<uname_r>/*.dtb
        // __ROOT_DEV_UUID__ needs to be replaced with the root partition UUID
        // __KERNEL_CMDLINE__ needs to be replaced with additional kernel cmdline parameters

        let install_path = if let Some(ref install_path) = uboot_info.install_path {
            install_path.clone()
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvState,
                "setup: incomplete configuration, missing install_path",
            ));
        };

        let kernel_dest =
            UBootManager::get_target_file_name(BootFileType::KernelFile, &install_path.path, None);

        UBootManager::copy_and_check(&mig_info.kernel_file, &kernel_dest)?;

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            kernel_dest.display()
        );

        call(CHMOD_CMD, &["+x", &kernel_dest.to_string_lossy()], false)?;

        let initrd_dest =
            UBootManager::get_target_file_name(BootFileType::Initramfs, &install_path.path, None);
        UBootManager::copy_and_check(&mig_info.initrd_file, &initrd_dest)?;

        info!(
            "initramfs file: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            initrd_dest.display()
        );

        let dtb_dir =
            UBootManager::get_target_file_name(BootFileType::DtbFile, &install_path.path, None);
        if !dir_exists(&dtb_dir)? {
            create_dir_all(&dtb_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to create dtb directory: '{}", dtb_dir.display()),
            ))?;
        }

        for dtb_name in &self.dtb_names {
            let dtb_src = path_append(&mig_info.work_path.path, &dtb_name);
            let dtb_dest = UBootManager::get_target_file_name(
                BootFileType::DtbFile,
                &install_path.path,
                Some(dtb_name.as_str()),
            );

            // TODO: inconsistent - no hash checking here
            fs::copy(&dtb_src, &dtb_dest).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy file '{}' to '{}'",
                    dtb_src.display(),
                    dtb_dest.display()
                ),
            ))?;

            info!(
                "dtb file: '{}' -> '{}'",
                dtb_src.display(),
                dtb_dest.display()
            );
        }

        //backup all found uEnv.txt files
        UBootManager::backup_uenv(&uboot_info, boot_cfg_bckup)?;

        let uenv_path = path_append(&install_path.path, UENV_FILE_NAME);

        // **********************************************************************
        // ** create new /uEnv.txt
        // convert kernel / initrd / dtb paths to mountpoint relative paths for uEnv.txt

        let mut uenv_text = String::from(BALENA_FILE_TAG);
        uenv_text.push_str(UENV_TXT1);
        uenv_text = uenv_text.replace("__BALENA_KERNEL_UNAME_R__", BALENA_UBOOT_UNAME);
        // TODO: create from kernel path in kernel_dest

        uenv_text = uenv_text.replace(
            "__ROOT_DEV_ID__",
            &install_path.device_info.get_uboot_kernel_cmd(),
        );
        uenv_text = uenv_text.replace("__KERNEL_CMDLINE__", kernel_opts);

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

    // manual setup strategy for fn setup
    fn strategy_manual(
        &self,
        uboot_info: &UBootInfo,
        mig_info: &MigrateInfo,
        kernel_opts: &str,
        boot_cfg_bckup: &mut Vec<BackupCfg>,
    ) -> Result<(), MigError> {
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

        let install_path = if let Some(ref install_path) = uboot_info.install_path {
            install_path.clone()
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvState,
                "setup: incomplete configuration, missing install_path",
            ));
        };

        let part_num = {
            let dev_name = &install_path.device_info.device;

            if let Some(captures) = Regex::new(UBOOT_DRIVE_REGEX).unwrap().captures(dev_name) {
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

        // **********************************************************************
        // ** copy new kernel & iniramfs

        let mut copied_files: Vec<PathBuf> = Vec::new();
        let kernel_dest =
            UBootManager::get_target_file_name(BootFileType::KernelFile, &install_path.path, None);
        UBootManager::copy_and_check(&mig_info.kernel_file, &kernel_dest)?;

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            kernel_dest.display()
        );

        call(CHMOD_CMD, &["+x", &kernel_dest.to_string_lossy()], false)?;
        copied_files.push(kernel_dest);

        let initrd_dest = UBootManager::get_target_file_name(
            BootFileType::Initramfs,
            install_path.path.as_path(),
            None,
        );
        UBootManager::copy_and_check(&mig_info.initrd_file, &initrd_dest)?;
        info!(
            "initramfs file: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            initrd_dest.display()
        );
        copied_files.push(initrd_dest);

        let dtb_tgt_dir = UBootManager::get_target_file_name(
            BootFileType::DtbFile,
            install_path.path.as_path(),
            None,
        );

        if let Some(dtb_name) = self.dtb_names.get(0) {
            let dtb_src = path_append(&mig_info.work_path.path, dtb_name.as_str());
            let dtb_dest = path_append(&dtb_tgt_dir, dtb_name.as_str());
            fs::copy(&dtb_src, &dtb_dest).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    dtb_src.display(),
                    dtb_dest.display()
                ),
            ))?;
            info!(
                "dtb file: '{}' -> '{}'",
                dtb_src.display(),
                dtb_dest.display()
            );
            copied_files.push(dtb_dest);
        }

        UBootManager::backup_uenv(&uboot_info, boot_cfg_bckup)?;

        // **********************************************************************
        // ** create new /uEnv.txt
        // convert kernel / initrd / dtb paths to mountpoint relative paths for uEnv.txt
        let uenv_dest = path_append(&install_path.path, UENV_FILE_NAME);
        let mut dev_paths: Vec<PathBuf> = Vec::new();
        let result = copied_files.iter().all(|path| {
            let mut done = false;
            if (install_path.device_info.mountpoint != PathBuf::from(ROOT_PATH))
                && path.starts_with(&install_path.device_info.mountpoint)
            {
                match path.strip_prefix(&install_path.device_info.mountpoint) {
                    Ok(path) => {
                        dev_paths.push(path.to_path_buf());
                        done = true
                    }
                    Err(why) => error!(
                        "cannot remove prefix '{}' from '{}', error: {:?}",
                        path.display(),
                        install_path.device_info.mountpoint.display(),
                        why
                    ),
                }
            } else {
                dev_paths.push(path.clone());
                done = true;
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

        debug!("converted paths for uEnv.txt: {}", dev_paths.len());

        let mut uenv_text = String::from(BALENA_FILE_TAG);
        uenv_text.push_str(UENV_TXT2);
        uenv_text = uenv_text.replace("__DTB_PATH__", &dev_paths.pop().unwrap().to_string_lossy());
        uenv_text = uenv_text.replace(
            "__INITRD_PATH__",
            &dev_paths.pop().unwrap().to_string_lossy(),
        );
        uenv_text = uenv_text.replace(
            "__KERNEL_PATH__",
            &dev_paths.pop().unwrap().to_string_lossy(),
        );
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

    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr
        debug!("can_migrate: entered");

        // determine flash device - either from config or root device
        let flash_device = if let Some(dev_path) = config.migrate.get_force_flash_device() {
            warn!("Config forces use of flash device '{}'", dev_path.display());
            BlockDevice::from_device_path(dev_path)?
        } else {
            let (root_dev, _root_fs_type) = get_kernel_root_info()?;
            // TODO: might have to support other devices than mmcblk ?
            if let Some(captures) = Regex::new(r##"(/dev/mmcblk\d)p\d$"##)
                .unwrap()
                .captures(&*root_dev.to_string_lossy())
            {
                let root_dev = PathBuf::from(captures.get(1).unwrap().as_str());
                info!(
                    "Using root device as flash device: '{}'",
                    root_dev.display()
                );

                BlockDevice::from_device_path(&root_dev)?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvState,
                    &format!(
                        "Invalid flash device type '{}', expected mmcblk device",
                        root_dev.display()
                    ),
                ));
            }
        };

        debug!("can_migrate: using flash device '{}'", flash_device.name);

        let mut uboot_info = UBootInfo {
            flash_device,
            in_mbr: false,
            mlo_path: None,
            install_path: None,
            uenv_path: Vec::new(),
        };

        // scan flash_devices drives for uboot boot manager and suitable install locations
        if let Some(ref lsblk_parts) = uboot_info.flash_device.children {
            debug!(
                "get_uboot_info: '{}' has children",
                uboot_info.flash_device.name
            );

            {
                let mut flash_disk =
                    Disk::from_drive_file(&uboot_info.flash_device.get_path(), None)?;

                for (index, partition) in PartitionIterator::new(&mut flash_disk)?.enumerate() {
                    debug!(
                        "get_uboot_info: looking at partition {}: {:?}",
                        index, partition
                    );

                    let lsblk_part = if let Some(lsblk_part) = lsblk_parts.get(index) {
                        lsblk_part
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvState,
                            &format!(
                                "Failed to retrieve lsblk_info for partition index {}",
                                index
                            ),
                        ));
                    };

                    if let Some(ref fs_type) = lsblk_part.fstype {
                        // uboot can only handle certain fstypes
                        match fs_type.as_str() {
                            "vfat" | "ext2" | "ext4" => (),
                            _ => {
                                warn!(
                                    "Skipping partition '{}' due to invalid fstype: {}",
                                    lsblk_part.name, fs_type
                                );
                                continue;
                            }
                        }
                    }

                    // mount the drive if not already mounted
                    let mountpoint = if let Some(ref mountpoint) = lsblk_part.mountpoint {
                        mountpoint.clone()
                    } else {
                        tmp_mount(lsblk_part.get_path(), &lsblk_part.fstype, &None)?
                    };

                    let path_info = PathInfo::from_mounted(
                        &path_append(&mountpoint, BOOT_PATH),
                        &mountpoint,
                        &uboot_info.flash_device,
                        &lsblk_part,
                    )?;

                    if uboot_info.install_path.is_none()
                        && self.check_boot_req_space(&path_info, mig_info)?
                    {
                        // enough space to install hereget_
                        uboot_info.install_path = Some(path_info.clone());
                    }

                    if uboot_info.mlo_path.is_none()
                        && partition.is_bootable()
                        && ((partition.ptype == 0xe) || (partition.ptype == 0xc))
                        && path_append(&mountpoint, MLO_FILE_NAME).exists()
                        && path_append(&mountpoint, UBOOT_FILE_NAME).exists()
                    {
                        info!("Found uboot bootloader partition '{:?}'", partition);
                        // uboot boot manager found here
                        uboot_info.mlo_path = Some(path_info.clone());
                    }

                    // check for existing uEnv.txt files that might need to be hidden / backed up
                    let uenv_path = path_append(&mountpoint, UENV_FILE_NAME);
                    if uenv_path.exists() {
                        uboot_info.uenv_path.push(uenv_path);
                    }
                    let uenv_path =
                        path_append(path_append(&mountpoint, BOOT_PATH), UENV_FILE_NAME);

                    if uenv_path.exists() {
                        uboot_info.uenv_path.push(uenv_path);
                    }
                }
            }

            // check for uboot boot manager in MBR
            let mut dev_file = OpenOptions::new()
                .read(true)
                .write(false)
                .create(false)
                .open(&uboot_info.flash_device.get_path())
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to open device for reading '{}'",
                        uboot_info.flash_device.get_path().display()
                    ),
                ))?;

            dev_file
                .seek(SeekFrom::Start(UBOOT_DEV_OFFSET))
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed seek operation on device '{}'",
                        uboot_info.flash_device.get_path().display()
                    ),
                ))?;

            let mut buffer: [u8; 4] = [0; 4];
            dev_file
                .read_exact(&mut buffer)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed read operation on device '{}'",
                        uboot_info.flash_device.get_path().display()
                    ),
                ))?;

            uboot_info.in_mbr = UBootManager::u32_from_big_endian(&buffer, 0) == UBOOT_MAGIC_WORD;

            if (uboot_info.in_mbr || uboot_info.mlo_path.is_some())
                && uboot_info.install_path.is_some()
            {
                Ok(true)
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvState,
                    &format!(
                        "Cannot setup boot from device '{}' - no current u-boot boot loader found or no space for installation",
                        uboot_info.flash_device.get_path().display()
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!(
                    "No partitions found on flash device: '{}'",
                    uboot_info.flash_device.name
                ),
            ))
        }
    }

    fn setup(
        &mut self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        // TODO: setup MLO/u-boot.img

        if let Some(ref uboot_info) = self.uboot_info {
            let mut boot_cfg_bckup: Vec<BackupCfg> = Vec::new();

            // copy u-boot boot manager
            if let Some(ref mlo_path) = uboot_info.mlo_path {
                let mlo_src = path_append(&mlo_path.path, MLO_FILE_NAME);
                let mlo_dest = path_append(
                    &mlo_path.path,
                    format!(
                        "{}-{}",
                        &mlo_src.to_string_lossy(),
                        Local::now().format("%s")
                    ),
                );
                fs::rename(&mlo_src, &mlo_dest).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to rename '{}' to '{}'",
                        mlo_src.display(),
                        mlo_dest.display()
                    ),
                ))?;

                boot_cfg_bckup.push(BackupCfg::from_device_info(
                    &mlo_path.device_info,
                    mlo_src.as_path(),
                    mlo_dest.as_path(),
                ));

                let mlo_dest = mlo_src;
                let mlo_src = path_append(&mig_info.work_path.path, MLO_FILE_NAME);
                fs::copy(&mlo_src, &mlo_dest).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to copy '{}' to '{}'",
                        mlo_src.display(),
                        mlo_dest.display()
                    ),
                ))?;

                let uboot_src = path_append(&mlo_path.path, UBOOT_FILE_NAME);
                let uboot_dest = path_append(
                    &mlo_path.path,
                    format!(
                        "{}-{}",
                        &uboot_src.to_string_lossy(),
                        Local::now().format("%s")
                    ),
                );
                fs::rename(&uboot_src, &uboot_dest).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to rename '{}' to '{}'",
                        uboot_src.display(),
                        uboot_dest.display()
                    ),
                ))?;

                boot_cfg_bckup.push(BackupCfg::from_device_info(
                    &mlo_path.device_info,
                    uboot_src.as_path(),
                    uboot_dest.as_path(),
                ));

                let uboot_dest = uboot_src;
                let uboot_src = path_append(&mig_info.work_path.path, UBOOT_FILE_NAME);
                fs::copy(&uboot_src, &uboot_dest).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to copy '{}' to '{}'",
                        uboot_src.display(),
                        uboot_dest.display()
                    ),
                ))?;
            } else {
                if uboot_info.in_mbr {}
            }

            let res = match self.strategy {
                UEnvStrategy::UName => self.strategy_uname(
                    uboot_info,
                    mig_info,
                    config,
                    kernel_opts,
                    &mut boot_cfg_bckup,
                ),
                UEnvStrategy::Manual => {
                    self.strategy_manual(uboot_info, mig_info, kernel_opts, &mut boot_cfg_bckup)
                }
            };
            s2_cfg.set_boot_bckup(boot_cfg_bckup);
            res
        } else {
            error!("setup: boot manager is missing config data",);
            Err(MigError::displayed())
        }

        // TODO: allow devices other than mmcblk
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

        if !restore_backups(config.get_boot_backups(), Some(PathBuf::from(MOUNT_DIR))) {
            res = false;
        }

        // TODO: remove kernel & initramfs, dtb  too
        res
    }

    fn get_bootmgr_path(&self) -> PathInfo {
        unimplemented!()
    }
}
