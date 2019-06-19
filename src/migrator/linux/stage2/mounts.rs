use failure::{ResultExt};
use std::path::{PathBuf, Path};
use std::fs::{create_dir_all, read_dir, read_to_string};
use log::{trace, info, warn, debug, error};
use std::thread;
use std::time::{Duration};

use nix::{
    mount::{mount, umount, MsFlags},
};

use crate::{
    defs::{STAGE2_CFG_FILE},
    linux::{
        linux_common::{to_std_device_path, get_kernel_root_info, drive_from_partition},
        linux_defs::{NIX_NONE, },
        ensured_cmds::{EnsuredCmds, UDEVADM_CMD},
    },
    common::{
        dir_exists, file_exists, path_append,
        MigError, MigErrorKind, MigErrCtx,
        stage2_config::{
            PathType,
        }
    }
};

use crate::common::stage2_config::Stage2Config;
use crate::common::call;
use mod_logger::{Logger, LogDestination};

const MOUNT_DIR: &str = "/tmp_mount";
const BOOTFS_DIR: &str = "boot";
const WORKFS_DIR: &str = "work";
const LOGFS_DIR: &str = "log";

const UDEVADM_PARAMS: &[&str] = &["settle", "-t", "10"];

/*
Attempts to mount the former boot device
First approach is to extract root & root fs type from kernel command line
If that fails all relevant block devices are searched for STAGE2_CFG_FILE.

This device will be used to flash:
 drive path in flash_device
 partition in boot_part

*/
#[derive(Debug)]
pub(crate) struct Mounts {
    stage2_config: PathBuf,
    flash_device: PathBuf,
    boot_part: PathBuf,
    boot_mountpoint: PathBuf,
    work_path: Option<PathBuf>,
    work_device: Option<PathBuf>,
    log_path: Option<PathBuf>,
    log_device: Option<PathBuf>,
}


