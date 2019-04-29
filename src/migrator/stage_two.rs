use crate::common::{MigError};
use crate::linux_common::{
    ensure_cmds, 
    DF_CMD, 
    LSBLK_CMD, 
    MOUNT_CMD, 
    FILE_CMD, 
    UNAME_CMD, 
    REBOOT_CMD, 
    CHMOD_CMD, 
    };

mod util;


const REQUIRED_CMDS: &'static [&'static str] = &[DF_CMD, LSBLK_CMD, MOUNT_CMD, FILE_CMD, UNAME_CMD, REBOOT_CMD, CHMOD_CMD];
const OPTIONAL_CMDS: &'static [&'static str] = &[];


pub struct Stage2 {
    
}

impl Stage2 {
    pub fn try_init() -> Result<(),MigError> {
        
        ensure_cmds(REQUIRED_CMDS, OPTIONAL_CMDS)?;

        
        // **********************************************************************
        // try to establish and mount former root file system




        Ok(())
    }



}