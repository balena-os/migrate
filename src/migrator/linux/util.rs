use log::{error, trace, debug};
use regex::Regex;

use failure::{ResultExt};

use crate::common::{MigError,MigErrCtx, MigErrorKind, OSArch};
use crate::linux_common::{
    call_cmd, 
    UNAME_CMD, 
    GRUB_INSTALL_CMD, 
    MOKUTIL_CMD,
    };

const GRUB_INST_VERSION_ARGS: [&str; 1] = ["--version"];
const GRUB_INST_VERSION_RE: &str = r#"^.*\s+\(GRUB\)\s+([0-9]+)\.([0-9]+)[^0-9].*$"#;

const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

const MODULE: &str = "balena_migrator::linux::util";

pub(crate) fn get_grub_version() -> Result<(String, String), MigError> {
    trace!("LinuxMigrator::get_grub_version: entered");
    let cmd_res = call_cmd(GRUB_INSTALL_CMD, &GRUB_INST_VERSION_ARGS, true).context(
        MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("{}::get_grub_version: call {}", MODULE, UNAME_CMD),
        ),
    )?;

    if cmd_res.status.success() {
        let re = Regex::new(GRUB_INST_VERSION_RE).unwrap();
        if let Some(captures) = re.captures(cmd_res.stdout.as_ref()) {
            Ok((
                String::from(captures.get(1).unwrap().as_str()),
                String::from(captures.get(2).unwrap().as_str()),
            ))
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::get_grub_version: failed to parse grub version string: {}",
                    MODULE, cmd_res.stdout
                ),
            ))
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "{}::get_os_arch: command failed: {}",
                MODULE,
                cmd_res.status.code().unwrap_or(0)
            ),
        ))
    }
}

pub(crate) fn is_secure_boot() -> Result<bool, MigError> {
    trace!("LinuxMigrator::is_secure_boot: entered");
    let cmd_res = match call_cmd(MOKUTIL_CMD, &MOKUTIL_ARGS_SB_STATE, true) {
        Ok(cr) => {
            debug!("{}::is_secure_boot: {} -> {:?}", MODULE, MOKUTIL_CMD, cr);
            cr
        }
        Err(why) => {
            debug!("{}::is_secure_boot: {} -> {:?}", MODULE, MOKUTIL_CMD, why);
            match why.kind() {
                MigErrorKind::NotFound => {
                    return Ok(false);
                }
                _ => {
                    return Err(why);
                }
            }
        }
    };

    let regex = Regex::new(r"^SecureBoot\s+(disabled|enabled)$").unwrap();
    let lines = cmd_res.stdout.lines();
    for line in lines {
        if let Some(cap) = regex.captures(line) {
            if cap.get(1).unwrap().as_str() == "enabled" {
                return Ok(true);
            } else {
                return Ok(false);
            }
        }
    }
    error!(
        "{}::is_secure_boot: failed to parse command output: '{}'",
        MODULE, cmd_res.stdout
    );
    Err(MigError::from_remark(
        MigErrorKind::InvParam,
        &format!("{}::is_secure_boot: failed to parse command output", MODULE),
    ))
}
