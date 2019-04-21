use failure::{ResultExt};
use std::fmt::{self, Display, Formatter};
use log::{trace, debug, warn};
use regex::Regex;
use lazy_static::lazy_static;
use serde_json::{Value};

use crate::migrator::{    
    MigError, 
    MigErrorKind,
    MigErrCtx,
    common::{format_size_with_unit},
    linux::util::{dir_exists, call_cmd, DF_CMD, MOUNT_CMD, LSBLK_CMD}    
    };



const MODULE: &str = "migrator::linux::partition_info";
//const FINDMNT_CMD: &str = "findmnt";


const SIZE_REGEX: &str = r#"^(\d+)K?$"#;
const LSBLK_REGEX: &str = r#"^(\S+)\s+(\d+)\s+(\S+)\s+(\S+)(\s+(.*))?$"#;

const DRIVE_REGEX: &str = r#"^(/dev/([^/]+/)*.*)p[0-9]+$"#;

const MOUNT_REGEX: &str = r#"^(\S+)\s+on\s+(\S+)\s+type\s+(\S+)\s+\(([^\)]+)\).*$"#;
// /dev/mmcblk0p2 on / type ext4 (rw,noatime,data=ordered)

#[derive(Debug)]
pub(crate) struct PartitionInfo {    
    pub path: String,
    pub device: String,
    pub drive: String,
    pub fs_type: String,
    pub mount_opts: String,
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
            drive: String::from(""),
            fs_type: String::from(""),
            mount_opts: String::from(""),
            uuid: String::from(""),
            part_uuid: String::from(""),
            part_label: String::from(""),    
            part_size: 0,
            fs_size: 0,
            fs_free: 0,
        }
    }

    pub fn new(path: &str) -> Result<Option<PartitionInfo>,MigError> {
        trace!("PartitionInfo::new: entered with: '{}'", path);

        if ! dir_exists(path)? {           
            return Ok(None);             
        }

        lazy_static! {
            static ref LSBLK_RE: Regex = Regex::new(LSBLK_REGEX).unwrap();
            static ref SIZE_RE: Regex = Regex::new(SIZE_REGEX).unwrap();
            static ref DRIVE_RE: Regex = Regex::new(DRIVE_REGEX).unwrap();            
            static ref MOUNT_RE: Regex = Regex::new(MOUNT_REGEX).unwrap();
        }

        let mut result = PartitionInfo::default(path);

        
        let args: Vec<&str> = vec![    
            "--block-size=K",
            "--output=source,size,used",
            path];
        
        let cmd_res = call_cmd(DF_CMD, &args, true)?;

        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to find mountpoint for {}", MODULE, path)));
        }

        let output: Vec<&str> = cmd_res.stdout.lines().collect();
        if output.len() != 2 {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse mountpoint attributes for {}", MODULE, path)));
        }
        
        // debug!("PartitionInfo::new: '{}' df result: {:?}", path, &output[1]);

        let words: Vec<&str> = output[1].split_whitespace().collect();
        if words.len() != 3 {
            debug!("PartitionInfo::new: '{}' df result: words {}", path, words.len());
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to parse mountpoint attributes for {}", MODULE, path)));
        }

        debug!("PartitionInfo::new: '{}' df result: {:?}", path, &words);

        let args: Vec<&str> = vec![];
        let cmd_res = call_cmd(MOUNT_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed to find mountpoint for {}", MODULE, path)));
        }

        let mut found = false;    
        for mount in cmd_res.stdout.lines() {    
            debug!("looking at '{}'", mount);
            if let Some(captures) = MOUNT_RE.captures(mount) {
                if words[0] == "/dev/root" {                    
                    // look for root mount
                    if captures.get(2).unwrap().as_str() == "/" {
                        result.device = String::from(captures.get(1).unwrap().as_str());                      
                    } else {
                        continue;
                    }
                } else {
                    if captures.get(1).unwrap().as_str() == words[0] {
                        result.device = String::from(words[0]);                      
                    } else {
                        continue;
                    }
                }

                result.fs_type = String::from(captures.get(3).unwrap().as_str());                      
                result.mount_opts = String::from(captures.get(4).unwrap().as_str());                      

                found = true;
                break;
            } else {
                warn!("unable to parse mount '{}'", mount);
            }
        }

        if ! found {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::new: failed to find device in mounts: '{}' ", MODULE, words[0])));
        }

        if let Some(captures) = DRIVE_RE.captures(&result.device) {
            result.drive = String::from(captures.get(1).unwrap().as_str());
        } else {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::new: cannot derive disk device from partition {}", MODULE, result.device)));
        }

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
            "--output=SIZE,UUID,PARTUUID,PARTLABEL",
            "--json",
            &result.device];

        // TODO: use distinct calls for UUIDs or --json format to tolerate missing/empty UUIDs

        let cmd_res = call_cmd(LSBLK_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::ExecProcess , &format!("{}::new: failed to determine mountpoint attributes for {}", MODULE, path)));
        }

        debug!("PartitionInfo::new: '{}' lsblk result: '{}'", path, &cmd_res.stdout);
        let parse_res: Value = serde_json::from_str(&cmd_res.stdout)
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::new: failed to parse lsblk json output: '{}'", MODULE, &cmd_res.stdout)))?;

        
        if let Some(ref devs) = parse_res.get("blockdevices") {
            if let Some(device) = devs.get(0) {                
                
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
