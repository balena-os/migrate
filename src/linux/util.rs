use failure::{Fail, ResultExt};
use log::trace;
use regex::Regex;
use std::fs::read_to_string;
use std::io::Read;

const MODULE: &str = "Linux::util";
const WHEREIS_CMD: &str = "whereis";

use crate::common::call;
use crate::{MigErrCtx, MigError, MigErrorKind};

pub fn parse_file(fname: &str, regex: &Regex) -> Result<String, MigError> {
    let os_info = std::fs::read_to_string(fname).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("File read '{}'", fname),
    ))?;

    for line in os_info.lines() {
        trace!("{}::parse_file: line: '{}'", MODULE, line);

        if let Some(cap) = regex.captures(line) {
            return Ok(String::from(cap.get(1).unwrap().as_str()));
        };
    }

    Err(MigError::from(MigErrorKind::NotFound))
}

pub fn file_exists(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
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
