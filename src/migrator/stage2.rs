use std::env;
use std::fs::{create_dir, remove_file};
use log::{info};
use regex::Regex;
use failure::{ResultExt};

use crate::common::{
    dir_exists,
    file_exists,
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
    FILE_CMD, 
    UNAME_CMD, 
    REBOOT_CMD, 
    };

mod util;


// for starters just restore old boot config, only required command is mount
const REQUIRED_CMDS1: &'static [&'static str] = &[MOUNT_CMD];
const OPTIONAL_CMDS1: &'static [&'static str] = &[];

// later ensure all other required commands
const REQUIRED_CMDS2: &'static [&'static str] = &[LSBLK_CMD, FILE_CMD, UNAME_CMD, REBOOT_CMD];
const OPTIONAL_CMDS2: &'static [&'static str] = &[];

const KERNEL_CMDLINE: & str = "/proc/cmdline";
const ROOTFS_REGEX: &str = r#"\sroot=(\S+)\s"#;
const ROOTFS_DIR: &str = "/tmp_root";

const UENV_FILE: &str = "/uEnv.txt";

pub struct Stage2 {
    
}

impl Stage2 {
    pub fn try_init() -> Result<(),MigError> {

/*
        let mut cmd_args: String  = String::from("");
        for param in env::args() {
            cmd_args.push_str(&format!("[{}],", &param));                    
        }
        println!("cmd args: {}", cmd_args);

        let mut env_str: String = String::from("");

        for param in env::vars() {
            env_str.push_str(&format!("'{}'='{}'\n", param.0, param.1));                    
        }

        println!("env_vars: {}", env_str);
*/
        match Logger::initialise(2) {
            Ok(_s) => info!("Balena Migrate Stage 2 initializing"),
            Err(_why) => { println!("Balena Migrate Stage 2 initializing");
                           println!("failed to initalize logger");
            },
        }

        ensure_cmds(REQUIRED_CMDS1, OPTIONAL_CMDS1)?;

        // TODO: beaglebone version - make device_slug dependant
        if let Some(parse_res) = parse_file(KERNEL_CMDLINE,&Regex::new(&ROOTFS_REGEX).unwrap())? {
            let root_device = parse_res.get(1).unwrap(); 
            if ! dir_exists(ROOTFS_DIR)? {
                create_dir(ROOTFS_DIR).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to create mountpoint for roofs in {}", ROOTFS_DIR )))?;
            }
            
            // TODO: add options to make this more reliable
            call_cmd(MOUNT_CMD, &[&root_device, ROOTFS_DIR] , true)?;

            let mig_boot_cfg = format!("{}{}",ROOTFS_DIR, UENV_FILE);
            if file_exists(&mig_boot_cfg) {
                remove_file(&mig_boot_cfg).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to remove migrate boot config file {}", &mig_boot_cfg )))?;
            } 
        }

        ensure_cmds(REQUIRED_CMDS2, OPTIONAL_CMDS2)?;

        // call_cmd(&LSBLK_CMD, &[""], true)?;

        
        // **********************************************************************
        // try to establish and mount former root file system


        Ok(())
    }



}