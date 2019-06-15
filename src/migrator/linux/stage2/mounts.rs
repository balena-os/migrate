use std::path::{PathBuf, Path};
use std::fs::{create_dir, read_dir, read_to_string};
use log::{info, warn, debug};
use regex::{Regex};
use nix::{
    mount::{mount, umount, MsFlags},
};

use crate::{
    defs::{STAGE2_CFG_FILE, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::{
        linux_common::{get_kernel_root_info, to_std_device_path},
        linux_defs::{BOOT_PATH, NIX_NONE, KERNEL_CMDLINE_PATH, }
    },
    common::{
        dir_exists, file_exists, path_append,
        MigError, MigErrorKind, MigErrCtx,
        stage2_config::{
            MountConfig,
            PathType,
        }
    }
};
use nix::mount::umount;
use crate::common::stage2_config::Stage2Config;

const ROOTFS_DIR: &str = "/tmp_root";


pub(crate) struct Mounts {
    root_mountpoint: PathBuf,
    root_device: PathBuf,
    stage2_config: PathBuf,
    boot_path: Option<PathBuf>,
    boot_device: Option<PathBuf>,
    bootmgr_path: Option<PathBuf>,
    bootmgr_device: Option<PathBuf>,
    work_path: Option<PathBuf>,
    work_device: Option<PathBuf>,
}


impl<'a> Mounts {
    pub fn new() -> Result<Mounts, MigError> {
        let root_mountpoint = PathBuf::from(ROOTFS_DIR);
        let stage2_config = path_append(&root_mountpoint, STAGE2_CFG_FILE);

        let (kernel_root_device, kernel_root_fs_type) = Mounts::get_kernel_root_info()?;

        info!(
            "Using root device '{}' with fs-type: '{:?}'",
            root_device.display(),
            kernel_root_fs_type
        );

        if !dir_exists(&root_mountpoint)? {
            create_dir(&root_mountpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create mountpoint for roofs in {}",
                    &root_mountpoint.display()
                ),
            ))?;
        } else {
            warn!("root mount directory {} exists", &root_mountpoint.display());
        }


        // try find root from kernel cmd line
        let mut root_device =
            if file_exists(&kernel_root_device) {
                debug!(
                    "mounting root device '{}' on '{}' with fs type: {:?}",
                    kernel_root_device.display(),
                    root_mountpoint.display(),
                    kernel_root_fs_type
                );
                mount(
                    Some(&kernel_root_device),
                    &root_mountpoint,
                    if let Some(ref fs_type) = root_fs_type {
                        Some(kernel_root_fs_type.as_bytes())
                    } else {
                        NIX_NONE
                    },
                    MsFlags::empty(),
                    NIX_NONE,
                )
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to mount previous root device '{}' to '{}' with type: {:?}",
                            &kernel_root_device.display(),
                            &root_mountpoint.display(),
                            kernel_root_fs_type
                        ),
                    ))?;

                debug!("looking for '{}'", stage2_config.display());

                if !file_exists(&stage2_config) {
                    let message = format!(
                        "failed to locate stage2 config in {}",
                        stage2_config.display()
                    );
                    error!("{}", &message);

                    umount(&root_mountpoint)
                        .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to unmount from: '{}'", root_mountpoint.display())))?;
                    None
                } else {
                    Some(kernel_root_device)
                }
            } else {
                None
            };

        if let None = root_device {
            root_device = Mounts::find_root_mount(&stage2_config, &root_mountpoint, &kernel_root_fs_type);
        }

        if let Some(root_device) = root_device {
            Ok(Mounts{
                root_device,
                root_mountpoint,
                stage2_config,
                boot_path: None,
                boot_device: None,
                bootmgr_path: None,
                bootmgr_device: None,
                work_path: None,
                work_device: None,
            })
        } else {
            error!("Failed to find a device containing the stage2 config. Giving up");
            Err(MigError::displayed())
        }
    }

    pub fn get_root_mountpoint(&'a self) -> &'a Path {
        &self.root_mountpoint
    }

    pub fn get_stage2_config(&'a self) -> &'a Path {
        &self.stage2_config
    }

    pub fn mount_all(&mut self, stage2_config: Stage2Config) -> Result<(),MigError> {

        if let Some(mount_cfg) = stage2_config.get_boot_mount() {
            let device = to_std_device_path(mount_cfg.get_device())?;
            if self.root_device != device {
                let mountpoint = path_append(&self.root_mountpoint, mount_cfg.get_mountpoint());
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
                            "Failed to mount previous boot device '{}' to '{}' with fstype: {:?}",
                            &device.display(),
                            &mountpoint.display(),
                            mount_cfg.get_fstype()
                        ),
                    ))?;

                self.boot_path = Some(mountpoint);
                self.boot_device = Some(device);
            }
        }

        if let Some(mount_cfg) = stage2_config.get_bootmgr_mount() {
            let device = to_std_device_path(mount_cfg.get_device())?;

            let mounted =
                if let Some(boot_device) = self.boot_device {
                    boot_device == device
                } else {
                    false
                };

            if !mounted && self.root_device != device {
                let mountpoint = path_append(&self.root_mountpoint, mount_cfg.get_mountpoint());
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

                self.bootmgr_path = Some(mountpoint);
                self.bootmgr_device = Some(device);
            }
        }


        match stage2_config.get_work_path() {
            PathType::Path(work_path) => {
                self.work_path = Some(work_path.clone());
            },
            PathType::Mount(mount_cfg) => {
                let device = to_std_device_path(mount_cfg.get_device())?;

                let mounted =
                    if let Some(boot_device) = self.boot_device {
                        boot_device == device
                    } else {
                        false
                    };

                if !mounted && self.root_device != device {
                    let mountpoint = path_append(&self.root_mountpoint, mount_cfg.get_mountpoint());
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

    fn find_root_mount(
        config_path: &'a PathBuf,
        root_mount: &PathBuf,
        root_fs_type: &Option<String>,
    ) -> Option<PathBuf> {
        let devices = match read_dir("/dev/") {
            Ok(devices) => devices,
            Err(_why) => {
                return None;
            }
        };

        let fstypes: Vec<&str> = if let Some(fstype) = root_fs_type {
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
                                    root_mount.as_path(),
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


    /******************************************************************
    * parse /proc/cmdline to extract root device & fs_type
    ******************************************************************/

    fn get_kernel_root_info() -> Result<(PathBuf, Option<String>), MigError> {
        const ROOT_DEVICE_REGEX: &str = r#"\sroot=(\S+)\s"#;
        const ROOT_PARTUUID_REGEX: &str = r#"^PARTUUID=(\S+)$"#;
        const ROOT_UUID_REGEX: &str = r#"^UUID=(\S+)$"#;
        const ROOT_FSTYPE_REGEX: &str = r#"\srootfstype=(\S+)\s"#;

        trace!("get_root_info: entered");

        let cmd_line = read_to_string(KERNEL_CMDLINE_PATH).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to read from file: '{}'", KERNEL_CMDLINE_PATH),
        ))?;

        debug!("get_root_info: got cmdline: '{}'", cmd_line);

        let root_device = if let Some(captures) =
        Regex::new(ROOT_DEVICE_REGEX).unwrap().captures(&cmd_line)
        {
            let root_dev = captures.get(1).unwrap().as_str();
            debug!("Got root device string: '{}'", root_dev);

            if let Some(uuid_part) =
            if let Some(captures) = Regex::new(ROOT_PARTUUID_REGEX).unwrap().captures(root_dev) {
                debug!("Got root device PARTUUID: {:?}", captures.get(1));
                Some(path_append(
                    DISK_BY_PARTUUID_PATH,
                    captures.get(1).unwrap().as_str(),
                ))
            } else {
                if let Some(captures) = Regex::new(ROOT_UUID_REGEX).unwrap().captures(root_dev) {
                    debug!("Got root device UUID: {:?}", captures.get(1));
                    Some(path_append(
                        DISK_BY_UUID_PATH,
                        captures.get(1).unwrap().as_str(),
                    ))
                } else {
                    None
                }
            }
            {
                debug!("trying device path: '{}'", uuid_part.display());
                to_std_device_path(&uuid_part)?
            } else {
                debug!("Got plain root device '{}'", root_dev);
                PathBuf::from(root_dev)
            }
        } else {
            warn!("Got no root was found in kernel command line '{}'", cmd_line);
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "Failed to parse root device path from kernel command line: '{}'",
                    cmd_line
                ),
            ));
        };

        debug!("Using root device: '{}'", root_device.display());

        let root_fs_type =
            if let Some(captures) = Regex::new(&ROOT_FSTYPE_REGEX).unwrap().captures(&cmd_line) {
                Some(String::from(captures.get(1).unwrap().as_str()))
            } else {
                warn!("failed to parse {} for root fs type", cmd_line);
                None
            };

        Ok((root_device, root_fs_type))
    }
}