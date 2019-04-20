use failure::{Fail, ResultExt};
use std::fmt::{self, Display, Formatter};
use log::{trace, debug};
use regex::Regex;
use lazy_static::lazy_static;

use crate::migrator::{
    MigError, 
    MigErrorKind,
    MigErrCtx,
    common::{format_size_with_unit},
    };

use super::LinuxMigrator;
use super::util::{dir_exists};

const MODULE: &str = "balena_migrate::migrator::linux::partition_info";
const FINDMNT_CMD: &str = "findmnt";

const DF_CMD: &str = "df";
const LSBLK_CMD: &str = "lsblk";

const SIZE_REGEX: &str = r#"^(\d+)K$"#;
const LSBLK_REGEX: &str = r#"^(\S+)\s+(\d+)\s+(\S+)\s+(\S+)(\s+(.*))?$"#;

#[derive(Debug)]
pub(crate) struct PartitionInfo {    
    pub mountpoint: String,
    pub device: String,
    pub fs_type: String,
    pub uuid: String,
    pub part_uuid: String,
    pub part_label: String,    
    pub part_size: u64,
    pub fs_size: u64,
    pub fs_free: u64,
}

impl PartitionInfo {    
    fn default(mountpoint: &str) -> PartitionInfo {
        PartitionInfo {
            mountpoint: String::from(mountpoint),
            device: String::from(""),
            fs_type: String::from(""),
            uuid: String::from(""),
            part_uuid: String::from(""),
            part_label: String::from(""),    
            part_size: 0,
            fs_size: 0,
            fs_free: 0,

        }
    }

    pub fn new(path: &str, migrator: &mut LinuxMigrator) -> Result<Option<PartitionInfo>,MigError> {
        trace!("PartitionInfo::new: entered with: '{}'", path);

        if ! dir_exists(path)? {           
            return Ok(None);             
        }

        lazy_static! {
            static ref LSBLK_RE: Regex = Regex::new(LSBLK_REGEX).unwrap();
            static ref SIZE_RE: Regex = Regex::new(SIZE_REGEX).unwrap();
        }

        let args: Vec<&str> = vec![    
            "--noheadings",
            "--canonicalize",
            "--output",
            "SOURCE",
            path];
        
        let cmd_res = migrator.call_cmd(FINDMNT_CMD, &args, true)?;

        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Ok(None);                         
        }
        
        debug!("PartitionInfo::new: '{}' findmnt result: {:?}", path, cmd_res);

        let mut result = PartitionInfo::default(path);

        result.device = String::from(cmd_res.stdout);

        let args: Vec<&str> = vec![    
            "-b",
            "--output=FSTYPE,SIZE,UUID,PARTUUID,PARTLABEL",
            &result.device];

        // TODO: use distinct calls for UUIDs or --json format to tolerate missing/empty UUIDs

        let cmd_res = migrator.call_cmd(LSBLK_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::ExecProcess , &format!("{}::new: failed to determine mountpoint attributes for {}", MODULE, path)));
        }

        let output: Vec<&str> = cmd_res.stdout.lines().collect();
        if output.len() != 2 {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse block device attributes for {}", MODULE, path)));
        }

        debug!("PartitionInfo::new: '{}' lsblk result: '{}'", path, &output[1]);

        if let Some(captures) = LSBLK_RE.captures(&output[1]) {
            result.fs_type = String::from(captures.get(1).unwrap().as_str());
            let size_str = captures.get(2).unwrap().as_str();
            result.part_size = size_str.parse::<u64>().context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::new: failed to parse size from {}", MODULE,size_str)))?;
            result.uuid = String::from(captures.get(3).unwrap().as_str());
            result.part_uuid = String::from(captures.get(4).unwrap().as_str());
            if let Some(cap) = captures.get(6) {
                result.part_label = String::from(cap.as_str());    
            }                        
        } else {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse block device attributes for {} from {}", MODULE, path, &output[1])));
        }

        let args: Vec<&str> = vec![    
            "-BK",
            "--output=size,used",
            path];

        let cmd_res = migrator.call_cmd(DF_CMD, &args, true)?;
        
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::ExecProcess , &format!("{}::new: failed to determine mountpoint attributes for {}", MODULE, path)));
        }

        let output: Vec<&str> = cmd_res.stdout.lines().collect();
        if output.len() != 2 {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse mountpoint attributes for {}", MODULE, path)));
        }
        
        debug!("PartitionInfo::new: '{}' df result: {:?}", path, &output[1]);

        let words: Vec<&str> = output[1].split_whitespace().collect();
        if words.len() != 2 {
            debug!("PartitionInfo::new: '{}' df result: words {}", path, words.len());
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse mountpoint attributes for {}", MODULE, path)));
        }

        debug!("PartitionInfo::new: '{}' df result: {:?}", path, &words);

        result.fs_size = if let Some(captures) = SIZE_RE.captures(words[0]) {
            captures.get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::new: failed to parse size from {} ", MODULE, words[0])))? * 1024
        }  else {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::new: failed to parse size from {} ", MODULE, words[0])));
        };    
        

        let fs_used = if let Some(captures) = SIZE_RE.captures(words[1]) {
            captures.get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::new: failed to parse size from {} ", MODULE, words[1])))? * 1024
        }  else {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::new: failed to parse size from {} ", MODULE, words[1])));
        };   

        result.fs_free = result.fs_size - fs_used;

        Ok(Some(result))
    }
}

impl Display for PartitionInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f, 
            "Mountpoint: {} device: {}, uuid: {}, fstype: {}, size: {}, fs_size: {}, fs_free: {}", 
            self.mountpoint, 
            self.device, 
            self.uuid, 
            self.fs_type, 
            format_size_with_unit(self.part_size), 
            format_size_with_unit(self.fs_size), 
            format_size_with_unit(self.fs_free))
    }
}
