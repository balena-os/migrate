use std::fs::{create_dir, remove_file};
use log::{info, error, warn};
use regex::Regex;
use failure::{ResultExt};
use std::fs::copy;

use crate::common::{
    dir_exists,
    file_exists,
    is_balena_file,
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
    LSBLK_CMD, 
    MOUNT_CMD, 
    UNAME_CMD, 
    REBOOT_CMD, 
    };

mod util;
mod stage2_config;
use stage2_config::{Stage2Config};

// for starters just restore old boot config, only required command is mount

// later ensure all other required commands


const KERNEL_CMDLINE: & str = "/proc/cmdline";
const ROOTFS_REGEX: &str = r#"\sroot=(\S+)\s"#;
const ROOTFS_DIR: &str = "/tmp_root";

const INIT_REQUIRED_CMDS: &'static [&'static str] = &[MOUNT_CMD];
const INIT_OPTIONAL_CMDS: &'static [&'static str] = &[];

const BBG_REQUIRED_CMDS: &'static [&'static str] = &[LSBLK_CMD, UNAME_CMD, REBOOT_CMD];
const BBG_OPTIONAL_CMDS: &'static [&'static str] = &[];


const UENV_FILE: &str = "/uEnv.txt";


pub(crate) struct Stage2 {
    config: Stage2Config,
    root_path: String,
    root_device: String,
    boot_path: String,
}

impl Stage2 {
    pub fn try_init() -> Result<Stage2,MigError> {

        match Logger::initialise(2) {
            Ok(_s) => info!("Balena Migrate Stage 2 initializing"),
            Err(_why) => { println!("Balena Migrate Stage 2 initializing");
                           println!("failed to initalize logger");
            },
        }

        ensure_cmds(INIT_REQUIRED_CMDS, INIT_OPTIONAL_CMDS)?;

        // TODO: beaglebone version - make device_slug dependant
        let root_device = 
            if let Some(parse_res) = parse_file(KERNEL_CMDLINE,&Regex::new(&ROOTFS_REGEX).unwrap())? {
                parse_res.get(1).unwrap().clone()
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
        match call_cmd(MOUNT_CMD, &[&root_device, ROOTFS_DIR] , true) {
            Ok(_s) => { info!("mounted {} on {}", &root_device, ROOTFS_DIR); },
            Err(_why) => { 
                error!("failed to mount {} on {}", &root_device, ROOTFS_DIR);
                return Err(MigError::from_remark(MigErrorKind::InvState, "could not mount former root file system"));
            }
        }

        let stage2_cfg_file = format!("{}{}", ROOTFS_DIR, STAGE2_CFG_FILE);
        if !file_exists(&stage2_cfg_file) {
            let message = format!("failed to locate stage2 config in {}", &stage2_cfg_file);
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }
        
        let stage2_cfg = Stage2Config::from_config(&stage2_cfg_file)?;

        info!("Read stage 2 config file from {}", &stage2_cfg_file);

        // TODO: probably paranoid 
        if root_device != stage2_cfg.get_root_device() {
            let message = format!("The device mounted as root does not match the former root device: {} != {}", root_device, stage2_cfg.get_root_device());
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));            
        }

        // Ensure /boot is mounted in ROOTFS_DIR/boot

        let boot_path = format!("{}/boot", ROOTFS_DIR);
        if ! dir_exists(&boot_path)? {
            let message = format!("cannot find boot mount point on root device: {}, path {}", root_device, boot_path);
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));                            
        }

        let boot_device = stage2_cfg.get_boot_device();
        if boot_device != root_device {                        
            match call_cmd(MOUNT_CMD, &[boot_device, &boot_path] , true) {
                Ok(_s) => { info!("mounted {} on {}", boot_device, boot_path); },
                Err(_why) => { 
                    let message = format!("failed to mount {} on {}", boot_device, boot_path);
                    error!("{}", &message);
                    return Err(MigError::from_remark(MigErrorKind::InvState, &message));
                }
            }
        }
        
        return Ok(
                Stage2{
                    root_path: String::from(ROOTFS_DIR),
                    boot_path: boot_path.clone(),
                    root_device: root_device.clone(),
                    config: stage2_cfg,
                    })
    }


    pub fn migrate(&self) -> Result<(), MigError> {
        let device_slug = self.config.get_device_slug();
        
        info!("migrating '{}'", &device_slug);

        match device_slug {
            "beaglebone-green" => { 
                info!("initializing stage 2 for '{}'", device_slug);
                self.stage2_bbg()
            },
            _ => { 
                let message = format!("unexpected device type: {}", device_slug);
                error!("{}", &message);                    
                Err(MigError::from_remark(MigErrorKind::InvState, &message))
            },
        }
    }

    fn restore_backups(&self) -> Result<(),MigError> {
        // restore boot config backups
        for backup in self.config.get_backups() {
            let src = format!("{}{}",self.root_path, backup.1);
            let tgt = format!("{}{}",self.root_path, backup.0);
            copy(&src,&tgt).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to restore '{}' to '{}'", &src, &tgt)))?;
            info!("Restored '{}' to '{}'", &src, &tgt)
        }

        Ok(())
    }

    fn stage2_bbg(&self) -> Result<(),MigError> {        
        
        let uenv_file = format!("{}{}", &self.root_path, UENV_FILE);

        if file_exists(&uenv_file) && is_balena_file(&uenv_file)? {
            remove_file(&uenv_file)
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to remove migrate boot config file {}", &uenv_file )))?;
            info!("Removed balena boot config file '{}'", &uenv_file);    
        } else {
            warn!("balena boot config file not found in '{}'", &uenv_file);
        }

        self.restore_backups()?;

        info!("The original boot configuration was restored");

        ensure_cmds(BBG_REQUIRED_CMDS, BBG_OPTIONAL_CMDS)?;

        // call_cmd(&LSBLK_CMD, &[""], true)?;

        
        // **********************************************************************
        // try to establish and mount former root file system


        Ok(())
    }

}