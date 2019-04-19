use crate::migrator::{
    MigError, 
    MigErrorKind,
    };

use super::LinuxMigrator;
use super::util::{dir_exists};

const FINDMNT_CMD: &str = "findmnt";

pub(crate) struct PartitionInfo {    
    mountpoint: String,
    device: String,
    size: u64,
    free: u64,
}

impl PartitionInfo {
    pub fn new(path: &str, migrator: &mut LinuxMigrator) -> Result<PartitionInfo,MigError> {
        if ! dir_exists(path)? {
            return Err(MigError::from(MigErrorKind::NotFound));
        }

        let mut args: Vec<&str> = vec![    
            "--noheadings",
            "--canonicalize",
            "--output",
            "SOURCE",
            path];
        
        let cmd_res = migrator.call_cmd(FINDMNT_CMD, &args, true)?;

        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
