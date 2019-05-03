use log::{info, error, warn};
use regex::Regex;
use failure::{ResultExt};
use std::fs::{copy, create_dir, read_dir};
use std::path::{Path, PathBuf};


use crate::common::{
    dir_exists,
    file_exists,    
    STAGE2_CFG_FILE,
    Logger,
    MigError, 
    MigErrCtx,
    MigErrorKind,
    parse_file, 
    Stage2Info, 
    };

use crate::linux_common::{
    ensure_cmds, 
    call_cmd,    
    Device,
    MOUNT_CMD, 
    UMOUNT_CMD,
    DD_CMD,
    REBOOT_CMD, 
    };



pub(crate) mod stage2_config;
pub(crate) use stage2_config::{Stage2Config};

use crate::beaglebone::{BeagleboneGreen};
use crate::raspberrypi::{RaspberryPi3};
use crate::intel_nuc::{IntelNuc};



// for starters just restore old boot config, only required command is mount

// later ensure all other required commands


const KERNEL_CMDLINE: & str = "/proc/cmdline";
const ROOTFS_REGEX: &str = r#"\sroot=(\S+)\s"#;
const ROOTFS_DIR: &str = "/tmp_root";

const MIGRATE_TEMP_DIR: &str = "/migrate_tmp";

const INIT_REQUIRED_CMDS: &'static [&'static str] = &[MOUNT_CMD];
const INIT_OPTIONAL_CMDS: &'static [&'static str] = &[];

const MIG_REQUIRED_CMDS: &'static [&'static str] = &[UMOUNT_CMD, DD_CMD, REBOOT_CMD];
const MIG_OPTIONAL_CMDS: &'static [&'static str] = &[];

const BALENA_IMAGE_FILE: &str = "balenaOS.img.gz";
const BALENA_CONFIG_FILE: &str = "config.json";

const SYSTEM_CONNECTIONS_DIR: &str = "system-connections";

pub(crate) struct Stage2 {
    config: Stage2Config,
    boot_mounted: bool,
}

