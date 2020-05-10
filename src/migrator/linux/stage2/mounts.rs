use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::str;
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};

use nix::{
    mount::{mount, umount, MsFlags},
    unistd::sync,
};

use crate::linux::linux_common::to_std_device_path;
use crate::{
    common::{
        call, dir_exists, file_exists, path_append,
        stage2_config::{PathType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{DISK_BY_LABEL_PATH, STAGE2_CFG_FILE},
    linux::{
        linux_common::{drive_from_partition, drive_to_partition, get_kernel_root_info, whereis},
        linux_defs::NIX_NONE,
        linux_defs::{FAT_CHK_CMD, LS_CMD, UDEVADM_CMD},
        stage2::{
            BALENA_BOOT_FSTYPE, BALENA_BOOT_PART, BALENA_DATA_FSTYPE, BALENA_DATA_PART,
            BALENA_ROOTA_FSTYPE, BALENA_ROOTA_PART, BALENA_ROOTB_FSTYPE, BALENA_ROOTB_PART,
            BALENA_STATE_FSTYPE, BALENA_STATE_PART,
        },
    },
};

pub(crate) const MOUNT_DIR: &str = "/tmp_mount";
const BOOTFS_DIR: &str = "boot";
const WORKFS_DIR: &str = "work";
const LOGFS_DIR: &str = "log";

const BOOT_MNT_DIR: &str = "mnt_boot";
const ROOTA_MNT_DIR: &str = "mnt_rootA";
const ROOTB_MNT_DIR: &str = "mnt_rootB";
const STATE_MNT_DIR: &str = "mnt_state";
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
    boot_device: PathBuf,
    flash_device: PathBuf,
    boot_part: PathBuf,
    boot_mountpoint: PathBuf,
    work_no_copy: bool,
    work_path: Option<PathBuf>,
    work_mountpoint: Option<PathBuf>,
    log_path: Option<PathBuf>,
    balena_boot_mp: Option<PathBuf>,
    balena_root_a_mp: Option<PathBuf>,
    balena_root_b_mp: Option<PathBuf>,
    balena_state_mp: Option<PathBuf>,
    balena_data_mp: Option<PathBuf>,
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

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    pub fn new() -> Result<Mounts, MigError> {
        debug!("new: entered");
        thread::sleep(Duration::new(5, 0));
        // obtain boot device from kernel cmdline
        match get_kernel_root_info() {
            Ok((device, fstype)) => {
                debug!(
                    "Kernel cmd line points to root device '{}' with fs-type: '{:?}'",
                    device.display(),
                    fstype,
                );

                const WAIT_TIMEOUT: Duration = Duration::from_secs(60);
                const SLEEP_TIME: Duration = Duration::from_secs(10);
                let dev_wait_start = Instant::now();

                info!("calling {} {:?}", UDEVADM_CMD, UDEVADM_PARAMS);
                match call(UDEVADM_CMD, UDEVADM_PARAMS, true) {
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

                while !device.exists() {
                    warn!(
                        "root device could not be found: '{}' waiting a little longer",
                        device.display()
                    );
                    if Instant::now().duration_since(dev_wait_start) >= WAIT_TIMEOUT {
                        let cmd_res = call(LS_CMD, &["/dev/disk/by-uuid"], true)?;
                        if cmd_res.status.success() {
                            warn!("Root device could not be found: '{}'", device.display());
                            warn!("listing of /dev/disk/by-uuid: \n{}", cmd_res.stdout);
                        }

                        return Err(MigError::from_remark(
                            MigErrorKind::NotFound,
                            &format!("Failed to find device '{}'", device.display()),
                        ));
                    }
                    sleep(SLEEP_TIME);
                }

                let mut fstypes: Vec<String> = Vec::new();
                if let Some(ref fs_type) = fstype {
                    fstypes.push(fs_type.clone());
                } else {
                    TRY_FS_TYPES
                        .iter()
                        .for_each(|s| fstypes.push(String::from(*s)));
                }

                for fstype in &fstypes {
                    match Mounts::mount(BOOTFS_DIR, &device, fstype) {
                        Ok(boot_mountpoint) => {
                            debug!("device: '{}', boot fstype: '{}'", device.display(), fstype);

                            let stage2_config = path_append(&boot_mountpoint, STAGE2_CFG_FILE);
                            if file_exists(&stage2_config) {
                                debug!("device: '{}', boot fstype: '{}'", device.display(), fstype);
                                let init_device = match drive_from_partition(&device) {
                                    Ok(flash_device) => flash_device,
                                    Err(why) => {
                                        error!(
                                            "Failed to extract drive from partition: '{}', error: {:?}",
                                            device.display(),
                                            why
                                        );
                                        thread::sleep(Duration::new(5, 0));

                                        return Err(MigError::displayed());
                                    }
                                };
                                debug!(
                                    "found '{}' on device: '{}',",
                                    stage2_config.display(),
                                    init_device.display()
                                );
                                thread::sleep(Duration::new(5, 0));

                                return Ok(Mounts {
                                    boot_device: init_device.clone(),
                                    flash_device: init_device,
                                    boot_part: device,
                                    boot_mountpoint,
                                    stage2_config,
                                    work_no_copy: false,
                                    work_path: None,
                                    work_mountpoint: None,
                                    log_path: None,
                                    balena_boot_mp: None,
                                    balena_root_a_mp: None,
                                    balena_root_b_mp: None,
                                    balena_state_mp: None,
                                    balena_data_mp: None,
                                });
                            } else {
                                let _res = umount(&boot_mountpoint);
                            }
                        }
                        Err(why) => {
                            error!(
                                "Mount failed for {} on {} wth fstype: {}, error {:?}",
                                device.display(),
                                BOOTFS_DIR,
                                fstype,
                                why
                            );
                        }
                    }
                }
                error!(
                    "Failed to detect a boot device containing {}",
                    STAGE2_CFG_FILE
                );
                Err(MigError::displayed())
            }
            Err(why) => {
                error!(
                    "Failed to retrieve root path from kernel command line, error {:?}",
                    why
                );
                Err(MigError::displayed())
            }
        }
    }

    pub fn get_balena_boot_mountpoint(&'a self) -> Option<&'a Path> {
        if let Some(ref mountpoint) = self.balena_boot_mp {
            Some(mountpoint)
        } else {
            None
        }
    }

    pub fn get_balena_root_a_mountpoint(&'a self) -> Option<&'a Path> {
        if let Some(ref mountpoint) = self.balena_root_a_mp {
            Some(mountpoint)
        } else {
            None
        }
    }

    pub fn get_balena_root_b_mountpoint(&'a self) -> Option<&'a Path> {
        if let Some(ref mountpoint) = self.balena_root_b_mp {
            Some(mountpoint)
        } else {
            None
        }
    }

    pub fn get_balena_state_mountpoint(&'a self) -> Option<&'a Path> {
        if let Some(ref mountpoint) = self.balena_state_mp {
            Some(mountpoint)
        } else {
            None
        }
    }

    pub fn get_balena_data_mountpoint(&'a self) -> Option<&'a Path> {
        if let Some(ref mountpoint) = self.balena_data_mp {
            Some(mountpoint)
        } else {
            None
        }
    }

    /*    pub fn get_boot_mountpoint(&'a self) -> &'a Path {
            &self.boot_mountpoint
        }
    */
    pub fn get_stage2_config(&'a self) -> &'a Path {
        &self.stage2_config
    }

    pub fn set_force_flash_device(&mut self, device: &Path) {
        self.flash_device = device.to_path_buf();
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

    pub fn mount_from_config(&mut self, stage2_config: &Stage2Config) -> Result<(), MigError> {
        trace!("mount_all: entered");

        // TODO: ensure nothing is mounted twice, eg: work_mount == log_mount

        if let Some(log_dev) = stage2_config.get_log_device() {
            // TODO: establish fs_type ?
            let log_device = to_std_device_path(&log_dev.device)?;
            self.log_path = match Mounts::mount(LOGFS_DIR, &log_device, log_dev.fs_type.as_str()) {
                Ok(mountpoint) => Some(mountpoint),
                Err(why) => {
                    warn!(
                        "Failed to mount log device: '{}': error: {:?}",
                        log_device.display(),
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
                self.work_no_copy = self.boot_device != self.flash_device;
                debug!("work_no_copy set to {}", self.work_no_copy);
            }
            PathType::Mount(mount_cfg) => {
                let device = to_std_device_path(mount_cfg.get_device())?;
                debug!("Work mountpoint is a mount: '{}'", device.display());
                // TODO: make all mounts retry with timeout
                if self.boot_part != device {
                    match Mounts::mount(WORKFS_DIR, &device, mount_cfg.get_fstype()) {
                        Ok(mountpoint) => {
                            match drive_from_partition(&device) {
                                Ok(drive) => {
                                    self.work_no_copy = drive != self.flash_device;
                                    debug!("work_no_copy set to {}", self.work_no_copy);
                                }
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
            sync();
            umount(mountpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to unmount log device: '{}'", mountpoint.display()),
            ))?;
        }
        Ok(())
    }

    // unmount all mounted drives except log
    // which is expected to be on a separate drive
    pub fn unmount_boot_devs(&mut self) -> Result<(), MigError> {
        sync();
        if let Some(ref mountpoint) = self.work_mountpoint {
            if self.work_no_copy {
                debug!("Not unmounting work_dir as it is separate from flash_device");
            } else {
                match umount(mountpoint) {
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
        }

        // TODO: make boot mount optional ?
        if self.boot_device == self.flash_device {
            debug!(
                "Unmounting boot device: '{}' from '{}'",
                self.boot_device.display(),
                self.boot_mountpoint.display()
            );
            match umount(&self.boot_mountpoint) {
                Ok(_) => Ok(()),
                Err(why) => {
                    error!(
                        "Failed to unmount former boot device: '{}', error: {:?}",
                        self.boot_mountpoint.display(),
                        why
                    );
                    Err(MigError::displayed())
                }
            }
        } else {
            debug!(
                "Not unmounting boot device: '{}' as it is different from flash_device: '{}'",
                self.boot_device.display(),
                self.flash_device.display()
            );
            Ok(())
        }
    }

    pub fn mount_balena(&mut self, mount_all: bool) -> Result<bool, MigError> {
        let mut parts_found = true;
        let mut part_label = path_append(DISK_BY_LABEL_PATH, BALENA_BOOT_PART);
        if !file_exists(&part_label) {
            part_label = drive_to_partition(&self.flash_device, 1)?;
        }

        self.balena_boot_mp = match Mounts::mount(BOOT_MNT_DIR, &part_label, BALENA_BOOT_FSTYPE) {
            Ok(mountpoint) => Some(mountpoint),
            Err(why) => {
                error!(
                    "Failed to mount balena device: '{}', error: {:?}",
                    part_label.display(),
                    why
                );
                return Err(MigError::displayed());
            }
        };

        let mut part_label = path_append(DISK_BY_LABEL_PATH, BALENA_ROOTA_PART);
        if !file_exists(&part_label) {
            part_label = drive_to_partition(&self.flash_device, 2)?;
        }

        if mount_all {
            self.balena_root_a_mp =
                match Mounts::mount(ROOTA_MNT_DIR, &part_label, BALENA_ROOTA_FSTYPE) {
                    Ok(mountpoint) => Some(mountpoint),
                    Err(why) => {
                        error!(
                            "Failed to mount balena device: '{}', error: {:?}",
                            part_label.display(),
                            why
                        );
                        return Err(MigError::displayed());
                    }
                };
        } else if !file_exists(&part_label) {
            warn!(
                "Unable to find labeled partition: '{}'",
                part_label.display()
            );
            parts_found = false;
        }

        let mut part_label = path_append(DISK_BY_LABEL_PATH, BALENA_ROOTB_PART);
        if !file_exists(&part_label) {
            part_label = drive_to_partition(&self.flash_device, 3)?;
        }

        if mount_all {
            self.balena_root_b_mp =
                match Mounts::mount(ROOTB_MNT_DIR, &part_label, BALENA_ROOTB_FSTYPE) {
                    Ok(mountpoint) => Some(mountpoint),
                    Err(why) => {
                        error!(
                            "Failed to mount balena device: '{}', error: {:?}",
                            part_label.display(),
                            why
                        );
                        return Err(MigError::displayed());
                    }
                };
        } else if !file_exists(&part_label) {
            warn!(
                "Unable to find labeled partition: '{}'",
                part_label.display()
            );
            parts_found = false;
        }

        let mut part_label = path_append(DISK_BY_LABEL_PATH, BALENA_STATE_PART);
        if !file_exists(&part_label) {
            part_label = drive_to_partition(&self.flash_device, 5)?;
        }

        if mount_all {
            self.balena_state_mp =
                match Mounts::mount(STATE_MNT_DIR, &part_label, BALENA_STATE_FSTYPE) {
                    Ok(mountpoint) => Some(mountpoint),
                    Err(why) => {
                        error!(
                            "Failed to mount balena device: '{}', error: {:?}",
                            part_label.display(),
                            why
                        );
                        return Err(MigError::displayed());
                    }
                };
        } else if !file_exists(&part_label) {
            warn!(
                "Unable to find labeled partition: '{}'",
                part_label.display()
            );
            parts_found = false;
        }

        let mut part_label = path_append(DISK_BY_LABEL_PATH, BALENA_DATA_PART);
        if !file_exists(&part_label) {
            part_label = drive_to_partition(&self.flash_device, 6)?;
        }

        self.balena_data_mp = match Mounts::mount(DATA_MNT_DIR, &part_label, BALENA_DATA_FSTYPE) {
            Ok(mountpoint) => Some(mountpoint),
            Err(why) => {
                error!(
                    "Failed to mount balena data device: '{}', error: {:?}",
                    part_label.display(),
                    why
                );

                if mount_all {
                    return Err(MigError::displayed());
                } else {
                    parts_found = false;
                    None
                }
            }
        };

        Ok(parts_found)
    }

    pub fn unmount_balena(&mut self) -> bool {
        let mut success = true;

        sync();
        // below: debug missing network_manager files on bbxm
        thread::sleep(Duration::from_secs(5));

        if let Some(ref mountpoint) = self.balena_boot_mp {
            debug!("unmounting '{}'", mountpoint.display());
            if let Err(why) = umount(mountpoint) {
                error!(
                    "Failed to unmount device from mountpoint '{}', error: {:?}",
                    mountpoint.display(),
                    why
                );
                success = false;
            } else {
                self.balena_boot_mp = None;
            }
        }

        if let Some(ref mountpoint) = self.balena_root_a_mp {
            debug!("unmounting '{}'", mountpoint.display());
            if let Err(why) = umount(mountpoint) {
                error!(
                    "Failed to unmount device from mountpoint '{}', error: {:?}",
                    mountpoint.display(),
                    why
                );
                success = false;
            } else {
                self.balena_root_a_mp = None;
            }
        }

        if let Some(ref mountpoint) = self.balena_root_b_mp {
            debug!("unmounting '{}'", mountpoint.display());
            if let Err(why) = umount(mountpoint) {
                error!(
                    "Failed to unmount device from mountpoint '{}', error: {:?}",
                    mountpoint.display(),
                    why
                );
                success = false;
            } else {
                self.balena_root_b_mp = None;
            }
        }

        if let Some(ref mountpoint) = self.balena_state_mp {
            debug!("unmounting '{}'", mountpoint.display());
            if let Err(why) = umount(mountpoint) {
                error!(
                    "Failed to unmount device from mountpoint '{}', error: {:?}",
                    mountpoint.display(),
                    why
                );
                success = false;
            } else {
                self.balena_state_mp = None;
            }
        }

        if let Some(ref mountpoint) = self.balena_data_mp {
            debug!("unmounting '{}'", mountpoint.display());
            if let Err(why) = umount(mountpoint) {
                error!(
                    "Failed to unmount device from mountpoint '{}', error: {:?}",
                    mountpoint.display(),
                    why
                );
                success = false;
            }
            self.balena_data_mp = None;
        }

        success
    }

    // this could be the function used to mount other drives too (boot, work)
    fn mount<P1: AsRef<Path>, P2: AsRef<Path>>(
        dir: P1,
        device: P2,
        fstype: &str,
    ) -> Result<PathBuf, MigError> {
        // TODO: retry with delay
        let device = device.as_ref().to_path_buf();

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

        for attempt in 1..3 {
            if file_exists(&device) {
                debug!(
                    "Found device '{}' on attempt {} mounting on '{}' with fstype: {}",
                    device.display(),
                    attempt,
                    mountpoint.display(),
                    fstype
                );

                if fstype == "vfat" {
                    debug!("checking fat file system on '{}'", device.display());
                    if let Ok(path) = whereis(FAT_CHK_CMD) {
                        match call(&path, &["-a", &device.to_string_lossy()], true) {
                            Ok(cmd_res) => {
                                if !cmd_res.status.success() {
                                    warn!(
                                        "Failed to check file system '{}': {} ",
                                        device.display(),
                                        cmd_res.stderr
                                    );
                                }
                            }
                            Err(why) => {
                                warn!(
                                    "Failed to check file system '{}': {:?} ",
                                    device.display(),
                                    why
                                );
                            }
                        }
                    }
                }

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
                        "Failed to mount device '{}' to '{}' with fstype: {:?}",
                        &device.display(),
                        &mountpoint.display(),
                        fstype
                    ),
                ))?;

                return Ok(mountpoint);
            } else {
                debug!(
                    "Device '{}'  not found in attempt {}, will retry in 3 seconds",
                    device.display(),
                    attempt
                );
                thread::sleep(Duration::from_secs(3))
            }
        }

        error!("failed to find log device '{}'", device.display());
        Err(MigError::displayed())
    }
}
