use failure::{ResultExt};
use log::debug;
use regex::Regex;
use std::fs::read_to_string;
// use std::io::Read;
use std::path::Path;

// use libc::{getuid, sysinfo};

const MODULE: &str = "Linux::util";
const WHEREIS_CMD: &str = "whereis";

use crate::migrator::{
    common::call,
    MigErrCtx, 
    MigError, 
    MigErrorKind,
    };


pub fn parse_file(fname: &str, regex: &Regex) -> Result<Option<String>, MigError> {
    let os_info = read_to_string(fname).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("File read '{}'", fname),
    ))?;

    for line in os_info.lines() {
        debug!("{}::parse_file: line: '{}'", MODULE, line);

        if let Some(cap) = regex.captures(line) {
            return Ok(Some(String::from(cap.get(1).unwrap().as_str())));
        };
    }

    Ok(None)
}

pub fn dir_exists(name: &str) -> Result<bool,MigError> {
    let path = Path::new(name);
    if path.exists()  {
        Ok(path.metadata()
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::dir_exists: failed to retrieve metadata for path: {}", MODULE, name)))?
            .file_type().is_dir())
    } else {
        Ok(false)
    }
}


pub fn file_exists(file: &str) -> bool {
    Path::new(file).exists()    
}

pub fn whereis(cmd: &str) -> Result<String, MigError> {
    let args: [&str; 2] = ["-b", cmd];
    let cmd_res = call(WHEREIS_CMD, &args, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("{}::whereis: failed for '{}'", MODULE, cmd),
    ))?;
    if cmd_res.status.success() {
        if cmd_res.stdout.is_empty() {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::whereis: no command output for {}", MODULE, cmd),
            ))
        } else {
            let mut words = cmd_res.stdout.split(" ");
            if let Some(s) = words.nth(1) {
                Ok(String::from(s))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("{}::whereis: command not found: '{}'", MODULE, cmd),
                ))
                //
            }
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "{}::whereis: command failed for {}: {}",
                MODULE,
                cmd,
                cmd_res.status.code().unwrap_or(0)
            ),
        ))
    }
}

pub fn command_exists(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
}

pub fn exec_command(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
}