impl Stage2 {
    pub fn try_init() -> Result<Stage2,MigError> {

        match Logger::initialise(2) {
            Ok(_s) => info!("Balena Migrate Stage 2 initializing"),
            Err(_why) => { println!("Balena Migrate Stage 2 initializing");
                           println!("failed to initalize logger");
            },
        }

        let root_fs_dir = Path::new(ROOTFS_DIR);

        ensure_cmds(INIT_REQUIRED_CMDS, INIT_OPTIONAL_CMDS)?;

        // TODO: beaglebone version - make device_slug dependant
        let root_device = 
            if let Some(parse_res) = parse_file(KERNEL_CMDLINE,&Regex::new(&ROOTFS_REGEX).unwrap())? {
                PathBuf::from(parse_res.get(1).unwrap())
            } else {
                // TODO: manually scan possible devices for config file
                return Err(MigError::from_remark(MigErrorKind::InvState, &format!("failed to parse {} for root device", KERNEL_CMDLINE)));
            };
        
        if ! dir_exists(ROOTFS_DIR)? {
            create_dir(ROOTFS_DIR).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to create mountpoint for roofs in {}", ROOTFS_DIR )))?;
        } else {
            warn!("root mount directory {} exists", ROOTFS_DIR);
        }
            
        // TODO: add options to make this more reliable
        match call_cmd(MOUNT_CMD, &[&root_device.to_string_lossy(), &ROOTFS_DIR] , true) {
            Ok(_s) => { info!("mounted {} on {}", root_device.display(), ROOTFS_DIR); },
            Err(_why) => { 
                error!("failed to mount {} on {}", root_device.display(), ROOTFS_DIR);
                return Err(MigError::from_remark(MigErrorKind::InvState, "could not mount former root file system"));
            }
        }

        let stage2_cfg_file = root_fs_dir.join(STAGE2_CFG_FILE);
        if !file_exists(&stage2_cfg_file) {
            let message = format!("failed to locate stage2 config in {}", stage2_cfg_file.display());
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }
        
        let stage2_cfg = Stage2Config::from_config(&stage2_cfg_file)?;

        info!("Read stage 2 config file from {}", stage2_cfg_file.display());

        // TODO: probably paranoid 
        if root_device != stage2_cfg.get_root_device() {
            let message = format!("The device mounted as root does not match the former root device: {} != {}", root_device.display(), stage2_cfg.get_root_device().display());
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));            
        }

        // Ensure /boot is mounted in ROOTFS_DIR/boot

        let boot_path = root_fs_dir.join("boot");
        if ! dir_exists(&boot_path)? {
            let message = format!("cannot find boot mount point on root device: {}, path {}", root_device.display(), boot_path.display());
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));                            
        }

        let boot_device = stage2_cfg.get_boot_device();
        let mut boot_mounted = false;
        if boot_device != root_device {                        
            match call_cmd(MOUNT_CMD, &[&boot_device.to_string_lossy(), &boot_path.to_string_lossy()] , true) {
                Ok(_s) => { 
                    info!("mounted {} on {}", boot_device.display(), boot_path.display()); 
                    boot_mounted = true;    
                },
                Err(_why) => { 
                    let message = format!("failed to mount {} on {}", boot_device.display(), boot_path.display());
                    error!("{}", &message);
                    return Err(MigError::from_remark(MigErrorKind::InvState, &message));
                }
            }
        }
        
        return Ok(
                Stage2{
                    config: stage2_cfg,
                    boot_mounted,
                    })
    }


    pub fn migrate(&self) -> Result<(), MigError> {
        let device_slug = self.config.get_device_slug();

        let root_fs_dir = Path::new(ROOTFS_DIR);
        let mig_tmp_dir = Path::new(MIGRATE_TEMP_DIR);
        
        info!("migrating '{}'", &device_slug);

        let device =
            match device_slug {
                "beaglebone-green" => {             
                    let device: Box<Device> = Box::new(BeagleboneGreen::new());
                    device
                },
                "raspberrypi-3" => {             
                    let device: Box<Device> = Box::new(RaspberryPi3::new());
                    device
                },
                "intel-nuc" => {             
                    let device: Box<Device> = Box::new(IntelNuc::new());
                    device
                },
                _ => { 
                    let message = format!("unexpected device type: {}", device_slug);
                    error!("{}", &message);                    
                    return Err(MigError::from_remark(MigErrorKind::InvState, &message));
                },
            };

        device.restore_boot(&PathBuf::from(ROOTFS_DIR), &self.config)?;
                
        ensure_cmds(MIG_REQUIRED_CMDS, MIG_OPTIONAL_CMDS)?;                

        if ! dir_exists(mig_tmp_dir)? {
            create_dir(mig_tmp_dir).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to create migrate temp directory {}",MIGRATE_TEMP_DIR)))?;
        }
        
        
        let src = root_fs_dir.join(self.config.get_balena_image());
        let tgt = mig_tmp_dir.join(BALENA_IMAGE_FILE);
        copy(&src, &tgt)
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to copy balena image to migrate temp directory, '{}' -> '{}'", src.display(), tgt.display())))?;

        info!("copied balena OS image to '{}'", tgt.display());
        
        let src = root_fs_dir.join(self.config.get_balena_config());
        let tgt = mig_tmp_dir.join(BALENA_CONFIG_FILE);
        copy(&src, &tgt)
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to copy balena config to migrate temp directory, '{}' -> '{}'", src.display(), tgt.display())))?;

        info!("copied balena OS config to '{}'", tgt.display());

        let src_nwmgr_dir = root_fs_dir.join(self.config.get_work_path()).join(SYSTEM_CONNECTIONS_DIR);
        let tgt_nwmgr_dir = root_fs_dir.join(self.config.get_work_path()).join(SYSTEM_CONNECTIONS_DIR);
        if dir_exists(&src_nwmgr_dir)? {
            if ! dir_exists(&tgt_nwmgr_dir)? {
                create_dir(&tgt_nwmgr_dir)
                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to create systm-connections in migrate temp directory: '{}'", tgt_nwmgr_dir.display())))?;
            }

            let paths = read_dir(&src_nwmgr_dir)
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to list directory '{}'", src_nwmgr_dir.display())))?;

            for path in paths {
                if let Ok(path) = path {                    
                    let src_path = path.path();
                    if src_path.metadata().unwrap().is_file() {
                        let tgt_path = tgt_nwmgr_dir.join(&src_path.file_name().unwrap());
                        copy(&src_path,&tgt_path)
                            .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed copy network manager file to migrate temp directory '{}' -> '{}'", src_path.display(), tgt_path.display())))?;
                        info!("copied network manager config  to '{}'", tgt_path.display());                            
                    }
                } else {
                    return Err(MigError::from_remark(MigErrorKind::Upstream, &format!("Error reading entry from directory '{}'", src_nwmgr_dir.display())));
                }
            }
        }

        info!("Files copied to RAMFS");

        if self.boot_mounted {
            call_cmd(UMOUNT_CMD, &[&self.config.get_boot_device().to_string_lossy()], true)?;    
        }

        call_cmd(UMOUNT_CMD, &[&self.config.get_root_device().to_string_lossy()], true)?;

        info!("Unmounted root file system");

        // TODO: dd it !

        
        Ok(())
    }
}