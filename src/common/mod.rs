//pub mod mig_error;
use log::{trace};
use std::process::{Command,ExitStatus,Stdio};
use failure::{ResultExt};

use crate::mig_error::{MigError,MigErrorKind, MigErrCtx};


const MODULE: &str  = "common";

pub(crate) struct CmdRes {
    pub stdout: String,
    pub stderr: String,
    pub status: ExitStatus,
}

pub(crate) fn call(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    trace!(
        "{}::call(): '{}' called with {:?}, {}",
        MODULE,
        cmd,
        args,
        trim_stdout
    );

    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::call: failed to execute: command {} '{:?}'",
                MODULE, cmd, args
            ),
        ))?;

    Ok(CmdRes {
        stdout: match trim_stdout {
            true => String::from(String::from_utf8_lossy(&output.stdout).trim()),
            false => String::from(String::from_utf8_lossy(&output.stdout)),
        },
        stderr: String::from(String::from_utf8_lossy(&output.stderr)),
        status: output.status,
    })
}
