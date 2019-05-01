use std::env;
use std::fs::{create_dir, remove_file};
use log::{info, error, warn};
use regex::Regex;
use failure::{ResultExt};

use crate::common::{
    dir_exists,
    file_exists,
    STAGE2_CFG_FILE,
    Logger,
    MigError, 
    MigErrCtx,
    MigErrorKind,
    parse_file};

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

// for starters just restore old boot config, only required command is mount
const REQUIRED_CMDS1: &'static [&'static str] = &[MOUNT_CMD];
const OPTIONAL_CMDS1: &'static [&'static str] = &[];

// later ensure all other required commands
const REQUIRED_CMDS2: &'static [&'static str] = &[LSBLK_CMD, UNAME_CMD, REBOOT_CMD];
const OPTIONAL_CMDS2: &'static [&'static str] = &[];

const KERNEL_CMDLINE: & str = "/proc/cmdline";
const ROOTFS_REGEX: &str = r#"\sroot=(\S+)\s"#;
const ROOTFS_DIR: &str = "/tmp_root";

const UENV_FILE: &str = "/uEnv.txt";


pub struct Stage2 {
    
}

impl Stage2 {
    pub fn try_init() -> Result<(),MigError> {

        match Logger::initialise(2) {
            Ok(_s) => info!("Balena Migrate Stage 2 initializing"),
            Err(_why) => { println!("Balena Migrate Stage 2 initializing");
                           println!("failed to initalize logger");
            },
        }

        ensure_cmds(REQUIRED_CMDS1, OPTIONAL_CMDS1)?;

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
                    return Err(MigError::from_remark(MigErrorKind::InvState, "could not mount former root file system, giving up "));
                }
            }

            let stage2_cfg_file = format!("{}{}", ROOTFS_DIR, STAGE2_CFG_FILE);
            if !file_exists(&stage2_cfg_file) {
                let message = format!("failed to locate stage2 config in {}, giving up", &stage2_cfg_file);
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }

            let mig_boot_cfg = format!("{}{}",ROOTFS_DIR, UENV_FILE);
            if file_exists(&mig_boot_cfg) {
                remove_file(&mig_boot_cfg).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to remove migrate boot config file {}", &mig_boot_cfg )))?;            
            } else {
                error!("failed to parse {} for root device", KERNEL_CMDLINE);
            }

        ensure_cmds(REQUIRED_CMDS2, OPTIONAL_CMDS2)?;

        // call_cmd(&LSBLK_CMD, &[""], true)?;

        
        // **********************************************************************
        // try to establish and mount former root file system


        Ok(())
    }



}