impl<'a> Mounts {
    pub fn new(cmds: &mut EnsuredCmds) -> Result<Mounts, MigError> {
        trace!("new: entered");
        let boot_mountpoint = PathBuf::from(path_append(MOUNT_DIR, BOOTFS_DIR));

        let stage2_config = path_append(&boot_mountpoint, STAGE2_CFG_FILE);

        let (kernel_root_device, kernel_root_fs_type) = get_kernel_root_info()?;

        info!(
            "Kernel cmd line points to root device '{}' with fs-type: '{:?}'",
            kernel_root_device.display(),
            kernel_root_fs_type,
        );


        debug!("letting things mature for a while");
        thread::sleep(Duration::from_secs(3));

        debug!("attempting {} {:?}", UDEVADM_CMD, UDEVADM_PARAMS);

        if let Ok(command) = cmds.ensure(UDEVADM_CMD) {
            debug!("calling {} {:?}", command, UDEVADM_PARAMS);
            match call(&command, UDEVADM_PARAMS, true) {
                Ok(cmd_res) => {
                    if !cmd_res.status.success() {
                        warn!("{} {:?} failed with '{}'", command, UDEVADM_PARAMS, cmd_res.stderr);
                    }
                },
                Err(why) => {
                    warn!("{} {:?} failed with {:?}", command, UDEVADM_PARAMS, why);
                }
            }
            debug!("{} {:?} done", command, UDEVADM_PARAMS);
        } else {
            warn!("{} is not available", UDEVADM_CMD);
        }

        if !dir_exists(&boot_mountpoint)? {
            create_dir_all(&boot_mountpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create mountpoint for boot fs in {}",
                    &boot_mountpoint.display()
                ),
            ))?;
            debug!("created root mount directory {}", &boot_mountpoint.display());
        } else {
            warn!("root mount directory {} exists", &boot_mountpoint.display());
        }

        // try find root from kernel cmd line
        let mut boot_part =
            if file_exists(&kernel_root_device) {
                debug!(
                    "mounting root device '{}' on '{}' with fs type: {:?}",
                    kernel_root_device.display(),
                    boot_mountpoint.display(),
                    kernel_root_fs_type
                );

                match mount(
                        Some(&kernel_root_device),
                        &boot_mountpoint,
                        if let Some(ref fs_type) = kernel_root_fs_type {
                            Some(fs_type.as_bytes())
                        } else {
                            NIX_NONE
                        },
                        MsFlags::empty(),
                        NIX_NONE,
                    ) {
                    Ok(_) => {
                        info!("Mount succeeded");
                    },
                    Err(why) => {
                        error!(
                            "Failed to mount previous root device '{}' to '{}' with type: {:?}",
                            &kernel_root_device.display(),
                            &boot_mountpoint.display(),
                            kernel_root_fs_type
                        );
                        return Err(MigError::displayed());
                    }
                }

/*
                let log_path = path_append(&boot_mountpoint, "migrate.log");
                match Logger::set_log_file(&LogDestination::Stderr, &log_path) {
                    Ok(_) => {
                        info!("now logging to '{}'", log_path.display());
                    },
                    Err(why) => {
                        warn!("failed to set up logging to '{}' : {:?}", log_path.display(), why);
                    }
                }
*/

                debug!("looking for '{}'", stage2_config.display());

                if !file_exists(&stage2_config) {
                    let message = format!(
                        "failed to locate stage2 config in {}",
                        stage2_config.display()
                    );
                    error!("{}", &message);
                    umount(&boot_mountpoint)
                        .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to unmount from: '{}'", boot_mountpoint.display())))?;
                    None
                } else {
                    debug!("File found, returning '{}'", kernel_root_device.display());
                    Some(kernel_root_device)
                }
            } else {
                None
            };

        if let None = boot_part {
            debug!("Attempting to find boot mount on any drive");
            boot_part = Mounts::find_boot_mount(&stage2_config, &boot_mountpoint, &kernel_root_fs_type);
        }

        debug!("boot mount {:?}", boot_part);

        if let Some(boot_part) = boot_part {
            Ok(Mounts{
                flash_device: match drive_from_partition(&boot_part) {
                    Ok(flash_device) => flash_device,
                    Err(why) => {
                        error!("Failed to extract drive from partition: '{}', error: {:?}", boot_part.display(), why);
                        Logger::flush();
                        return Err(MigError::displayed());
                    }
                },
                boot_part,
                boot_mountpoint,
                stage2_config,
                work_path: None,
                work_device: None,
                log_device: None,
                log_path: None
            })
        } else {
            error!("Failed to find a device containing the stage2 config. Giving up");
            Err(MigError::displayed())
        }
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

    pub fn get_work_path(&'a self) -> &'a Path {
        self.work_path.as_ref().unwrap()
    }

    pub fn get_log_path(&'a self) -> Option<&'a Path> {
        if let Some(ref log_path) = self.log_path {
            Some(log_path)
        } else {
            None
        }
    }

    pub fn unmount_all(&mut self,) -> Result<(),MigError> {
        // TODO: unmount work_dir if necessarry
        umount(&self.boot_mountpoint)
            .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to unmount former root device: '{}'",
                self.boot_mountpoint.display()
            ),
        ))?;
        Ok(())
    }

    pub fn mount_log(&mut self, device: &Path, fstype: &str) -> Result<Option<PathBuf>,MigError> {
        // TODO: retry with delay
        let device = to_std_device_path(device)?;
        let mountpoint = path_append(MOUNT_DIR, LOGFS_DIR);
        match create_dir_all(&mountpoint) {
            Ok(_) => {
                for x in 1..4 {
                    if file_exists(&device) {
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

                        return Ok(Some(mountpoint))
                    } else {
                        thread::sleep(Duration::from_secs(3))
                    }
                }

                error!("failed to find log device '{}'", device.display());
                return Err(MigError::displayed())
            },
            Err(why) => {
                error!("Failed to create mountpoint: '{}' for log : {:?}", mountpoint.display(), why);
                Err(MigError::displayed())
            }
        }
    }

    pub fn mount_all(&mut self, stage2_config: &Stage2Config) -> Result<(),MigError> {
        trace!("mount_all: entered");

        if let Some((log_dev, log_fs)) = stage2_config.get_log_device() {
            let _res = self.mount_log(log_dev, log_fs);
        }

        match stage2_config.get_work_path() {
            PathType::Path(work_path) => {
                self.work_path = Some(work_path.clone());
            },
            PathType::Mount(mount_cfg) => {
                let device = to_std_device_path(mount_cfg.get_device())?;

                if self.boot_part != device {
                    let mountpoint = PathBuf::from(path_append(MOUNT_DIR, WORKFS_DIR));
                    debug!(
                        "attempting to mount '{}' on '{}' with fstype: {}",
                        device.display(),
                        mountpoint.display(),
                        mount_cfg.get_fstype()
                    );
                    mount(
                        Some(&device),
                        &mountpoint,
                        Some(mount_cfg.get_fstype()),
                        MsFlags::empty(),
                        NIX_NONE,
                    )
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to mount previous boot manager device '{}' to '{}' with fstype: {:?}",
                                &device.display(),
                                &mountpoint.display(),
                                mount_cfg.get_fstype()
                            ),
                        ))?;

                    self.work_path = Some(mountpoint);
                    self.work_device = Some(device);
                }
            }
        }

        /*
        let boot_path = path_append(&root_fs_dir, BOOT_PATH);

        let (boot_mount,boot_device) =
            if dir_exists(&boot_path)? {
                // TODO: provide fstype for boot
                if let Some(boot_mount) = stage2_cfg.get_boot_mount() {
                    let boot_device = to_std_device_path(boot_mount.get_device())?;


                    if boot_device != root_device {
                        let mountpoint = path_append(&root_fs_dir, boot_mount.get_mountpoint());
                        debug!(
                            "attempting to mount '{}' on '{}' with fstype: {}",
                            boot_device.display(),
                            mountpoint.display(),
                            boot_mount.get_fstype()
                        );
                        mount(
                            Some(&boot_device),
                            &mountpoint,
                            Some(boot_mount.get_fstype()),
                            MsFlags::empty(),
                            NIX_NONE,
                        )
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "Failed to mount previous boot device '{}' to '{}' with fstype: {:?}",
                                    &boot_mount.get_device().display(),
                                    &mountpoint.display(),
                                    boot_mount.get_fstype()
                                ),
                            ))?;
                        (Some(boot_path), Some(boot_device))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            } else {
                warn!(
                    "cannot find boot mount point on root device: {}, path {}",
                    root_device.display(),
                    boot_path.display()
                );
                (None, None)
            };


        // mount bootmgr partition (EFI, uboot)

        let bootmgr_mount =
            if let Some(bootmgr_mount) = stage2_cfg.get_bootmgr_mount() {
                let device = to_std_device_path(bootmgr_mount.get_device())?;
                let mounted =
                    if let Some(boot_device) = boot_device {
                        device == boot_device
                    }  else {
                        false
                    };

                if !mounted && device != root_device {
                    // TODO: sort out boot manager mountpoint for windows
                    // create 'virtual' mount point in windows and adjust boot backup paths appropriately as
                    // mountpoint D: for EFI backup will no t work
                    // maybe try /boot/ EFI
                    let mountpoint = path_append(&root_fs_dir, bootmgr_mount.get_mountpoint());
                    debug!(
                        "attempting to mount '{}' on '{}' with fstype: {}",
                        device.display(),
                        mountpoint.display(),
                        bootmgr_mount.get_fstype()
                    );
                    mount(
                        Some(&device),
                        &mountpoint,
                        Some(bootmgr_mount.get_fstype()),
                        MsFlags::empty(),
                        NIX_NONE,
                    )
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to mount previous bootmanager device '{}' to '{}' with fstype: {}",
                            device.display(),
                            mountpoint.display(),
                            bootmgr_mount.get_fstype()
                        ),
                    ))?;
                    Some(mountpoint)
                } else {
                    None
                }
            } else {
              None
            };

*/

        unimplemented!()
    }

    fn find_boot_mount(
        config_path: &'a PathBuf,
        boot_mount: &PathBuf,
        boot_fs_type: &Option<String>,
    ) -> Option<PathBuf> {
        let devices = match read_dir("/dev/") {
            Ok(devices) => devices,
            Err(_why) => {
                return None;
            }
        };

        let fstypes: Vec<&str> = if let Some(fstype) = boot_fs_type {
            vec![fstype]
        } else {
            vec!["ext4", "vfat", "ntfs", "ext2", "ext3"]
        };

        for device in devices {
            if let Ok(device) = device {
                if let Ok(ref file_type) = device.file_type() {
                    if file_type.is_file() {
                        let file_name = String::from(device.file_name().to_string_lossy());
                        debug!(
                            "Looking at '{}' -> '{}'",
                            device.path().display(),
                            file_name
                        );
                        if file_name.starts_with("sd")
                            || file_name.starts_with("hd")
                            || file_name.starts_with("mmcblk")
                            || file_name.starts_with("nvme")
                        {
                            debug!("Trying to mount '{}'", device.path().display());
                            for fstype in &fstypes {
                                debug!(
                                    "Attempting to mount '{}' with '{}'",
                                    device.path().display(),
                                    fstype
                                );
                                if let Ok(_s) = mount(
                                    Some(&device.path()),
                                    boot_mount.as_path(),
                                    Some(fstype.as_bytes()),
                                    MsFlags::empty(),
                                    NIX_NONE,
                                ) {
                                    debug!(
                                        "'{}' mounted ok with '{}' looking for ",
                                        device.path().display(),
                                        config_path.display()
                                    );
                                    if file_exists(config_path) {
                                        return Some(device.path());
                                    } else {
                                        let _res = umount(&device.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }
}