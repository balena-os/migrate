use failure::{ResultExt};
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

use serde_json::{Result as SResult, Value};

use super::LinuxMigrator;
use super::util::{dir_exists};

const MODULE: &str = "balena_migrate::migrator::linux::partition_info";
const FINDMNT_CMD: &str = "findmnt";

const DF_CMD: &str = "df";
const LSBLK_CMD: &str = "lsblk";

const SIZE_REGEX: &str = r#"^(\d+)K?$"#;
const LSBLK_REGEX: &str = r#"^(\S+)\s+(\d+)\s+(\S+)\s+(\S+)(\s+(.*))?$"#;

#[derive(Debug)]
pub(crate) struct PartitionInfo {    
    pub path: String,
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
    fn default(path: &str) -> PartitionInfo {
        PartitionInfo {
            path: String::from(path),
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

        let mut result = PartitionInfo::default(path);

        
        let args: Vec<&str> = vec![    
            "--block-size=K",
            "--output=source,size,used",
            path];
        
        let cmd_res = migrator.call_cmd(DF_CMD, &args, true)?;

        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to find mountpoint for {}", MODULE, path)));
        }

        let output: Vec<&str> = cmd_res.stdout.lines().collect();
        if output.len() != 2 {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse mountpoint attributes for {}", MODULE, path)));
        }
        
        debug!("PartitionInfo::new: '{}' df result: {:?}", path, &output[1]);

        let words: Vec<&str> = output[1].split_whitespace().collect();
        if words.len() != 3 {
            debug!("PartitionInfo::new: '{}' df result: words {}", path, words.len());
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse mountpoint attributes for {}", MODULE, path)));
        }

        debug!("PartitionInfo::new: '{}' df result: {:?}", path, &words);

        result.device = String::from(words[0]);

        result.fs_size = if let Some(captures) = SIZE_RE.captures(words[1]) {
            captures.get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::new: failed to parse size from {} ", MODULE, words[1])))? * 1024
        }  else {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::new: failed to parse size from {} ", MODULE, words[1])));
        };    
        

        let fs_used = if let Some(captures) = SIZE_RE.captures(words[2]) {
            captures.get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::new: failed to parse size from {} ", MODULE, words[2])))? * 1024
        }  else {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::new: failed to parse size from {} ", MODULE, words[2])));
        };   

        result.fs_free = result.fs_size - fs_used;

        let args: Vec<&str> = vec![    
            "-b",
            "--output=FSTYPE,SIZE,UUID,PARTUUID,PARTLABEL",
            "--json",
            &result.device];

        // TODO: use distinct calls for UUIDs or --json format to tolerate missing/empty UUIDs

        let cmd_res = migrator.call_cmd(LSBLK_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::ExecProcess , &format!("{}::new: failed to determine mountpoint attributes for {}", MODULE, path)));
        }

        debug!("PartitionInfo::new: '{}' lsblk result: '{}'", path, &cmd_res.stdout);
        let parse_res: Value = serde_json::from_str(&cmd_res.stdout)
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::new: failed to parse lsblk json output: '{}'", MODULE, &cmd_res.stdout)))?;

        
        if let Some(ref devs) = parse_res.get("blockdevices") {
            if let Some(device) = devs.get(0) {                
                
                if let Some(ref val) = device.get("fstype") {
                    debug!("fs_type res: {:?}", val);
                    if let Value::String(ref s) = val {
                        debug!("fs_type res: {:?}", s);
                        result.fs_type = String::from(s.as_ref());
                    }
                }

                if let Some(ref val) = device.get("size") {
                    if let Value::String(ref s) = val {
                        result.part_size = s.parse::<u64>()
                            .context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::new: failed to parse size from {}", MODULE,s)))?;
                    }
                } 

                if let Some(ref val) = device.get("uuid") {
                    if let Value::String(ref s) = val {
                        result.uuid = String::from(s.as_ref());
                    }
                }

                if let Some(ref val) = device.get("partuuid") {
                    if let Value::String(ref s) = val {
                        result.part_uuid = String::from(s.as_ref());
                    }
                }

                if let Some(ref val) = device.get("partlabel") {
                    if let Value::String(ref s) = val {
                        result.part_label = String::from(s.as_ref());
                    }
                }
            }
        }   

        debug!("PartitionInfo::new: '{}' lsblk result: '{:?}'", path, result);
        if result.fs_type.is_empty() || result.part_size == 0 {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse block device attributes for {} from {}", MODULE, path, &cmd_res.stdout))); 
        }

        Ok(Some(result))
    }
}

impl Display for PartitionInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f, 
            "path: {} device: {}, uuid: {}, fstype: {}, size: {}, fs_size: {}, fs_free: {}", 
            self.path, 
            self.device, 
            self.uuid, 
            self.fs_type, 
            format_size_with_unit(self.part_size), 
            format_size_with_unit(self.fs_size), 
            format_size_with_unit(self.fs_free))
    }
}
