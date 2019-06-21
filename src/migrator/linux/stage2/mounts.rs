use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use nix::mount::{mount, umount, MsFlags};

use crate::{
    common::{
        dir_exists, file_exists, path_append,
        stage2_config::{PathType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{DISK_BY_LABEL_PATH, STAGE2_CFG_FILE},
    linux::{
        ensured_cmds::{EnsuredCmds, UDEVADM_CMD},
        linux_common::{drive_from_partition, get_kernel_root_info, to_std_device_path},
        linux_defs::{
            BALENA_BOOT_FSTYPE, BALENA_BOOT_PART, BALENA_DATA_FSTYPE, BALENA_DATA_PART,
            BALENA_ROOTA_PART, BALENA_ROOTB_PART, BALENA_STATE_PART, NIX_NONE,
        },
        migrate_info::LsblkInfo,
    },
};

const MOUNT_DIR: &str = "/tmp_mount";
const BOOTFS_DIR: &str = "boot";
const WORKFS_DIR: &str = "work";
const LOGFS_DIR: &str = "log";

const BOOT_MNT_DIR: &str = "mnt_boot";
const DATA_MNT_DIR: &str = "mnt_data";

const UDEVADM_PARAMS: &[&str] = &["settle", "-t", "10"];

const TRY_FS_TYPES: &[&str] = &["ext4", "vfat", "ntfs", "ext2", "ext3"];

/*
Attempts to mount the former boot device
First approach is to extract root & root fs type from kernel command line
If that fails all relevant block devices are searched for STAGE2_CFG_FILE.

This device will be used to flash:
 drive path in flash_device
 partition in boot_part

*/

// TODO: test fallback strategy - boot device search

#[derive(Debug)]
pub(crate) struct Mounts {
    stage2_config: PathBuf,
    flash_device: PathBuf,
    boot_part: PathBuf,
    boot_mountpoint: PathBuf,
    work_no_copy: bool,
    work_path: Option<PathBuf>,
    work_mountpoint: Option<PathBuf>,
    log_path: Option<PathBuf>,
}

impl<'a> Mounts {
    // extract device / fstype from kerne cmd line and mount device
    // check if /balena-stage2-yml is found in device root
    // if that fails scan devices for /balena-stage2.yml as a fallback -
    // fallback should not be needed except for windows migration
    // as unix device names can not be reliably guessed (so far) in
    // windows. Have to rely on device UUIDs or this fallback

    // TODO: might make sense to further redesign this:
    // get lsblk info for starters and pick all devices from there
    // might get us in trouble with devices showing up slowly though

    pub fn new(cmds: &mut EnsuredCmds) -> Result<Mounts, MigError> {
        trace!("new: entered");

        // obtain boot device from kernel cmdline
        let (kernel_root_device, kernel_root_fs_type) = get_kernel_root_info()?;

        debug!(
            "Kernel cmd line points to root device '{}' with fs-type: '{:?}'",
            kernel_root_device.display(),
            kernel_root_fs_type,
        );

        // Not sure if this is needed but can't hurt to be patient
        thread::sleep(Duration::from_secs(3));

        info!("calling {} {:?}", UDEVADM_CMD, UDEVADM_PARAMS);
        match cmds.call(UDEVADM_CMD, UDEVADM_PARAMS, true) {
            Ok(cmd_res) => {
                if !cmd_res.status.success() {
                    warn!(
                        "{} {:?} failed with '{}'",
                        UDEVADM_CMD, UDEVADM_PARAMS, cmd_res.stderr
                    );
                }
            }
            Err(why) => {
                warn!("{} {:?} failed with {:?}", UDEVADM_CMD, UDEVADM_PARAMS, why);
            }
        }

        // try mount root from kernel cmd line

        let mut fstypes: Vec<String> = Vec::new();
        if let Some(ref fstype) = kernel_root_fs_type {
            fstypes.push(fstype.clone());
        } else {
            TRY_FS_TYPES
                .iter()
                .for_each(|s| fstypes.push(String::from(*s)));
        }

        for fstype in &fstypes {
            match Mounts::mount(BOOTFS_DIR, &kernel_root_device, fstype) {
                Ok(boot_mountpoint) => {
                    let stage2_config = path_append(&boot_mountpoint, STAGE2_CFG_FILE);
                    if file_exists(&stage2_config) {
                        return Ok(Mounts {
                            flash_device: match drive_from_partition(&kernel_root_device) {
                                Ok(flash_device) => flash_device,
                                Err(why) => {
                                    error!(
                                        "Failed to extract drive from partition: '{}', error: {:?}",
                                        kernel_root_device.display(),
                                        why
                                    );
                                    return Err(MigError::displayed());
                                }
                            },
                            boot_part: to_std_device_path(&kernel_root_device)?,
                            boot_mountpoint,
                            stage2_config,
                            work_no_copy: false,
                            work_path: None,
                            work_mountpoint: None,
                            log_path: None,
                        });
                    } else {
                        let _res = umount(&boot_mountpoint);
                    }
                }
                Err(why) => {
                    error!("Mount failed: {:?}", why);
                }
            }
        }

        match LsblkInfo::new(cmds) {
            Ok(lsblk_info) => {
                for blk_device in lsblk_info.get_blk_devices() {
                    if let Some(ref children) = blk_device.children {
                        for blk_part in children {
                            let mut fstypes: Vec<String> = Vec::new();
                            if let Some(ref fstype) = blk_part.fstype {
                                fstypes.push(fstype.clone());
                            } else {
                                TRY_FS_TYPES
                                    .iter()
                                    .for_each(|s| fstypes.push(String::from(*s)));
                            }

                            for fstype in fstypes {
                                let device = blk_part.get_path();
                                match Mounts::mount(BOOTFS_DIR, &device, &fstype) {
                                    Ok(boot_mountpoint) => {
                                        let stage2_config =
                                            path_append(&boot_mountpoint, STAGE2_CFG_FILE);
                                        if file_exists(&stage2_config) {
                                            return Ok(Mounts {
                                                flash_device: blk_device.get_path(),
                                                boot_part: device,
                                                boot_mountpoint,
                                                stage2_config,
                                                work_no_copy: false,
                                                work_path: None,
                                                work_mountpoint: None,
                                                log_path: None,
                                            });
                                        } else {
                                            umount(&boot_mountpoint);
                                        }
                                    }
                                    Err(why) => {
                                        error!("Mount failed: {:?}", why);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(why) => {
                warn!("Failed to retrieve block device info: {:?}", why);
                return Err(MigError::displayed());
            }
        }

        error!(
            "Failed to detect a boot device containing {}",
            STAGE2_CFG_FILE
        );
        Err(MigError::displayed())
    }

    pub fn get_boot_mountpoint(&'a self) -> &'a Path {
        &self.boot_mountpoint
    }

    pub fn get_stage2_config(&'a self) -> &'a Path {
        &self.stage2_config
    }

    pub fn get_flash_device(&'a self) -> &'a Path {
        &self.flash_device
    }

    pub fn is_work_no_copy(&'a self) -> bool {
        self.work_no_copy
    }

    pub fn get_work_path(&'a self) -> Option<&'a Path> {
        if let Some(ref work_path) = self.work_path {
            Some(work_path)
        } else {
            None
        }
    }

    pub fn get_log_path(&'a self) -> Option<&'a Path> {
        if let Some(ref log_path) = self.log_path {
            Some(log_path)
        } else {
            None
        }
    }

    pub fn mount_all(&mut self, stage2_config: &Stage2Config) -> Result<(), MigError> {
        trace!("mount_all: entered");

        if let Some((log_dev, log_fs)) = stage2_config.get_log_device() {
            self.log_path = match Mounts::mount(LOGFS_DIR, log_dev, log_fs) {
                Ok(mountpoint) => Some(mountpoint),
                Err(why) => {
                    warn!(
                        "Failed to mount log device: '{}': error: {:?}",
                        log_dev.display(),
                        why
                    );
                    None
                }
            };
        }

        debug!("log mountpoint is {:?}", self.log_path);

        match stage2_config.get_work_path() {
            PathType::Path(work_path) => {
                self.work_path = Some(path_append(&self.boot_mountpoint, work_path));
                debug!("Work mountpoint is a path: {:?}", self.work_path);
            }
            PathType::Mount(mount_cfg) => {
                let device = to_std_device_path(mount_cfg.get_device())?;
                debug!("Work mountpoint is a mount: '{}'", device.display());
                // TODO: make all mounts retry with timeout
                if self.boot_part != device {
                    match Mounts::mount(WORKFS_DIR, &device, mount_cfg.get_fstype()) {
                        Ok(mountpoint) => {
                            match drive_from_partition(&device) {
                                Ok(drive) => self.work_no_copy = drive != self.flash_device,
                                Err(why) => {
                                    warn!("Failed to derive drive from work partition: '{}', error: {:?}", device.display(), why);
                                }
                            };
                            self.work_path = Some(path_append(&mountpoint, mount_cfg.get_path()));
                            self.work_mountpoint = Some(mountpoint);
                        }
                        Err(why) => {
                            error!(
                                "Failed to mount log mount: '{}', error: {:?}",
                                device.display(),
                                why
                            );
                            return Err(MigError::displayed());
                        }
                    }
                    debug!("Work mountpoint is at path: {:?}", self.work_path);
                } else {
                    self.work_path = Some(path_append(&self.boot_mountpoint, mount_cfg.get_path()));
                    debug!("Work mountpoint is at path: {:?}", self.work_path);
                }
            }
        }

        Ok(())
    }

    pub fn unmount_log(&self) -> Result<(), MigError> {
        if let Some(ref mountpoint) = self.log_path {
            umount(mountpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to unmount log device: '{}'", mountpoint.display()),
            ))?;
        }
        Ok(())
    }

    // unmount all mounted drives except log
    // which is expected to be on a separate drive
    pub fn unmount_all(&mut self) -> Result<(), MigError> {
        // TODO: unmount work_dir if necessarry

        if let Some(ref mountpoint) = self.work_mountpoint {
            match umount(mountpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to unmount former work device: '{}'",
                    mountpoint.display()
                ),
            )) {
                Ok(_) => {
                    self.work_mountpoint = None;
                    self.work_path = None;
                    self.work_no_copy = false;
                }
                Err(why) => {
                    error!("Failed to unmount work mountpoint, error: {:?}", why);
                }
            }
        }

        // TODO: make boot moun optional ?
        umount(&self.boot_mountpoint).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to unmount former root device: '{}'",
                self.boot_mountpoint.display()
            ),
        ))?;

        Ok(())
    }

    pub fn mount_balena(&mut self) -> Result<(bool, PathBuf, Option<PathBuf>), MigError> {
        let mut parts_found = true;
        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_BOOT_PART);
        let boot_mountpoint = match Mounts::mount(BOOT_MNT_DIR, &part_label, BALENA_BOOT_FSTYPE) {
            Ok(mountpoint) => mountpoint,
            Err(why) => {
                error!(
                    "Failed to mount balena boot device: '{}'",
                    part_label.display()
                );
                return Err(MigError::displayed());
            }
        };

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_ROOTA_PART);
        if !file_exists(&part_label) {
            warn!(
                "Unable to find labeled partition: '{}'",
                part_label.display()
            );
            parts_found = false;
        }

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_ROOTB_PART);
        if !file_exists(&part_label) {
            warn!(
                "Unable to find labeled partition: '{}'",
                part_label.display()
            );
            parts_found = false;
        }

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_STATE_PART);
        if !file_exists(&part_label) {
            warn!(
                "Unable to find labeled partition: '{}'",
                part_label.display()
            );
            parts_found = false;
        }

        let part_label = path_append(DISK_BY_LABEL_PATH, BALENA_DATA_PART);
        let data_mountpoint = match Mounts::mount(DATA_MNT_DIR, &part_label, BALENA_DATA_FSTYPE) {
            Ok(mountpoint) => Some(mountpoint),
            Err(why) => {
                error!(
                    "Failed to mount balena data device: '{}'",
                    part_label.display()
                );
                parts_found = false;
                None
            }
        };

        Ok((parts_found, boot_mountpoint, data_mountpoint))
    }

    // this could be the function used to mount other drives too (boot, work)
    fn mount<P1: AsRef<Path>, P2: AsRef<Path>>(
        dir: P1,
        device: P2,
        fstype: &str,
    ) -> Result<PathBuf, MigError> {
        // TODO: retry with delay
        let device = device.as_ref();

        let mountpoint = path_append(MOUNT_DIR, dir.as_ref());
        debug!(
            "Attempting to mount '{}' on '{}' with fstype {}",
            device.display(),
            mountpoint.display(),
            fstype
        );

        if !dir_exists(&mountpoint)? {
            create_dir_all(&mountpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to create mountpoint: '{}'", mountpoint.display()),
            ))?;
        }

        for _x in 1..3 {
            if file_exists(&device) {
                let device = to_std_device_path(device.as_ref())?;

                debug!(
                    "attempting to mount '{}' on '{}' with fstype: {}",
                    device.display(),
                    mountpoint.display(),
                    fstype
                );
                mount(
                    Some(&device),
                    &mountpoint,
                    Some(fstype.as_bytes()),
                    MsFlags::empty(),
                    NIX_NONE,
                )
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to mount previous boot manager device '{}' to '{}' with fstype: {:?}",
                            &device.display(),
                            &mountpoint.display(),
                            fstype
                        ),
                    ))?;

                return Ok(mountpoint);
            } else {
                debug!(
                    "Device not found '{}' will retry in 3 seconds",
                    device.display()
                );
                thread::sleep(Duration::from_secs(3))
            }
        }

        error!("failed to find log device '{}'", device.display());
        return Err(MigError::displayed());
    }
}
